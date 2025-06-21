use itertools::Itertools;

use crate::state::{Answer, AnswerKeySheet, AnswerSheet, QuestionGroup};

#[derive(Debug, Clone)]
pub struct AnswerSheetResult {
    pub correct: u32,
    pub incorrect: u32,
    pub not_answered: u32,
    pub graded_questions: [CheckedQuestionGroup; 36],
}

#[allow(non_snake_case)]
#[derive(Debug, Clone, Copy)]
pub struct CheckedQuestionGroup {
    pub A: CheckedAnswer,
    pub B: CheckedAnswer,
    pub C: CheckedAnswer,
    pub D: CheckedAnswer,
    pub E: CheckedAnswer,
}

#[derive(Debug, Clone, Copy)]
pub enum CheckedAnswer {
    Correct,
    Incorrect,
    Missing,
    NotCounted,
}

impl Answer {
    pub fn check_with(curr: Option<Answer>, key: Option<Answer>) -> CheckedAnswer {
        match (curr, key) {
            (Some(curr), Some(key)) => {
                if curr == key {
                    CheckedAnswer::Correct
                } else {
                    CheckedAnswer::Incorrect
                }
            }
            (None, Some(_)) => CheckedAnswer::Incorrect,
            (Some(_), None) | (None, None) => CheckedAnswer::NotCounted,
        }
    }
}

impl QuestionGroup {
    pub fn check_with(&self, key: &Self) -> CheckedQuestionGroup {
        CheckedQuestionGroup {
            A: Answer::check_with(self.A, key.A),
            B: Answer::check_with(self.B, key.B),
            C: Answer::check_with(self.C, key.C),
            D: Answer::check_with(self.D, key.D),
            E: Answer::check_with(self.E, key.E),
        }
    }
}

impl CheckedQuestionGroup {
    /// returns: (correct, incorrect, not_answered)
    fn collect_stats(&self) -> (u32, u32, u32) {
        let (mut correct, mut incorrect, mut not_answered) = (0u32, 0u32, 0u32);
        for ans in [self.A, self.B, self.C, self.D, self.E] {
            match ans {
                CheckedAnswer::Incorrect => incorrect += 1,
                CheckedAnswer::Correct => correct += 1,
                CheckedAnswer::Missing => not_answered += 1,
                CheckedAnswer::NotCounted => (),
            }
        }
        (correct, incorrect, not_answered)
    }
}

impl AnswerSheet {
    pub fn score(&self, key_sheet: &AnswerKeySheet) -> AnswerSheetResult {
        let graded_questions: [CheckedQuestionGroup; 36] = self
            .answers
            .iter()
            .zip(key_sheet.answers.iter())
            .map(|(curr, key)| curr.check_with(key))
            .collect_array()
            .expect("should always be of size 36");

        let (mut correct, mut incorrect, mut not_answered) = (0u32, 0u32, 0u32);
        for qg in graded_questions {
            let (c, i, n) = qg.collect_stats();
            correct += c;
            incorrect += i;
            not_answered += n;
        }

        AnswerSheetResult {
            correct,
            incorrect,
            not_answered,
            graded_questions,
        }
    }
}
