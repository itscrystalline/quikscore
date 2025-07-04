<script setup lang="ts">
import { ref } from "vue";
import { invoke, Channel } from "@tauri-apps/api/core";
import { AnswerScoreResult, AnswerUpload, KeyUpload } from "./messages";
import StackedProgressBar, { ProgressBarProps } from "./components/StackedProgressBar.vue";

const keyEventHandler = (msg: KeyUpload): void => {
  switch (msg.event) {
    case "cancelled":
      keyStatus.value = "User cancelled upload";
      keyProgressBar.value = false;
      break;

    case "clear":
      keyImage.value = "";
      keyStatus.value = "";
      keyProgressBar.value = false;
      break;

    case "done":
      keyImage.value = msg.data.base64;
      keyStatus.value = "";
      keyProgressBar.value = false;
      break;

    case "error":
      keyStatus.value = msg.data.error;
      keyProgressBar.value = false;
      break;
  }
}
const answerEventHandler = (msg: AnswerUpload): void => {
  switch (msg.event) {
    case "cancelled":
      answerStatus.value = "User cancelled upload";
      answerProgressBar.value = undefined;
      break;
    case "clear":
      answerStatus.value = "";
      answerImages.value = [];
      answerProgressBar.value = undefined;
      break;
    case "almostDone":
      answerStatus.value = "Publishing results...";
      answerProgressBar.value = { type: "indeterminate" };
      break;
    case "processing":
      const { total, started, finished } = msg.data;
      answerStatus.value = `Processing ${started}/${total} sheets... (${started - finished} in progress)`;
      answerProgressBar.value = { type: "progress", max: total, progressTop: finished, progressBottom: started };
      break;
    case "done":
      answerStatus.value = "";
      answerImages.value = msg.data.uploaded;
      answerProgressBar.value = undefined;
      break;
    case "error":
      answerStatus.value = `Error uploading sheets: ${msg.data.error}`;
      answerProgressBar.value = undefined;
      break;
  }
}

const keyImage = ref("");
const keyStatus = ref("");
const keyProgressBar = ref(false);

const answerImages = ref<AnswerScoreResult[]>([]);
const answerStatus = ref("");
const answerProgressBar = ref<undefined | ProgressBarProps>(undefined);

async function uploadKey() {
  keyProgressBar.value = true;
  const keyEventChannel = new Channel<KeyUpload>();
  keyEventChannel.onmessage = keyEventHandler;
  await invoke("upload_key_image", { channel: keyEventChannel });
}
async function clearKey() {
  const keyEventChannel = new Channel<KeyUpload>();
  keyEventChannel.onmessage = keyEventHandler;
  await invoke("clear_key_image", { channel: keyEventChannel });
}

async function uploadSheets() {
  answerProgressBar.value = { type: "indeterminate" };
  const answerEventChannel = new Channel<AnswerUpload>();
  answerEventChannel.onmessage = answerEventHandler;
  await invoke("upload_sheet_images", { channel: answerEventChannel });
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

    <div class="header">
      <h2>Answer Key</h2>
      <button :class="`btn-key${answerImages.length !== 0 ? ' btn-disabled' : ''}`" @click="uploadKey"
        v-bind:disabled="answerImages.length !== 0">{{ keyImage ===
          ""
          ?
          "📥\nUpload Answer Key..." :
          "Change Answer Key" }}</button>
      <button :class="`btn-clear${answerImages.length !== 0 ? ' btn-disabled' : ''}`" @click="clearKey"
        v-bind:disabled="answerImages.length !== 0" v-if="keyImage !== ''">🔄 Clear
        Answer Key</button>
    </div>
    <div class="card">
      <img v-bind:src="keyImage" :style="keyImage == '' ? 'display: none;' : ''"></img>
      <p class="placeholder" v-if="!keyImage && answerImages.length === 0">{{ keyStatus === "" ? "Upload a key..." :
        keyStatus }}</p>
      <StackedProgressBar v-if="keyProgressBar" type="indeterminate" />
    </div>

    <div class="header">
      <h2>Answer Sheets</h2>
      <button class="btn-sheet" @click="uploadSheets" :disabled="keyImage == ''">{{ answerImages.length === 0 ?
        "🧾 Upload Answer Sheets..." :
        "Change Answer Sheets"
        }}</button>
      <button class="btn-clear" @click="clearSheets" :disabled="keyImage == ''" v-if="answerImages.length !== 0">🔄
        Clear
        Answer
        Sheets</button>
    </div>
    <!-- 📦 Result Placeholder -->
    <div class="card">
      <div v-for="{ result, data } in answerImages" class="pad">
        <div v-if="result == 'ok'" class="result">
          <img :src="data.base64"></img>
          <div class="stats">
            <p>ID {{ data.studentId }}</p>
            <p>score: {{ data.correct }}</p>
            <p>incorrect: {{ data.incorrect }}</p>
            <p>questions not answered: {{ data.notAnswered }}</p>
          </div>
        </div>
        <p v-else>
          {{ data.error }}
        </p>
      </div>
      <p class="placeholder" v-if="answerImages.length === 0">{{ answerStatus === "" ?
        "Upload files to see results here" : answerStatus }}</p>
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
</style>
