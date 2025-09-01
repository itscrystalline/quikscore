<script setup lang="ts">
import { Ref, ref, watch } from "vue";
import { invoke, Channel } from "@tauri-apps/api/core";
import {
  AnswerUpload,
  KeyUpload,
  CsvExport,
  ModelDownload,
  AppState,
  BlobbedAnswerScoreResult,
  AnswerScoreResult,
} from "./types";
import StackedProgressBar, { ProgressBarProps } from "./components/StackedProgressBar.vue";
import ImagePreview from "./components/ImagePreview.vue";
import { listen } from "@tauri-apps/api/event";

type TimeElapsed = | "notCounting" | number;
const hms = (secs: number): string => {
  if (secs > 0) {
    var h = Math.floor(secs / 3600);
    var m = Math.floor(secs % 3600 / 60);
    var s = Math.floor(secs % 3600 % 60);
    var hDisplay = h > 0 ? h + "h " : "";
    var mDisplay = m > 0 ? m + "m " : "";
    var sDisplay = s > 0 ? s + "s" : "";
    return hDisplay + mDisplay + sDisplay;
  } else {
    return "<1s";
  }
}


const appState = ref<AppState>("Init");
listen<AppState>("state", (event) => {
  appState.value = event.payload;
  console.log("state changed to " + appState.value);
})

const modelDownloadEventHandler = (progressBar: Ref<undefined | ProgressBarProps>, status: Ref<string>) => (msg: ModelDownload): void => {
  switch (msg.event) {
    case "progress":
      const { total, progress } = msg.data
      progressBar.value = { type: "progress", max: total, progressTop: 0, progressBottom: progress };
      status.value = "Downloading " + (progress * 100 / total).toFixed(2) + "%";
      return;
    case "success":
      progressBar.value = { type: "indeterminate" };
      return;
  }
}
const keyEventHandler = (msg: KeyUpload): void => {
  switch (msg.event) {
    case "cancelled":
      keyStatus.value = "User cancelled upload";
      keyProgressBar.value = undefined;
      break;

    case "clearImage":
      URL.revokeObjectURL(keyImage.value);
      keyImage.value = "";
      keyStatus.value = "";
      keyProgressBar.value = undefined;
      break;

    case "clearWeights":
      keyHasWeights.value = "notUploaded";
      keyStatus.value = "";
      break;

    case "image":
      keyImage.value = bytesToBlobUrl(msg.data.bytes);
      keyStatus.value = "";
      keyProgressBar.value = undefined;
      break;

    case "uploadedWeights":
      keyHasWeights.value = "yes";
      keyStatus.value = "";
      break;

    case "missingWeights":
      keyHasWeights.value = "missingWeights";
      break;

    case "error":
      keyStatus.value = msg.data.error;
      keyProgressBar.value = undefined;
      break;

    default:
      keyStatus.value = "Unhandled event";
  }
}
const answerEventHandler = (msg: AnswerUpload): void => {
  const blobify = (old: AnswerScoreResult[]): BlobbedAnswerScoreResult[] => {
    return old.map(o => {
      switch (o.result) {
        case "ok":
          return {
            result: "ok",
            data: {
              studentId: o.data.studentId,
              studentName: o.data.studentName,
              examRoom: o.data.examRoom,
              examSeat: o.data.examSeat,
              blobUrl: bytesToBlobUrl(o.data.bytes),
              score: o.data.score,
              maxScore: o.data.maxScore,
              correct: o.data.correct,
              incorrect: o.data.incorrect,
            },
          };

        case "error":
          return {
            result: "error",
            data: {
              error: o.data.error
            }
          };
      }
    });
  };
  const clearBlobs = (old: BlobbedAnswerScoreResult[]): void => {
    old.forEach(o => {
      if (o.result == "ok") {
        URL.revokeObjectURL(o.data.blobUrl);
      }
    });
  }

  switch (msg.event) {
    case "cancelled":
      answerStatus.value = "User cancelled upload";
      answerProgressBar.value = undefined;
      elapsed.value = "notCounting";
      break;
    case "clear":
      answerStatus.value = "";
      clearBlobs(answerImages.value);
      clearIdMappings()
      answerImages.value = [];
      answerProgressBar.value = undefined;
      elapsed.value = "notCounting";
      break;
    case "almostDone":
      answerStatus.value = "Publishing results...";
      answerProgressBar.value = { type: "indeterminate" };
      elapsed.value = "notCounting";
      break;
    case "processing":
      if (answerImages.value.length != 0) {
        clearBlobs(answerImages.value);
        clearIdMappings()
        answerImages.value = [];
      }

      const { total, started, finished } = msg.data;
      var secsPerImage = -1;
      if (elapsed.value == "notCounting") {
        elapsed.value = Date.now();
      } else if (finished > 0) {
        const timePassed = (Date.now() - elapsed.value) / 1000;
        secsPerImage = Math.round(timePassed / finished);
      }
      const leftText = secsPerImage != -1 ? `, ${hms(secsPerImage * (total - finished))} left` : '';
      answerStatus.value = `Processing ${started}/${total} sheets... (${started - finished} in progress${leftText})`;
      answerProgressBar.value = { type: "progress", max: total, progressTop: finished, progressBottom: started };
      break;
    case "done":
      answerStatus.value = "";
      answerImages.value = blobify(msg.data.uploaded);
      answerProgressBar.value = undefined;
      elapsed.value = "notCounting";
      break;
    case "error":
      answerStatus.value = `Error uploading sheets: ${msg.data.error} `;
      answerProgressBar.value = undefined;
      elapsed.value = "notCounting";
      break;
    default:
      answerStatus.value = "Unhandled event";
  }
}
const csvExportEventHandler = (msg: CsvExport) => {
  switch (msg.event) {
    case "cancelled":
      answerStatus.value = "Export cancelled";
      break;
    case "done":
      answerStatus.value = "Export success!";
      break;
    case "error":
      answerStatus.value = `Export failed: ${msg.data.error}`;
      break;
  }
  answerProgressBar.value = undefined;
}

const ocr = ref(false);
const ocrStatus = ref("");
watch(ocr, (new_ocr, _) => {
  invoke("set_ocr", { ocr: new_ocr })
    .then(() => ocrStatus.value = "")
    .catch(err => ocrStatus.value = err)
});

const keyImage = ref("");
const keyHasWeights = ref<"notUploaded" | "missingWeights" | "yes">("notUploaded");
const keyStatus = ref("");
const keyProgressBar = ref<undefined | ProgressBarProps>(undefined);

const canUploadKey = () => appState.value == "Init" || appState.value == "WithKey";
const canChangeKey = () => appState.value == "WithKey" || appState.value == "WithKeyAndWeights";
const canClearKey = () => appState.value == "WithKey";
const canUploadWeights = () => appState.value == "WithKey" || appState.value == "WithKeyAndWeights";
const canChangeWeights = () => appState.value == "WithKeyAndWeights";
const canClearWeights = () => appState.value == "WithKeyAndWeights";
const canUploadSheets = () => appState.value == "WithKeyAndWeights";
const canChangeSheets = () => appState.value == "Scored";
const canCancelSheetUpload = () => appState.value == "Scoring";
const canClearSheets = () => appState.value == "Scored";
const canExportCsv = () => appState.value == "Scored";

const answerImages = ref<BlobbedAnswerScoreResult[]>([]);
const answerStatus = ref("");
const answerProgressBar = ref<undefined | ProgressBarProps>(undefined);

const elapsed = ref<TimeElapsed>("notCounting");

const mongoDbUri = ref("");
const mongoDbName = ref("");

const previewingImage = ref<string | undefined>(undefined);

async function enterDatabaseInfo() {
  try {
    await invoke("enter_database_information", { uri: mongoDbUri.value, name: mongoDbName.value });
    console.log("Database information sent:", mongoDbUri.value, mongoDbName.value);
  } catch (err) {
    console.error("Failed to send database info:", err);
  }
}

async function ensureModels(progressBar: Ref<undefined | ProgressBarProps>, status: Ref<string>) {
  progressBar.value = { type: "indeterminate" };
  status.value = "Verifying OCR Models...";
  let retries = 0;
  while (retries < 3) {
    try {
      const modelDownloadChannel = new Channel<ModelDownload>();
      modelDownloadChannel.onmessage = modelDownloadEventHandler(progressBar, status);
      await invoke("ensure_models", { channel: modelDownloadChannel });
      retries = 3;
    } catch (e) {
      status.value = "failed to ensure models: " + e + (retries < 2 ? ", retrying" : "");
      retries += 1;
    }
  }
}

async function uploadKey() {
  const path = await ensureModels(keyProgressBar, keyStatus);
  keyStatus.value = "Upload A Key...";
  keyProgressBar.value = { type: "indeterminate" };
  const keyEventChannel = new Channel<KeyUpload>();
  keyEventChannel.onmessage = keyEventHandler;
  await invoke("upload_key_image", { channel: keyEventChannel, modelDir: path });
}
async function clearKey() {
  const keyEventChannel = new Channel<KeyUpload>();
  keyEventChannel.onmessage = keyEventHandler;
  await invoke("clear_key_image", { channel: keyEventChannel });
}

async function uploadWeights() {
  const keyEventChannel = new Channel<KeyUpload>();
  keyEventChannel.onmessage = keyEventHandler;
  await invoke("upload_weights", { channel: keyEventChannel });
}
async function clearWeights() {
  const keyEventChannel = new Channel<KeyUpload>();
  keyEventChannel.onmessage = keyEventHandler;
  await invoke("clear_weights", { channel: keyEventChannel });
}

async function uploadSheets() {
  const path = await ensureModels(answerProgressBar, answerStatus);
  answerStatus.value = "Upload files to see results here";
  answerProgressBar.value = { type: "indeterminate" };
  const answerEventChannel = new Channel<AnswerUpload>();
  answerEventChannel.onmessage = answerEventHandler;
  await invoke("upload_sheet_images", { channel: answerEventChannel, modelDir: path });
}
async function cancelUploadSheets() {
  const answerEventChannel = new Channel<AnswerUpload>();
  answerEventChannel.onmessage = answerEventHandler;
  await invoke("cancel_upload_sheets", { channel: answerEventChannel });
}
async function clearSheets() {
  const answerEventChannel = new Channel<AnswerUpload>();
  answerEventChannel.onmessage = answerEventHandler;
  await invoke("clear_sheet_images", { channel: answerEventChannel });
}
async function exportCsv() {
  answerProgressBar.value = { type: "indeterminate" };
  const csvExportChannel = new Channel<CsvExport>();
  csvExportChannel.onmessage = csvExportEventHandler;
  await invoke("export_csv", { channel: csvExportChannel });
}

const idToPreview = new Map<string, string>();
async function image_from_id(id: string) {
  const url = idToPreview.get(id);
  if (url) {
    previewingImage.value = url;
  } else {
    const img: number[] | undefined = await invoke("image_of", { id });
    if (!img) {
      console.error(`image for ${id} could not be found.`)
      return;
    }
    const blobUrl = bytesToBlobUrl(img);
    idToPreview.set(id, blobUrl);

    previewingImage.value = blobUrl;
  }
}
function clearIdMappings() {
  idToPreview.forEach((v, _k, _m) => URL.revokeObjectURL(v));
  idToPreview.clear();
}

function bytesToBlobUrl(bytes: number[]): string {
  const blob = new Blob([new Uint8Array(bytes)], { type: "image/webp" });
  return URL.createObjectURL(blob);
}

function avgMinMax(result: BlobbedAnswerScoreResult[]): { avg: number, min: number, max: number } {
  const scores: number[] = result.filter(v => v.result == "ok").map(v => v.data.score);
  const avg = Math.round((scores.reduce((l, r) => l + r) / scores.length) * 100) / 100;
  const min = Math.min(...scores);
  const max = Math.max(...scores);
  return { avg, min, max };
}
</script>

<template>
  <main class="container">
    <ImagePreview :id="previewingImage" @close="previewingImage = undefined" />

    <div class="logo">
      <img class="logonana" src="/src/assets/logo_fit.png" alt="Quikscore logo">
      <span class="logo-text"><span class="q-letter"></span>uikscore</span>
    </div>
    <p class="credits">KOSEN-KMITL PBL Year 3 (C14, C35, C41, C43)</p>
    <p class="instructions">Upload your key sheet and some answer sheets!</p>
    <div class="header" style="justify-content: center;">
      <input type="checkbox" id="ocr-ck" v-model="ocr" v-bind:disabled="ocrStatus != ''" />
      <label for="ocr-ck" class="ocr-text" v-if="ocrStatus == ''">
        Enable OCR (Requires tesseract to be installed)
      </label>
      <label for="ocr-ck" class="ocr-text disabled" v-if="ocrStatus != ''">
        OCR Disabled ({{ ocrStatus }})
      </label>
    </div>

    <div class="mongo_db_information_field">
      <div class="form_group">
        <div class="form_wrapper">
          <label for="mongo_db_uri">MongoDB URI: </label>
          <input type="text" id="mongo_db_uri" class="text-box" v-model="mongoDbUri" placeholder="URI...." />
        </div>
        <div class="form_wrapper">
          <label for="mongo_db_name">MongoDB Name: </label>
          <input type="text" id="mongo_db_name" class="text-box" v-model="mongoDbName" placeholder="Name...." />
        </div>
      </div>
      <button class="mongo_db_enter" @click="enterDatabaseInfo">Enter</button>
    </div>


    <div class="header">
      <h2>Answer Key & Weights</h2>
      <button :class="`btn-key${!(canUploadKey() || canChangeKey()) ? ' btn-disabled' : ''}`" @click="uploadKey"
        v-bind:disabled="!(canUploadKey() || canChangeKey())">
        {{ canChangeKey() ? "Change Answer Key" : "📥\nUpload Answer Key..." }}
      </button>
      <button :class="`btn-clear${!canClearKey() ? ' btn-disabled' : ''}`" @click="clearKey"
        v-bind:disabled="!canClearKey()" v-if="keyImage !== ''">
        🔄 Clear Answer Key
      </button>

      <button :class="`btn-key${!(canUploadWeights() || canChangeWeights()) ? ' btn-disabled' : ''}`"
        @click="uploadWeights" v-bind:disabled="!(canUploadWeights() || canChangeWeights())">
        {{ canChangeWeights() ? "Change Weights file" : "📥\nUpload Weights file..." }}
      </button>
      <button :class="`btn-clear${!canClearWeights() ? ' btn-disabled' : ''}`" @click="clearWeights"
        v-bind:disabled="!canClearWeights()" v-if="keyImage !== ''">
        🔄 Clear Weights
      </button>
    </div>
    <div class="card">
      <p class="placeholder" v-if="keyStatus !== '' || !keyImage">
        {{ keyStatus === "" ? "Upload a key..." : keyStatus }}
      </p>
      <StackedProgressBar v-if="keyProgressBar" v-bind="keyProgressBar" />
      <div :style="keyImage == '' ? 'display: none;' : ''" class="key-image-container">
        <div :class="keyHasWeights == 'notUploaded' ? 'yellow' : keyHasWeights == 'missingWeights' ? 'red' : 'green'">
          <img v-if="keyHasWeights == 'notUploaded'" src="/src/assets/no_weights.svg" />
          <img v-if="keyHasWeights == 'missingWeights'" src="/src/assets/missing_weights.svg" />
          <img v-if="keyHasWeights == 'yes'" src="/src/assets/have_weights.svg" />
          <p>
            {{
              keyHasWeights == 'notUploaded' ? "Please upload weights for this key." :
                keyHasWeights == 'missingWeights' ? "Weights missing for this key." :
                  "Weights uploaded!"
            }}
          </p>
        </div>
        <img v-bind:src="keyImage" :style="keyImage == '' ? 'display: none;' : ''" />
      </div>
    </div>

    <div class="header">
      <h2>Answer Sheets</h2>
      <button class="btn-sheet" @click="uploadSheets" :disabled="!(canUploadSheets() || canChangeSheets())">
        {{ canChangeSheets() ? "Change Answer Sheets" : "🧾 Upload Answer Sheets..." }}
      </button>
      <button class="btn-clear" @click="cancelUploadSheets" :disabled="!canCancelSheetUpload()"
        v-if="canCancelSheetUpload()">
        Cancel Upload
      </button>
      <button class="btn-clear" @click="clearSheets" :disabled="!canClearSheets()" v-if="canClearSheets()">
        🔄 Clear Answer Sheets
      </button>
      <button class="btn-sheet" @click="exportCsv" :disabled="!canExportCsv()" v-if="canExportCsv()">
        Export to CSV...
      </button>
    </div>
    <!-- 📦 Result Placeholder -->
    <div class="card">
      <p class="placeholder" v-if="answerImages.length === 0 || answerStatus">
        {{ answerStatus === "" ? "Upload files to see results here" : answerStatus }}
      </p>
      <StackedProgressBar v-if="answerProgressBar" v-bind="answerProgressBar" />

      <div v-if="answerImages.length != 0">
        <p> Average: {{ avgMinMax(answerImages).avg }} </p>
        <p> Minimum Score: {{ avgMinMax(answerImages).min }} </p>
        <p> Maximum Score: {{ avgMinMax(answerImages).max }} </p>
      </div>
      <div v-for="{ result, data } in answerImages" class="pad">
        <div v-if="result == 'ok'" class="result">
          <img :src="data.blobUrl" @click="image_from_id(data.studentId)" title="Click to Preview Image"></img>
          <div class="stats">
            <div>
              <p v-if="data.studentName">{{ data.studentName }}</p>
              <code>({{ data.studentId }})</code>
            </div>
            <div>
              <p v-if="data.examRoom">Room {{ data.examRoom }}</p>
              <p v-if="data.examSeat">Seat {{ data.examSeat }}</p>
            </div>
            <p>{{ data.score }}/{{ data.maxScore }}</p>
            <div class="score-wrap" :title="`${data.score} / ${data.maxScore}`">
              <div class="score-bar" role="progressbar" :aria-valuenow="data.score" aria-valuemin="0"
                :aria-valuemax="data.maxScore"
                :aria-valuetext="data.maxScore ? `${Math.round((data.score / data.maxScore) * 100)}% correct` : 'no max score'">
                <div class="segment correct" :style="{
                  width: Math.max(0, Math.min(100, data.maxScore ? (data.score / data.maxScore) * 100 : 0)) + '%'
                }"></div>

                <div class="segment wrong" :style="{
                  width: Math.max(
                    0,
                    Math.min(
                      100,
                      data.maxScore ? 100 - (data.score / data.maxScore) * 100 : 100
                    )
                  ) + '%'
                }"></div>
              </div>

            </div>
          </div>
        </div>
        <p v-else>
          {{ data.error }}
        </p>
      </div>
    </div>
  </main>
</template>

<style scoped>
.logo.vite:hover {
  filter: drop-shadow(0 0 2em #747bff);
}

.logo.vue:hover {
  filter: drop-shadow(0 0 2em #249b73);
}
</style>
<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 19px;
  line-height: 60px;
  font-weight: 400;
  animation-duration: 0.1s;

  color: #cdd6f4;
  background-color: #111827;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.logo {
  display: flex;
  align-items: center;
  align-self: center;
  padding: 3vh;
}

.logo-text {
  font-size: 2em;
  font-weight: 600;
  color: #cdd6f4;
  margin-top: 0px;
  display: inline-block;
}

.logonana {
  width: 3em;
  height: 3em;
  align-items: center;
}

.container {
  margin: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
  text-align: center;
  flex-basis: content;
}


.logo.tauri:hover {
  filter: drop-shadow(0 0 2em #24c8db);
}

.row {
  display: flex;
  justify-content: center;
}

.header {
  display: flex;
  align-items: center;
  justify-content: left;
}

a {
  font-weight: 500;
  color: #646cff;
  text-decoration: inherit;
}

a:hover {
  color: #535bf2;
}

h2 {
  text-align: left;
  margin: 0 1ch;
}

p {
  margin: 0;
}

p.instructions {
  padding-bottom: 3vh;
}

p.credits {
  color: #a6adc8;
  font-size: 0.6em;
  padding: 0;
  height: fit-content;
}

.mongo_db_information_field {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 1rem;
}

.form_wrapper {
  display: flex;
  align-items: center;
  gap: 0.5rem;
}

.text-box {
  padding: 0.25rem;           
  border-radius: 50px;        
  border: 1px solid #1e293b;    
  width: 300px;                         
  background-color: #1e293b 
}

.mongo_db_enter {
  border-radius: 25px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  padding: 1vh;
  margin-right: 1vh;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  transition: border-color 0.25s;
  transition: all 0.2s ease;
  box-shadow: 0 2px 2px rgba(2, 59, 98, 0.2);
  background-color: #5d98f6;
  color: #ffffff;
}

.mongo_db_enter:hover {
  background-color: #2563eb;
  border-color: #45475a;
}

.result {
  display: flex;
  align-items: start;
}

.result>img {
  object-fit: cover;
  border-radius: 8px;
  box-shadow: 0 1px 4px rgba(16, 24, 40, 0.06);
  cursor: zoom-in;
  transition: transform .16s ease, box-shadow .16s ease;
}

.result>img:hover,
.result>img:focus {
  transform: scale(1.03);
  box-shadow: 0 8px 20px rgba(16, 24, 40, 0.12);
  outline: none;
}

.result>img:focus {
  outline: 3px solid rgba(59, 130, 246, 0.18);
  outline-offset: 2px;
}

.stats {
  text-align: left;
  margin-left: 3vh;
  width: 100%;
}

.stats>div {
  display: flex;
  flex-direction: row;
  align-items: start;
  gap: 1vh;
}

/* score progress UI */
.score-wrap {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 140px;
  max-width: 100%;
}

/* outer rail */
.score-bar {
  display: flex;
  width: 100%;
  height: 12px;
  background: #445165;
  border-radius: 999px;
  overflow: hidden;
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.6);
  border: 1px solid rgba(15, 23, 42, 0.03);
}

/* segments (green for correct, red for wrong) */
.segment {
  height: 100%;
  transition: width .28s ease;
  flex-shrink: 0;
  will-change: width;
}

/* correct = green gradient */
.segment.correct {
  background: linear-gradient(90deg, #a6e3a1, #a6da95);
}

/* wrong = red gradient */
.segment.wrong {
  background: linear-gradient(90deg, #ed8796, #f38ba8);
}

/* numeric label to the right */
.score-label {
  font-size: 13px;
  font-weight: 600;
  color: #0f172a;
  white-space: nowrap;
  margin-left: 6px;
}

/* hide zero-width segments cleanly (avoids tiny anti-alias artifacts) */
.segment[style*="width: 0%"] {
  width: 0 !important;
  min-width: 0 !important;
}

.pad:not(:last-child) {
  margin-bottom: 3vh;
}


button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  padding: 1vh;
  margin-right: 1vh;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  transition: border-color 0.25s;
  transition: all 0.2s ease;
  box-shadow: 0 2px 2px rgba(0, 0, 0, 0.2);
}

button {
  cursor: pointer;
}

button:hover {
  border-color: #45475a;
}

button:active {
  border-color: #45475a;
  background-color: #585b70;
}

button {
  outline: none;
}

button.btn-disabled {
  filter: opacity(50%);
  cursor: not-allowed;
}

button.btn-key {
  background-color: #3b82f6;
  color: #ffffff;
}

button.btn-key:hover {
  background-color: #2563eb;
}

button.btn-sheet {
  background-color: #10b981;
  color: #ffffff;
}

button:hover:not(:disabled) {
  transform: scale(0.98);
}

button:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

button.btn-sheet:hover {
  background-color: #059669;
}

button.btn-clear {
  background-color: #f87171;
  color: #ffffff;
}

button.btn-clear:hover:not(:disabled) {
  background-color: #ef4444;
}

#greet-input {
  margin-right: 5px;
}

.card {
  border: 1px solid var(--border);
  border-radius: 10px;
  margin: 2vh;
  padding: 2vh;
  background: #1e293b;
  /* slate-800 */
}

.placeholder {
  display: flex;
  align-items: center;
  justify-content: center;
  color: #9ca3af;
  font-style: italic;
}

img {
  object-fit: contain;
  max-height: 100%;
  max-width: 100%;
}

.key-image-container {
  display: inline-block;
  position: relative;
}

.key-image-container>div {
  position: absolute;
  width: 100%;
  /* 100% bleeds outside the image for some reason */
  height: 96%;
  display: flex;
  overflow: hidden;
  align-items: self-end;
  justify-content: start;
}

.key-image-container>div.red {
  background: #E64553;
  background: linear-gradient(0deg, rgba(230, 69, 83, 0.8) 0%, rgba(233, 94, 106, 0.6) 20%, rgba(255, 255, 255, 0) 75%);
}

.key-image-container>div.yellow {
  background: #DF8E1D;
  background: linear-gradient(0deg, rgba(223, 142, 29, 0.8) 0%, rgba(227, 158, 60, 0.6) 20%, rgba(255, 255, 255, 0) 75%);
}

.key-image-container>div.green {
  background: #10B981;
  background: linear-gradient(0deg, rgba(16, 185, 129, 0.8) 0%, rgba(79, 204, 162, 0.6) 20%, rgba(255, 255, 255, 0) 75%);
}

.key-image-container>div>* {
  margin-left: 3vh;
  margin-bottom: 3vh;
  color: #eff1f5;
  height: 2em;
  font-size: 1.3em;
  text-shadow: rgba(0, 0, 0, 0.25) 0px 54px 55px, rgba(0, 0, 0, 0.12) 0px -12px 30px, rgba(0, 0, 0, 0.12) 0px 4px 6px, rgba(0, 0, 0, 0.17) 0px 12px 13px, rgba(0, 0, 0, 0.09) 0px -3px 5px;

}

.key-image-container>img {
  max-width: 100%;
}

.ocr-text {
  padding-left: 1vh;
}

.disabled {
  color: #9399b2;
  font-style: italic;
}
</style>
