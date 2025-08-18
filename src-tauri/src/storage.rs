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
    q1: String,
    q2: String,
    q3: String,
    q4: String,
    q5: String,
    q6: String,
    q7: String,
    q8: String,
    q9: String,
    q10: String,
    q11: String,
    q12: String,
    q13: String,
    q14: String,
    q15: String,
    q16: String,
    q17: String,
    q18: String,
    q19: String,
    q20: String,
    q21: String,
    q22: String,
    q23: String,
    q24: String,
    q25: String,
    q26: String,
    q27: String,
    q28: String,
    q29: String,
    q30: String,
    q31: String,
    q32: String,
    q33: String,
    q34: String,
    q35: String,
    q36: String,
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
                let mut graded = graded_questions.into_iter().map(|(_, w)| w);

                QuestionScoreRow {
                    subject_id: subject_id.clone(),
                    student_id: student_id.clone(),
                    subject_name: subject_name.clone().unwrap_or_default(),
                    student_name: student_name.clone().unwrap_or_default(),
                    exam_room: exam_room.clone().unwrap_or_default(),
                    exam_seat: exam_seat.clone().unwrap_or_default(),
                    q1: graded.next().unwrap_or_default().to_string(),
                    q2: graded.next().unwrap_or_default().to_string(),
                    q3: graded.next().unwrap_or_default().to_string(),
                    q4: graded.next().unwrap_or_default().to_string(),
                    q5: graded.next().unwrap_or_default().to_string(),
                    q6: graded.next().unwrap_or_default().to_string(),
                    q7: graded.next().unwrap_or_default().to_string(),
                    q8: graded.next().unwrap_or_default().to_string(),
                    q9: graded.next().unwrap_or_default().to_string(),
                    q10: graded.next().unwrap_or_default().to_string(),
                    q11: graded.next().unwrap_or_default().to_string(),
                    q12: graded.next().unwrap_or_default().to_string(),
                    q13: graded.next().unwrap_or_default().to_string(),
                    q14: graded.next().unwrap_or_default().to_string(),
                    q15: graded.next().unwrap_or_default().to_string(),
                    q16: graded.next().unwrap_or_default().to_string(),
                    q17: graded.next().unwrap_or_default().to_string(),
                    q18: graded.next().unwrap_or_default().to_string(),
                    q19: graded.next().unwrap_or_default().to_string(),
                    q20: graded.next().unwrap_or_default().to_string(),
                    q21: graded.next().unwrap_or_default().to_string(),
                    q22: graded.next().unwrap_or_default().to_string(),
                    q23: graded.next().unwrap_or_default().to_string(),
                    q24: graded.next().unwrap_or_default().to_string(),
                    q25: graded.next().unwrap_or_default().to_string(),
                    q26: graded.next().unwrap_or_default().to_string(),
                    q27: graded.next().unwrap_or_default().to_string(),
                    q28: graded.next().unwrap_or_default().to_string(),
                    q29: graded.next().unwrap_or_default().to_string(),
                    q30: graded.next().unwrap_or_default().to_string(),
                    q31: graded.next().unwrap_or_default().to_string(),
                    q32: graded.next().unwrap_or_default().to_string(),
                    q33: graded.next().unwrap_or_default().to_string(),
                    q34: graded.next().unwrap_or_default().to_string(),
                    q35: graded.next().unwrap_or_default().to_string(),
                    q36: graded.next().unwrap_or_default().to_string(),
                }
            },
        )
        .fold(vec![], |mut acc, v| {
            acc.push(v);
            acc
        })
}
