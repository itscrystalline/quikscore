use std::collections::BTreeMap;
use serde::Serialize;
use std::fs::File;
use std::error::Error;
use crate::scoring::CheckedAnswer;

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

pub fn export_to_csv(score: &DetailedScore, path: &str) -> Result<(), Box<dyn Error>> {
    let file = File::create(path)?;
    let mut wtr = csv::Writer::from_writer(file);

    for row in score.to_rows() {
        wtr.serialize(row)?;
    }

    wtr.flush()?;
    Ok(())
}

