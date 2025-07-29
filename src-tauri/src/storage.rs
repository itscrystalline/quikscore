use crate::{
    errors::CsvError,
    scoring::CheckedAnswer,
    signal,
    state::{AppState, CsvExport},
};
use serde::Serialize;
use std::collections::BTreeMap;
use std::error::Error;
use std::fs::File;
use tauri::{ipc::Channel, Emitter, Manager, Runtime};
use tauri_plugin_fs::FilePath;

#[derive(Debug, Clone)]
pub struct DetailedScore {
    pub details: BTreeMap<String, CheckedAnswer>,
}

#[derive(Debug, Serialize)]
pub struct QuestionScoreRow {
    pub question_id: String,
    pub score: u8,
}

impl DetailedScore {
    pub fn from_result(result: &crate::scoring::AnswerSheetResult) -> Self {
        let mut details = BTreeMap::new();
        let letters = ['A', 'B', 'C', 'D', 'E'];

        for (i, group) in result.graded_questions.iter().enumerate() {
            for (j, &letter) in letters.iter().enumerate() {
                let key = format!("{}{}", i + 1, letter);
                if let Some(ans) = group.at(j) {
                    details.insert(key, ans);
                }
            }
        }

        DetailedScore { details }
    }

    pub fn to_rows(&self) -> Vec<QuestionScoreRow> {
        let mut entries: Vec<_> = self.details.iter().collect();

        entries.sort_by_key(|(key, _)| {
            key.chars()
                .take_while(|c| c.is_ascii_digit())
                .collect::<String>()
                .parse::<u32>()
                .unwrap_or(0)
        });

        entries
            .into_iter()
            .map(|(key, value)| QuestionScoreRow {
                question_id: key.clone(),
                score: match value {
                    CheckedAnswer::Correct => 1,
                    _ => 0,
                },
            })
            .collect()
    }
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
    match export_to_csv_impl(app, path, &channel) {
        Ok(_) => signal!(channel, CsvExport::Done),
        Err(e) => signal!(
            channel,
            CsvExport::Error {
                error: format!("error exporting to CSV: {e}")
            }
        ),
    }
}
pub fn export_to_csv_impl<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    path: FilePath,
    channel: &Channel<CsvExport>,
) -> Result<(), CsvError> {
    let path = path.into_path()?;
    let file = File::create(path)?;
    let mut wtr = csv::Writer::from_writer(file);

    let results = AppState::get_scored_answers(app).ok_or(CsvError::IncorrectState)?;

    // for row in score.to_rows() {
    //     wtr.serialize(row)?;
    // }

    wtr.flush()?;
    Ok(())
}
