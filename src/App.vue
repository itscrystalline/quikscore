<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke, Channel } from "@tauri-apps/api/core";
import { AnswerScoreResult, AnswerUpload, AppState, KeyUpload, ModelDownload } from "./types";
import StackedProgressBar, { ProgressBarProps } from "./components/StackedProgressBar.vue";
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

const modelDownloadEventHandler = (msg: ModelDownload): void => {
  console.log(msg);
  switch (msg.event) {
    case "progress":
      const { total, progressDetection, progressRecognition } = msg.data
      keyProgressBar.value = { type: "progress", max: total, progressTop: progressDetection, progressBottom: progressDetection + progressRecognition };
      return;
    case "success":
      keyProgressBar.value = { type: "indeterminate" };
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
      keyImage.value = "";
      keyStatus.value = "";
      keyProgressBar.value = undefined;
      break;

    case "clearWeights":
      keyHasWeights.value = "notUploaded";
      keyStatus.value = "";
      break;

    case "image":
      keyImage.value = msg.data.base64;
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
  switch (msg.event) {
    case "cancelled":
      answerStatus.value = "User cancelled upload";
      answerProgressBar.value = undefined;
      elapsed.value = "notCounting";
      break;
    case "clear":
      answerStatus.value = "";
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
      answerImages.value = msg.data.uploaded;
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

const ocr = ref(true);
watch(ocr, async (new_ocr, _) => { await invoke("set_ocr", { ocr: new_ocr }) });

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

const answerImages = ref<AnswerScoreResult[]>([]);
const answerStatus = ref("");
const answerProgressBar = ref<undefined | ProgressBarProps>(undefined);

const elapsed = ref<TimeElapsed>("notCounting");


async function ensureModels() {
  const modelDownloadChannel = new Channel<ModelDownload>();
  modelDownloadChannel.onmessage = modelDownloadEventHandler;
  try {
    await invoke("ensure_models", { channel: modelDownloadChannel });
  } catch (e) {
    console.error("failed to ensure models: " + e);
  }
}

async function uploadKey() {
  const path = await ensureModels();
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
  const path = await ensureModels();
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
</script>

<template>
  <main class="container">
    <div class="logo">
      <img class="logonana" src="/src/assets/logo_fit.png" alt="Quikscore logo">
      <span class="logo-text"><span class="q-letter"></span>uikscore</span>
    </div>
    <p class="credits">KOSEN-KMITL PBL Year 3 (C14, C35, C41, C43)</p>
    <p class="instructions">Upload your key sheet and some answer sheets!</p>
    <div class="header" style="justify-content: center;">
      <input type="checkbox" id="ocr-ck" v-model="ocr" />
      <label for="ocr-ck" style="padding-left: 1vh;">Enable OCR</label>
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
      <button class="btn-clear" @click="clearSheets" :disabled="!canClearSheets()" v-if="answerImages.length !== 0">
        🔄 Clear Answer Sheets
      </button>
    </div>
    <!-- 📦 Result Placeholder -->
    <div class="card">
      <div v-for="{ result, data } in answerImages" class="pad">
        <div v-if="result == 'ok'" class="result">
          <img :src="data.base64"></img>
          <div class="stats">
            <p>ID {{ data.studentId }}</p>
            <p>score: {{ data.score }}/{{ data.maxScore }}</p>
            <p>questions not answered: {{ data.notAnswered }}</p>
          </div>
        </div>
        <p v-else>
          {{ data.error }}
        </p>
      </div>
      <p class="placeholder" v-if="answerImages.length === 0 || answerStatus">
        {{ answerStatus === "" ? "Upload files to see results here" : answerStatus }}
      </p>
      <StackedProgressBar v-if="answerProgressBar" v-bind="answerProgressBar" />
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

.result {
  display: flex;
  align-items: start;
}

.stats {
  text-align: left;
  margin-left: 3vh;
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
</style>
