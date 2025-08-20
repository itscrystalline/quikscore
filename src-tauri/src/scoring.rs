use crate::err_log;
use std::{collections::HashMap, fs::File, io::BufReader, mem};

use csv::DeserializeRecordsIntoIter;
use itertools::{multizip, Itertools};
use log::{debug, error, warn};
use tauri::{ipc::Channel, Emitter, Manager, Runtime};
use tauri_plugin_fs::FilePath;

use crate::{
    signal,
    state::{Answer, AnswerKeySheet, AnswerSheet, AppState, KeyUpload, NumberType, QuestionGroup},
};

#[derive(Debug, Clone)]
pub struct AnswerSheetResult {
    pub correct: u32,
    pub incorrect: u32,
    pub score: u32,
    pub graded_questions: [(CheckedQuestionGroup, u8); 36],
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

impl CheckedQuestionGroup {
    pub fn at(&self, idx: usize) -> Option<CheckedAnswer> {
        match idx {
            0 => Some(self.A),
            1 => Some(self.B),
            2 => Some(self.C),
            3 => Some(self.D),
            4 => Some(self.E),
            _ => None,
        }
    }
    pub fn verdict(&self) -> CheckedAnswer {
        let mut verdict = CheckedAnswer::NotCounted;
        for ele in [self.A, self.B, self.C, self.D, self.E] {
            verdict = match (verdict, ele) {
                (CheckedAnswer::Correct, CheckedAnswer::Correct) => CheckedAnswer::Correct,
                (CheckedAnswer::Correct, CheckedAnswer::Incorrect) => CheckedAnswer::Incorrect,
                (CheckedAnswer::Correct, CheckedAnswer::Missing) => CheckedAnswer::Missing,
                (CheckedAnswer::Correct, CheckedAnswer::NotCounted) => CheckedAnswer::Correct,
                (CheckedAnswer::Incorrect, _) => CheckedAnswer::Incorrect,
                (CheckedAnswer::Missing, _) => CheckedAnswer::Missing,
                (CheckedAnswer::NotCounted, c) => c,
            };
        }
        verdict
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
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
            (None, Some(_)) => CheckedAnswer::Missing,
            (Some(_), None) | (None, None) => CheckedAnswer::NotCounted,
        }
    }
    pub fn from_bubbles_vec(vec: Vec<u8>) -> Option<Answer> {
        let mut num_type: Option<NumberType> = None;
        let mut num: Option<u8> = None;

        for idx in vec {
            if idx < 3 {
                if num_type.is_none() {
                    num_type = Some(match idx {
                        0 => NumberType::Plus,
                        1 => NumberType::Minus,
                        2 => NumberType::PlusOrMinus,
                        _ => unreachable!(),
                    });
                } else {
                    return None;
                }
            } else if num.is_none() {
                num = Some(idx - 3);
            } else {
                debug!("found double circle");
                return None;
            }
        }
        Some(Answer {
            num_type,
            number: num?,
        })
    }
}

impl QuestionGroup {
    pub fn check_with(&self, key: &Self) -> CheckedQuestionGroup {
        let arr = [
            Answer::check_with(self.A, key.A),
            Answer::check_with(self.B, key.B),
            Answer::check_with(self.C, key.C),
            Answer::check_with(self.D, key.D),
            Answer::check_with(self.E, key.E),
        ];
        #[allow(non_snake_case)]
        let [A, B, C, D, E] = arr;
        CheckedQuestionGroup { A, B, C, D, E }
    }
}

impl CheckedQuestionGroup {}

impl AnswerSheet {
    pub fn score(&self, key_sheet: &AnswerKeySheet, weights: &[u8]) -> AnswerSheetResult {
        let graded_questions: [CheckedQuestionGroup; 36] =
            multizip((self.answers.iter(), key_sheet.answers.iter()))
                .map(|(curr, key)| curr.check_with(key))
                .collect_array()
                .expect("should always be of size 36");

        let (mut correct, mut incorrect, mut score) = (0u32, 0u32, 0u32);
        let graded_questions = graded_questions
            .iter()
            .zip(weights)
            .map(|(qg, weight)| match qg.verdict() {
                CheckedAnswer::Correct => {
                    score += *weight as u32;
                    correct += 1;
                    (*qg, *weight)
                }
                CheckedAnswer::Incorrect | CheckedAnswer::Missing => {
                    incorrect += 1;
                    (*qg, 0)
                }
                CheckedAnswer::NotCounted => (*qg, 0),
            })
            .collect_array()
            .expect("should always be of size 36");

        AnswerSheetResult {
            correct,
            incorrect,
            graded_questions,
            score,
        }
    }
}

#[allow(non_snake_case)]
#[derive(Debug, serde::Deserialize)]
struct RawScoreWeights {
    subject_code: String,
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
impl RawScoreWeights {
    fn weights_into_vec(self) -> Vec<u8> {
        macro_rules! conv {
            ($i: expr) => {
                $i.parse::<u8>().unwrap_or_else(|e| {
                    debug!("Cannot read question answer weight: {e}, using 0 as weight");
                    0
                })
            };
        }
        vec![
            conv!(self.q1),
            conv!(self.q2),
            conv!(self.q3),
            conv!(self.q4),
            conv!(self.q5),
            conv!(self.q6),
            conv!(self.q7),
            conv!(self.q8),
            conv!(self.q9),
            conv!(self.q10),
            conv!(self.q11),
            conv!(self.q12),
            conv!(self.q13),
            conv!(self.q14),
            conv!(self.q15),
            conv!(self.q16),
            conv!(self.q17),
            conv!(self.q18),
            conv!(self.q19),
            conv!(self.q20),
            conv!(self.q21),
            conv!(self.q22),
            conv!(self.q23),
            conv!(self.q24),
            conv!(self.q25),
            conv!(self.q26),
            conv!(self.q27),
            conv!(self.q28),
            conv!(self.q29),
            conv!(self.q30),
            conv!(self.q31),
            conv!(self.q32),
            conv!(self.q33),
            conv!(self.q34),
            conv!(self.q35),
            conv!(self.q36),
        ]
    }
}

#[derive(Default, Debug, Clone, PartialEq, Eq)]
pub struct ScoreWeights {
    pub weights: HashMap<String, (Vec<u8>, u32)>,
}

type WeightsIter<R> = DeserializeRecordsIntoIter<R, RawScoreWeights>;
impl<R: std::io::Read> From<WeightsIter<R>> for ScoreWeights {
    fn from(values: WeightsIter<R>) -> Self {
        let mut weights: HashMap<String, (Vec<u8>, u32)> = HashMap::new();
        for value in values {
            let value_option = match value {
                Ok(ok) => Some(ok),
                Err(e) => {
                    error!("Cannot deserialize score weights: {e}");
                    None
                }
            };

            if let Some(mut raw_weights) = value_option {
                let subject_code = mem::take(&mut raw_weights.subject_code);
                if weights.get_mut(&subject_code).is_none() {
                    let w = raw_weights.weights_into_vec();
                    let sum = w.iter().map(|s| *s as u32).sum();
                    weights.insert(subject_code, (w, sum));
                } else {
                    warn!("Duplicate entry for the same subject ID found. Ignoring.");
                }
            }
        }
        Self { weights }
    }
}
impl ScoreWeights {
    pub fn max_score_deduction(&self, key: &AnswerKeySheet) -> u32 {
        if let Some((weights, _)) = self.weights.get(&key.subject_id) {
            key.answers.iter().zip(weights).fold(0, |acc, (q, w)| {
                acc + if q.A.is_none()
                    && q.B.is_none()
                    && q.C.is_none()
                    && q.D.is_none()
                    && q.E.is_none()
                {
                    *w as u32
                } else {
                    0
                }
            })
        } else {
            0
        }
    }
}

pub fn upload_weights_impl<R: Runtime, A: Emitter<R> + Manager<R>>(
    app: &A,
    path_maybe: Option<FilePath>,
    channel: Channel<KeyUpload>,
) {
    let Some(file_path) = path_maybe else {
        signal!(channel, KeyUpload::Cancelled);
        return;
    };
    let file = match file_path.into_path() {
        Ok(path) => match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                err_log!(&e);
                signal!(
                    channel,
                    KeyUpload::Error {
                        error: format!("Error while opining weights file: {e}")
                    }
                );
                return;
            }
        },
        Err(e) => {
            err_log!(&e);
            signal!(
                channel,
                KeyUpload::Error {
                    error: format!("Error while opening weights file: {e}")
                }
            );
            return;
        }
    };
    let reader = csv::Reader::from_reader(BufReader::new(file));
    AppState::upload_weights(app, &channel, reader.into_deserialize().into());
}

#[cfg(test)]
mod unit_tests {
    use std::array;

    use super::*;
    use crate::state::{Answer, AnswerKeySheet, AnswerSheet, NumberType, QuestionGroup};

    fn answer(num: u8) -> Option<Answer> {
        Some(Answer {
            num_type: Some(NumberType::Plus),
            number: num,
        })
    }

    fn none_answer() -> Option<Answer> {
        None
    }

    #[test]
    fn test_check_with_answer() {
        let a1 = answer(42);
        let a2 = answer(42);
        let a3 = answer(43);

        assert_eq!(Answer::check_with(a1, a2), CheckedAnswer::Correct);
        assert_eq!(Answer::check_with(a1, a3), CheckedAnswer::Incorrect);
        assert_eq!(Answer::check_with(None, a2), CheckedAnswer::Missing);
        assert_eq!(Answer::check_with(a2, None), CheckedAnswer::NotCounted);
        assert_eq!(Answer::check_with(None, None), CheckedAnswer::NotCounted);
    }

    #[test]
    fn test_check_with_question_group() {
        let group1 = QuestionGroup {
            A: answer(1),
            B: answer(2),
            C: answer(3),
            D: answer(4),
            E: none_answer(),
        };
        let key = QuestionGroup {
            A: answer(1),
            B: answer(0),
            C: answer(3),
            D: none_answer(),
            E: answer(5),
        };

        let checked = group1.check_with(&key);

        assert_eq!(checked.A, CheckedAnswer::Correct);
        assert_eq!(checked.B, CheckedAnswer::Incorrect);
        assert_eq!(checked.C, CheckedAnswer::Correct);
        assert_eq!(checked.D, CheckedAnswer::NotCounted);
        assert_eq!(checked.E, CheckedAnswer::Missing);
    }

    #[test]
    fn test_score_answersheet() {
        let correct_group = QuestionGroup {
            A: answer(1),
            B: answer(2),
            C: answer(3),
            D: answer(4),
            E: none_answer(),
        };
        let incorrect_group = QuestionGroup {
            A: answer(1),     // correct
            B: answer(9),     // incorrect
            C: answer(3),     // correct
            D: none_answer(), // missing
            E: answer(1),     // not counted
        };
        let missing_group = QuestionGroup {
            A: answer(1),
            B: answer(2),
            C: answer(3),
            D: none_answer(),
            E: none_answer(),
        };

        let combined = [correct_group.clone(), incorrect_group, missing_group];
        let answers: [QuestionGroup; 36] = vec![combined; 12]
            .into_flattened()
            .try_into()
            .expect("12 * 3 is not 36");

        let answer_sheet = AnswerSheet {
            subject_id: 1001.to_string(),
            student_id: 123456.to_string(),
            answers,
            subject_name: None,
            student_name: None,
            exam_room: None,
            exam_seat: None,
        };

        let key_sheet = AnswerKeySheet {
            subject_id: 1001.to_string(),
            answers: array::from_fn(|_| correct_group.clone()),
        };

        let result = answer_sheet.score(&key_sheet, &[1; 36]);

        // Per group: 2 correct, 3 incorrect (since missing is also considered incorrect here)
        assert_eq!(result.correct, 12);
        assert_eq!(result.score, 12);
        assert_eq!(result.incorrect, 24);
        assert_eq!(result.graded_questions.len(), 36);
    }

    #[test]
    fn test_bubble_definite() {
        let bubbles = vec![3u8];
        let ans = Answer::from_bubbles_vec(bubbles).unwrap();

        assert!(matches!(
            ans,
            Answer {
                num_type: None,
                number: 0u8
            }
        ))
    }
    #[test]
    fn test_bubble_unclear() {
        let bubbles = vec![5u8, 8u8];
        let ans = Answer::from_bubbles_vec(bubbles);
        assert!(ans.is_none());
    }
    #[test]
    fn test_bubble_none() {
        let bubbles = vec![0u8];
        assert!(Answer::from_bubbles_vec(bubbles).is_none());
    }
    #[test]
    fn test_bubble_plus_minus() {
        let bubbles_plus = vec![0u8, 5u8];
        let bubbles_minus = vec![1u8, 5u8];
        let bubbles_both = vec![2u8, 5u8];
        let ans_plus = Answer::from_bubbles_vec(bubbles_plus).unwrap();
        let ans_minus = Answer::from_bubbles_vec(bubbles_minus).unwrap();
        let ans_both = Answer::from_bubbles_vec(bubbles_both).unwrap();

        assert!(matches!(
            ans_plus,
            Answer {
                num_type: Some(NumberType::Plus),
                number: 2u8
            }
        ));
        assert!(matches!(
            ans_minus,
            Answer {
                num_type: Some(NumberType::Minus),
                number: 2u8
            }
        ));
        assert!(matches!(
            ans_both,
            Answer {
                num_type: Some(NumberType::PlusOrMinus),
                number: 2u8
            }
        ));
    }

    #[test]
    fn read_weight_csv() {
        let _ = env_logger::builder().is_test(true).try_init();

        let csv = "\
subject_code,q1,q2,q3,q4,q5,q6,q7,q8,q9,q10,q11,q12,q13,q14,q15,q16,q17,q18,q19,q20,q21,q22,q23,q24,q25,q26,q27,q28,q29,q30,q31,q32,q33,q34,q35,q36
10,2,3,1,1,1,1,4,2,2,3,,,,,,,,,,,,,,,,,,,,,,,,,,
";

        let reader = csv::Reader::from_reader(csv.as_bytes());
        let mut result: ScoreWeights = reader.into_deserialize().into();
        let (question_weights, max_score) = result.weights.remove("10").unwrap();
        let mut question_weights = question_weights.into_iter();
        assert_eq!(max_score, 2 + 3 + 1 + 1 + 1 + 1 + 4 + 2 + 2 + 3);
        assert_eq!(question_weights.next(), Some(2));
        assert_eq!(question_weights.next(), Some(3));
        assert_eq!(question_weights.next(), Some(1));
        assert_eq!(question_weights.next(), Some(1));
        assert_eq!(question_weights.next(), Some(1));
        assert_eq!(question_weights.next(), Some(1));
        assert_eq!(question_weights.next(), Some(4));
        assert_eq!(question_weights.next(), Some(2));
        assert_eq!(question_weights.next(), Some(2));
        assert_eq!(question_weights.next(), Some(3));
    }
    // #[test]
    // fn test_export_csv() {
    //     let correct_group = QuestionGroup {
    //         A: answer(1),
    //         B: answer(2),
    //         C: answer(3),
    //         D: answer(4),
    //         E: none_answer(),
    //     };
    //     let student_group = QuestionGroup {
    //         A: answer(1),
    //         B: answer(9),
    //         C: answer(3),
    //         D: none_answer(),
    //         E: answer(1),
    //     };
    //
    //     let answer_sheet = AnswerSheet {
    //         subject_id: "1001".to_string(),
    //         student_id: "123456".to_string(),
    //         answers: std::array::from_fn(|_| student_group.clone()),
    //         subject_name: None,
    //         student_name: None,
    //         exam_room: None,
    //         exam_seat: None,
    //     };
    //     let key_sheet = AnswerKeySheet {
    //         subject_code: "1001".to_string(),
    //         answers: std::array::from_fn(|_| correct_group.clone()),
    //     };
    //
    //     grade_and_export_csv(&answer_sheet, &key_sheet, "test_scores.csv")?;
    //     Ok(())
    // }
}
