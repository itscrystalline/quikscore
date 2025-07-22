export type KeyUpload =
  | {
      event: "cancelled";
    }
  | {
      event: "clear";
    }
  | {
      event: "uploadedWeights";
    }
  | {
      event: "done";
      data: {
        base64: string;
      };
    }
  | {
      event: "error";
      data: { error: string };
    };

export type AnswerUpload =
  | {
      event: "cancelled";
    }
  | {
      event: "clear";
    }
  | {
      event: "almostDone";
    }
  | {
      event: "processing";
      data: {
        total: number;
        started: number;
        finished: number;
      };
    }
  | {
      event: "done";
      data: {
        uploaded: AnswerScoreResult[];
      };
    }
  | {
      event: "error";
      data: { error: string };
    };

export type AnswerScoreResult =
  | {
      result: "ok";
      data: {
        studentId: string;
        base64: string;
        correct: number;
        incorrect: number;
        notAnswered: number;
      };
    }
  | {
      result: "error";
      data: { error: string };
    };
