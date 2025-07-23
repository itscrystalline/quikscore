export type AppState =
  | "Init"
  | "WithKey"
  | "WithKeyAndWeights"
  | "Scoring"
  | "Scored";
export type KeyUpload =
  | {
      event: "cancelled";
    }
  | {
      event: "clearImage";
    }
  | {
      event: "clearWeights";
    }
  | {
      event: "uploadedWeights";
    }
  | {
      event: "missingWeights";
    }
  | {
      event: "image";
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
        score: number;
        maxScore: number;
        correct: number;
        incorrect: number;
        notAnswered: number;
      };
    }
  | {
      result: "error";
      data: { error: string };
    };
