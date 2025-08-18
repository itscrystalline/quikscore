use crate::err_log;
use crate::{
    errors::CsvError,
    scoring::{AnswerSheetResult, CheckedAnswer},
    signal,
    state::{AnswerSheet, AppState, CsvExport},
};
use log::{error, info};
use opencv::prelude::Mat;
use serde::{Serialize, Deserialize};
use std::{collections::HashMap, fs::File};
use tauri::{ipc::Channel, Emitter, Manager, Runtime};
use tauri_plugin_fs::FilePath;

use mongodb::{options::ClientOptions, Client};
use dotenvy;

#[allow(non_snake_case)]
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct QuestionScoreRow {
    pub subject_id: String,
    pub student_id: String,
    pub subject_name: String,
    pub student_name: String,
    pub exam_room: String,
    pub exam_seat: String,
    pub question_number: String,
    pub A: String,
    pub B: String,
    pub C: String,
    pub D: String,
    pub E: String,
}

#[derive(Debug, Serialize)]
pub struct StudentTotalScore {
    pub subject_id: String,
    pub student_id: String,
    pub subject_name: String,
    pub student_name: String,
    pub exam_room: String,
    pub exam_seat: String,
    pub total_score: f32,
}

pub fn export_to_csv_wrapper<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    path: Option<FilePath>,
    channel: Channel<CsvExport>,
) {
    let Some(path) = path else {
        signal!(channel, CsvExport::Cancelled);
        return;
    };
    match export_to_csv_impl(app, path) {
        Ok(_) => signal!(channel, CsvExport::Done),
        Err(e) => {
            err_log!(&e);
            signal!(
                channel,
                CsvExport::Error {
                    error: format!("error exporting to CSV: {e}")
                }
            )
        }
    }
}

pub fn export_to_csv_impl<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    path: FilePath,
) -> Result<(), CsvError> {
    let path = path.into_path()?;
    info!("Exporting scanned results to {}...", path.display());
    let file = File::create(path)?;
    let mut wtr = csv::Writer::from_writer(file);

    let results = AppState::get_scored_answers(app).ok_or(CsvError::IncorrectState)?;

    let question_rows = map_to_csv(results.clone());
    let len = question_rows.len();

    for row in question_rows {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    //info!("Finished Exporting! Written {len} rows.");
    let student_totals = map_to_db_scores(results);
    if let Err(e) = store_scores_in_db(student_totals) {
        error!("Failed to store total scores in MongoDB: {}", e);
    }

    Ok(())
}

fn map_to_csv(
    map: HashMap<String, (Mat, AnswerSheet, AnswerSheetResult)>,
) -> Vec<QuestionScoreRow> {
    #[inline(always)]
    fn score_for(ans: CheckedAnswer) -> String {
        match ans {
            CheckedAnswer::Correct(Some(score)) => format!("{score}"),
            CheckedAnswer::Correct(None) => {
                error!("bug: missing score in correct answer, using 1");
                "1".to_string()
            }
            CheckedAnswer::Incorrect => "0".to_string(),
            CheckedAnswer::Missing => "0".to_string(),
            CheckedAnswer::NotCounted => "".to_string(),
        }
    }

    map.into_iter()
        .map(
            |(
                student_id,
                (
                    _,
                    AnswerSheet {
                        subject_id,
                        subject_name,
                        student_name,
                        exam_room,
                        exam_seat,
                        ..
                    },
                    AnswerSheetResult {
                        graded_questions, ..
                    },
                ),
            )| {
                graded_questions
                    .into_iter()
                    .enumerate()
                    .map(|(i, c)| QuestionScoreRow {
                        subject_id: subject_id.clone(),
                        student_id: student_id.clone(),
                        subject_name: subject_name.clone().unwrap_or_default(),
                        student_name: student_name.clone().unwrap_or_default(),
                        exam_room: exam_room.clone().unwrap_or_default(),
                        exam_seat: exam_seat.clone().unwrap_or_default(),
                        question_number: format!("{i:02}"),
                        A: score_for(c.A),
                        B: score_for(c.B),
                        C: score_for(c.C),
                        D: score_for(c.D),
                        E: score_for(c.E),
                    })
                    .collect::<Vec<_>>()
            },
        )
        .reduce(|mut acc, v| {
            acc.extend(v);
            acc
        })
        .unwrap_or_else(Vec::new)
}

pub fn map_to_db_scores(
    map: HashMap<String, (Mat, AnswerSheet, AnswerSheetResult)>,
) -> Vec<StudentTotalScore> {
    #[inline(always)]
    fn score_for(ans: CheckedAnswer) -> f32 {
        match ans {
            CheckedAnswer::Correct(Some(score)) => score.into(),
            CheckedAnswer::Correct(None) => 1.0,
            CheckedAnswer::Incorrect => 0.0,
            CheckedAnswer::Missing => 0.0,
            CheckedAnswer::NotCounted => 0.0,
        }
    }

    map.into_iter()
        .map(|(
            student_id,
            (
                _,
                AnswerSheet {
                    subject_id,
                    subject_name,
                    student_name,
                    exam_room,
                    exam_seat,
                    ..
                },
                AnswerSheetResult {
                    graded_questions, ..
                },
            ),
        )| {
            let total: f32 = graded_questions
                .into_iter()
                .map(|c| {
                    score_for(c.A)
                        + score_for(c.B)
                        + score_for(c.C)
                        + score_for(c.D)
                        + score_for(c.E)
                })
                .sum();

            StudentTotalScore {
                subject_id,
                student_id,
                subject_name: subject_name.unwrap_or_default(),
                student_name: student_name.unwrap_or_default(),
                exam_room: exam_room.unwrap_or_default(),
                exam_seat: exam_seat.unwrap_or_default(),
                total_score: total,
            }
        })
        .collect()
}

fn store_scores_in_db(rows: Vec<StudentTotalScore>) -> Result<(), String> {
    dotenvy::dotenv().ok();
    
    //println!("MONGODB_URI = {:?}", std::env::var("MONGODB_URI"));
    //println!("MY_DATABASE = {:?}", std::env::var("MY_DATABASE"));
    let uri = std::env::var("MONGODB_URI").map_err(|e| e.to_string())?;
    let db_name = std::env::var("MY_DATABASE").map_err(|e| e.to_string())?;

    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    rt.block_on(async {
        let options = ClientOptions::parse(&uri).await.map_err(|e| e.to_string())?;
        let client = Client::with_options(options).map_err(|e| e.to_string())?;

        let collection = client.database(&db_name).collection::<StudentTotalScore>("student_total_scores");

        collection.insert_many(rows).await.map_err(|e| e.to_string())?;
        info!("Inserted total scores into MongoDB Atlas successfully");
        Ok(())
    })
}