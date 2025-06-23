use itertools::Itertools;

use crate::state::{Answer, AnswerKeySheet, AnswerSheet, NumberType, QuestionGroup};

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
            (None, Some(_)) => CheckedAnswer::Incorrect,
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
        assert_eq!(Answer::check_with(None, a2), CheckedAnswer::Incorrect);
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
        assert_eq!(checked.E, CheckedAnswer::Incorrect);
    }

    #[test]
    fn test_collect_stats() {
        let checked = CheckedQuestionGroup {
            A: CheckedAnswer::Correct,
            B: CheckedAnswer::Incorrect,
            C: CheckedAnswer::Incorrect,
            D: CheckedAnswer::Missing,
            E: CheckedAnswer::NotCounted,
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
            E: answer(5),
        };
        let student_group = QuestionGroup {
            A: answer(1),     // correct
            B: answer(9),     // incorrect
            C: answer(3),     // correct
            D: none_answer(), // incorrect
            E: none_answer(), // incorrect
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

        let result = answer_sheet.score(&key_sheet);

        // Per group: 2 correct, 3 incorrect (since missing is also considered incorrect here)
        assert_eq!(result.correct, 2 * 36);
        assert_eq!(result.incorrect, 3 * 36);
        assert_eq!(result.not_answered, 0);
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
