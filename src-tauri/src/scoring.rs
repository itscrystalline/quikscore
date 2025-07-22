use std::{collections::HashMap, fs::File, io::BufReader};

use csv::DeserializeRecordsIntoIter;
use itertools::{multizip, Itertools};
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
    pub not_answered: u32,
    pub score: u32,
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
    pub score: u16,
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
        vec.iter().for_each(|&idx| {
            if idx < 3 {
                if num_type.is_none() {
                    num_type.replace(match idx {
                        0 => NumberType::Plus,
                        1 => NumberType::Minus,
                        2 => NumberType::PlusOrMinus,
                        _ => unreachable!(),
                    });
                }
            } else if num.is_none() {
                num.replace(idx - 3);
            }
        });
        Some(Answer {
            num_type,
            number: num?,
        })
    }
}

impl QuestionGroup {
    pub fn check_with(&self, key: &Self, weights: &ScoreWeight) -> CheckedQuestionGroup {
        let arr = [
            Answer::check_with(self.A, key.A),
            Answer::check_with(self.B, key.B),
            Answer::check_with(self.C, key.C),
            Answer::check_with(self.D, key.D),
            Answer::check_with(self.E, key.E),
        ];
        let weight_arr = [weights.A, weights.B, weights.C, weights.D, weights.E];
        let score = arr.iter().zip(weight_arr.iter()).fold(0, |acc, (&c, &w)| {
            if c == CheckedAnswer::Correct {
                acc + w as u16
            } else {
                acc
            }
        });
        #[allow(non_snake_case)]
        let [A, B, C, D, E] = arr;
        CheckedQuestionGroup {
            A,
            B,
            C,
            D,
            E,
            score,
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
    pub fn score(&self, key_sheet: &AnswerKeySheet, weights: &[ScoreWeight]) -> AnswerSheetResult {
        let graded_questions: [CheckedQuestionGroup; 36] = multizip((
            self.answers.iter(),
            key_sheet.answers.iter(),
            weights.iter(),
        ))
        .map(|(curr, key, weights)| curr.check_with(key, weights))
        .collect_array()
        .expect("should always be of size 36");

        let (mut correct, mut incorrect, mut not_answered, mut score) = (0u32, 0u32, 0u32, 0u32);
        for qg in graded_questions {
            score += qg.score as u32;
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
            score,
        }
    }
}

#[allow(non_snake_case)]
#[derive(Debug, serde::Deserialize)]
struct RawScoreWeights {
    subject_code: String,
    question_num: String,
    A: String,
    B: String,
    C: String,
    D: String,
    E: String,
}
#[derive(Default, Debug, Clone)]
pub struct ScoreWeights {
    pub weights: HashMap<String, (Vec<ScoreWeight>, u32)>,
}
#[allow(non_snake_case)]
#[derive(Default, Debug, Clone, Copy)]
pub struct ScoreWeight {
    pub A: u8,
    pub B: u8,
    pub C: u8,
    pub D: u8,
    pub E: u8,
}
impl ScoreWeight {
    fn max_score(&self) -> u32 {
        self.A as u32 + self.B as u32 + self.C as u32 + self.D as u32 + self.E as u32
    }
    #[cfg(test)]
    fn identity() -> Self {
        Self {
            A: 1,
            B: 1,
            C: 1,
            D: 1,
            E: 1,
        }
    }
}

type WeightsIter<R> = DeserializeRecordsIntoIter<R, RawScoreWeights>;
impl<R: std::io::Read> From<WeightsIter<R>> for ScoreWeights {
    fn from(values: WeightsIter<R>) -> Self {
        let mut weights: HashMap<String, (Vec<ScoreWeight>, u32)> = HashMap::new();
        for value in values {
            let value_option = match value {
                Ok(ok) => Some(ok),
                Err(e) => {
                    println!("error deserializing score weights: {e}");
                    None
                }
            };

            if let Some(RawScoreWeights {
                subject_code,
                question_num,
                A,
                B,
                C,
                D,
                E,
            }) = value_option
            {
                macro_rules! conv {
                    ($i: expr) => {
                        $i.parse::<u8>().unwrap_or_else(|e| {
                            println!("error reading question answer weight: not a number ('{e}'), using 0 as weight");
                            0
                        })
                    };
                }

                let Ok(question_num) = question_num.parse::<usize>() else {
                    println!("error reading question number: not a number ('{question_num}')");
                    continue;
                };
                if let Some((subject_weights, max_score)) = weights.get_mut(&subject_code) {
                    let w = ScoreWeight {
                        A: conv!(A),
                        B: conv!(B),
                        C: conv!(C),
                        D: conv!(D),
                        E: conv!(E),
                    };
                    *max_score += w.max_score();
                    subject_weights[question_num - 1] = w;
                } else {
                    let mut subject_weights = vec![ScoreWeight::default(); 36];
                    let w = ScoreWeight {
                        A: conv!(A),
                        B: conv!(B),
                        C: conv!(C),
                        D: conv!(D),
                        E: conv!(E),
                    };
                    subject_weights[question_num - 1] = w;
                    weights.insert(subject_code, (subject_weights, w.max_score()));
                }
            }
        }
        Self { weights }
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

        let checked = group1.check_with(&key, &ScoreWeight::identity());

        assert_eq!(checked.A, CheckedAnswer::Correct);
        assert_eq!(checked.B, CheckedAnswer::Incorrect);
        assert_eq!(checked.C, CheckedAnswer::Correct);
        assert_eq!(checked.D, CheckedAnswer::NotCounted);
        assert_eq!(checked.E, CheckedAnswer::Missing);
    }

    #[test]
    fn test_collect_stats() {
        let checked = CheckedQuestionGroup {
            A: CheckedAnswer::Correct,
            B: CheckedAnswer::Incorrect,
            C: CheckedAnswer::Incorrect,
            D: CheckedAnswer::Missing,
            E: CheckedAnswer::NotCounted,
            score: 1,
        };

        let stats = checked.collect_stats();
        assert_eq!(stats, (1, 2, 1));
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
        let student_group = QuestionGroup {
            A: answer(1),     // correct
            B: answer(9),     // incorrect
            C: answer(3),     // correct
            D: none_answer(), // missing
            E: answer(1),     // not counted
        };

        let answer_sheet = AnswerSheet {
            subject_code: 1001.to_string(),
            student_id: 123456.to_string(),
            answers: array::from_fn(|_| student_group.clone()),
        };

        let key_sheet = AnswerKeySheet {
            subject_code: 1001.to_string(),
            answers: array::from_fn(|_| correct_group.clone()),
        };

        let result = answer_sheet.score(&key_sheet, &[ScoreWeight::identity(); 36]);

        // Per group: 2 correct, 3 incorrect (since missing is also considered incorrect here)
        assert_eq!(result.correct, 2 * 36);
        assert_eq!(result.incorrect, 36);
        assert_eq!(result.not_answered, 36);
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
        let ans = Answer::from_bubbles_vec(bubbles).unwrap();

        assert!(matches!(
            ans,
            Answer {
                num_type: None,
                number: 2u8
            }
        ))
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
}
