<script setup lang="ts">
import { Ref, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

const keyImage = ref("");
const keyStatus = ref("");

const answerImages: Ref<string[], string[]> = ref([]);
const answerStatus = ref("");

async function uploadKey() {
  await invoke("upload_key_image");
}
async function clearKey() {
  await invoke("clear_key_image");
}

async function uploadSheets() {
  await invoke("upload_sheet_images");
}
async function clearSheets() {
  await invoke("clear_sheet_images");
}


listen<string>('key-status', (event) => {
  keyStatus.value = event.payload;
});
listen<string>('key-image', (event) => {
  keyImage.value = event.payload;
});

listen<string>('sheet-status', (event) => {
  answerStatus.value = event.payload;
});
listen<string[]>('sheet-images', (event) => {
  answerImages.value = event.payload;
});
</script>

<template>
  <main class="container">
    <h1>Quikscore</h1>
    <p>Upload your key sheet and some answer sheets!</p>

    <button @click="uploadKey" :disabled="answerImages.length !== 0">{{ keyImage === "" ? "Upload Answer Key..." :
      "Change Answer Key" }}</button>
    <button @click="clearKey" :disabled="answerImages.length !== 0" v-if="keyImage !== ''">Clear Answer Key</button>
    <p :style="keyStatus == '' ? 'display: none;' : ''">{{ keyStatus }}</p>
    <img v-bind:src="keyImage" :style="keyImage == '' ? 'display: none;' : ''"></img>

    <button @click="uploadSheets" :disabled="keyImage == ''">{{ answerImages.length === 0 ? "Upload Answer Sheets..." :
      "Change Answer Sheets"
      }}</button>
    <button @click="clearSheets" :disabled="keyImage == ''" v-if="answerImages.length !== 0">Clear Answer
      Sheets</button>
    <p :style="answerStatus == '' ? 'display: none;' : ''">{{ answerStatus }}</p>
    <img v-for="source in answerImages" :src="source"></img>
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
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  color: #cdd6f4;
  background-color: #11111b;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

.container {
  margin: 0;
  padding-top: 10vh;
  display: flex;
  flex-direction: column;
  justify-content: center;
  text-align: center;
}

.logo {
  height: 6em;
  padding: 1.5em;
  will-change: filter;
  transition: 0.75s;
}

.logo.tauri:hover {
  filter: drop-shadow(0 0 2em #24c8db);
}

.row {
  display: flex;
  justify-content: center;
}

a {
  font-weight: 500;
  color: #646cff;
  text-decoration: inherit;
}

a:hover {
  color: #535bf2;
}

h1 {
  text-align: center;
}

button {
  border-radius: 8px;
  border: 1px solid transparent;
  padding: 0.6em 1.2em;
  font-size: 1em;
  font-weight: 500;
  font-family: inherit;
  background-color: #313244;
  color: #cdd6f4;
  transition: border-color 0.25s;
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

#greet-input {
  margin-right: 5px;
}
</style>
