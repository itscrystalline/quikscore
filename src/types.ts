export type AppState =
  | "Init"
  | "WithKey"
  | "WithKeyAndWeights"
  | "Scoring"
  | "Scored";

export type ModelDownload =
  | {
      event: "progress";
      data: {
        progress: number;
        total: number;
      };
    }
  | { event: "success" };
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
        bytes: number[];
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
        studentName: string | undefined;
        examRoom: string | undefined;
        examSeat: string | undefined;
        bytes: number[];
        score: number;
        maxScore: number;
        correct: number;
        incorrect: number;
      };
    }
  | {
      result: "error";
      data: { error: string };
    };

export type BlobbedAnswerScoreResult =
  | {
      result: "ok";
      data: {
        studentId: string;
        studentName: string | undefined;
        examRoom: string | undefined;
        examSeat: string | undefined;
        blobUrl: string;
        score: number;
        maxScore: number;
        correct: number;
        incorrect: number;
      };
    }
  | {
      result: "error";
      data: { error: string };
    };

export type CsvExport =
  | { event: "cancelled" }
  | { event: "done" }
  | { event: "error"; data: { error: string } };
