use crate::err_log;
use crate::{
    errors::CsvError,
    scoring::{AnswerSheetResult, CheckedAnswer},
    signal,
    state::{AnswerSheet, AppState, CsvExport},
};
use log::{error, info};
use opencv::prelude::Mat;
use serde::Serialize;
use std::{collections::HashMap, fs::File};
use tauri::{ipc::Channel, Emitter, Manager, Runtime};
use tauri_plugin_fs::FilePath;

#[allow(non_snake_case)]
#[derive(Debug, Serialize)]
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
    info!("Exporing scanned results to {}...", path.display());
    let file = File::create(path)?;
    let mut wtr = csv::Writer::from_writer(file);

    let results = AppState::get_scored_answers(app).ok_or(CsvError::IncorrectState)?;
    let rows = map_to_csv(results);
    let len = rows.len();

    for row in rows {
        wtr.serialize(row)?;
    }

    wtr.flush()?;
    info!("Finished Exporting! Written {len} rows.");
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
        .unwrap_or(vec![])
}
