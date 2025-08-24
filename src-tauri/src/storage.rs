use crate::err_log;
use crate::state::{MongoDB, Options};
use crate::{
    errors::ExportError,
    scoring::AnswerSheetResult,
    signal,
    state::{AnswerSheet, AppState, CsvExport},
};
use log::info;
use opencv::prelude::Mat;
use serde::ser::SerializeStruct;
use serde::Serialize;
use std::{collections::HashMap, fs::File};
use tauri::{ipc::Channel, Emitter, Manager, Runtime};
use tauri_plugin_fs::FilePath;

use mongodb::{options::ClientOptions, Client};

#[allow(non_snake_case)]
#[derive(Debug, Clone)]
pub struct QuestionScoreRow {
    pub subject_id: String,
    pub student_id: String,
    pub subject_name: String,
    pub student_name: String,
    pub exam_room: String,
    pub exam_seat: String,
    questions: Vec<String>,
    total_score: String,
}

impl serde::Serialize for QuestionScoreRow {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("QuestionScoreRow", 36 + 6)?;
        state.serialize_field("subject_id", &self.subject_id)?;
        state.serialize_field("student_id", &self.student_id)?;
        state.serialize_field("subject_name", &self.subject_name)?;
        state.serialize_field("student_name", &self.student_name)?;
        state.serialize_field("exam_room", &self.exam_room)?;
        state.serialize_field("exam_seat", &self.exam_seat)?;

        state.serialize_field("01", &self.questions[0])?;
        state.serialize_field("02", &self.questions[1])?;
        state.serialize_field("03", &self.questions[2])?;
        state.serialize_field("04", &self.questions[3])?;
        state.serialize_field("05", &self.questions[4])?;
        state.serialize_field("06", &self.questions[5])?;
        state.serialize_field("07", &self.questions[6])?;
        state.serialize_field("08", &self.questions[7])?;
        state.serialize_field("09", &self.questions[8])?;
        state.serialize_field("10", &self.questions[9])?;
        state.serialize_field("11", &self.questions[10])?;
        state.serialize_field("12", &self.questions[11])?;
        state.serialize_field("13", &self.questions[12])?;
        state.serialize_field("14", &self.questions[13])?;
        state.serialize_field("15", &self.questions[14])?;
        state.serialize_field("16", &self.questions[15])?;
        state.serialize_field("17", &self.questions[16])?;
        state.serialize_field("18", &self.questions[17])?;
        state.serialize_field("19", &self.questions[18])?;
        state.serialize_field("20", &self.questions[19])?;
        state.serialize_field("21", &self.questions[20])?;
        state.serialize_field("22", &self.questions[21])?;
        state.serialize_field("23", &self.questions[22])?;
        state.serialize_field("24", &self.questions[23])?;
        state.serialize_field("25", &self.questions[24])?;
        state.serialize_field("26", &self.questions[25])?;
        state.serialize_field("27", &self.questions[26])?;
        state.serialize_field("28", &self.questions[27])?;
        state.serialize_field("29", &self.questions[28])?;
        state.serialize_field("30", &self.questions[29])?;
        state.serialize_field("31", &self.questions[30])?;
        state.serialize_field("32", &self.questions[31])?;
        state.serialize_field("33", &self.questions[32])?;
        state.serialize_field("34", &self.questions[33])?;
        state.serialize_field("35", &self.questions[34])?;
        state.serialize_field("36", &self.questions[35])?;
        state.serialize_field("total_score", &self.total_score)?;
        state.end()
    }
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
) -> Result<(), ExportError> {
    let path = path.into_path()?;
    info!("Exporting scanned results to {}...", path.display());
    let file = File::create(path)?;
    let mut wtr = csv::Writer::from_writer(file);

    let results = AppState::get_scored_answers(app).ok_or(ExportError::IncorrectState)?;

    let question_rows = map_to_csv(results.clone());
    let len = question_rows.len();

    for row in &question_rows {
        wtr.serialize(row)?;
    }
    wtr.flush()?;
    info!("Finished exporting to CSV! Written {len} rows.");
    let student_totals = map_to_db_scores(question_rows);

    if let Err(e) = store_scores_in_db(app, student_totals) {
        err_log!(&e);
    }

    Ok(())
}

fn map_to_csv(
    map: HashMap<String, (Mat, AnswerSheet, AnswerSheetResult)>,
) -> Vec<QuestionScoreRow> {
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
                let graded = graded_questions
                    .iter()
                    .map(|(_, w)| w.to_string())
                    .collect::<Vec<_>>();
                let total = graded_questions.iter().map(|(_, w)| *w as u32).sum::<u32>();

                QuestionScoreRow {
                    subject_id: subject_id.clone(),
                    student_id: student_id.clone(),
                    subject_name: subject_name.clone().unwrap_or_default(),
                    student_name: student_name.clone().unwrap_or_default(),
                    exam_room: exam_room.clone().unwrap_or_default(),
                    exam_seat: exam_seat.clone().unwrap_or_default(),
                    questions: graded,
                    total_score: total.to_string(),
                }
            },
        )
        .collect()
}

pub fn map_to_db_scores(question_score_rows: Vec<QuestionScoreRow>) -> Vec<StudentTotalScore> {
    question_score_rows
        .into_iter()
        .map(|row| {
            let mut total: f32 = 0.0;

            // collect all q1..q36 into an array of &String
            for ans in row.questions {
                total += ans.parse::<f32>().unwrap_or(0.0);
            }

            StudentTotalScore {
                subject_id: row.subject_id,
                student_id: row.student_id,
                subject_name: row.subject_name,
                student_name: row.student_name,
                exam_room: row.exam_room,
                exam_seat: row.exam_seat,
                total_score: total,
            }
        })
        .collect()
}

fn store_scores_in_db<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    rows: Vec<StudentTotalScore>,
) -> Result<(), ExportError> {
    if let Options {
        mongo: MongoDB::Enable {
            mongo_db_uri,
            mongo_db_name,
        },
        ..
    } = AppState::get_options(app)
    {
        //println!("MONGODB_URI = {:?}", std::env::var("MONGODB_URI"));
        //println!("MY_DATABASE = {:?}", std::env::var("MY_DATABASE"));

        tauri::async_runtime::block_on(async {
            let options = ClientOptions::parse(&mongo_db_uri).await?;
            let client = Client::with_options(options)?;

            let collection = client
                .database(&mongo_db_name)
                .collection::<StudentTotalScore>("student_total_scores");

            collection.insert_many(rows).await?;
            info!("Inserted total scores into MongoDB Atlas successfully");
            Ok(())
        })
    } else {
        println!("You choose to not user MongoDB");
        Ok(())
    }
}
