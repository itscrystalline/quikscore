<template>
  <header>
    <img class="logo" src="/src/assets/logo_fit.png" alt="quikscore logo">
    <span class="logo_text">uikscore</span>
  </header>
  <main class="container">
    <ul id="red_cups" class="red_cup" ref="redCupRef" :class="{ 'move-down': move }">
      <li v-for="(left, index) in cups" :key="index" class="cup" :style="{ left: left + 'px' }"
        @click="handleCupClick(index)">
        <img class="element" src="/src/assets/red_cup.png" alt="cup">
      </li>
    </ul>
    <button id="login_button" class="button" @click="handleLoginClick">Login</button>
  </main>
</template>

<style scoped>
header {
  display: flex;
  align-items: center;
  position: relative;
  height: 100px;
}

.container {
  display: flex;
  width: 100%;
  justify-content: center;
  align-items: center;
  height: 100vh;
  overflow: hidden;
}

.logo {
  display: inline-block;
  width: 3em;
  height: 3em;
  position: absolute;
  top: 20px;
  left: 20px;
  padding: 0;
}

.logo_text {
  font-size: 2em;
  font-weight: 600;
  color: #cdd6f4;
  margin-top: 0px;
  padding-left: 75px;
}

.red_cup {
  justify-content: center;
  width: 360px;
  height: 200px;
  display: flex;
  padding: 0;
  position: relative;
  top: -500px;
  transition: top 0.5s ease;
  z-index: 2;
  list-style: none;
  align-items: center;
}

.cup {
  position: absolute;
  transition: left 0.3s ease;
}

.element {
  width: 100px;
  height: auto;
}

.red_cup.move-down {
  top: 5px;
}

.button {
  position: absolute;
  z-index: 1;
}
</style>

<script setup>
import { invoke } from '@tauri-apps/api/core'
import { ref } from 'vue'

const move = ref(false)
const cups = ref([0, 130, 260])
const isShuffling = ref(false)
const targetCupIndex = ref(1)
const isRevealed = ref(false)
const result = ref("")

async function authenticate() {
  await invoke("auth_pass");
}

const shufflePositions = () => {
  const newPositions = [...cups.value]
  for (let i = newPositions.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1))
      ;[newPositions[i], newPositions[j]] = [newPositions[j], newPositions[i]]
  }
  cups.value = newPositions
}

const startShuffle = async (times = 5, delay = 300) => {
  isShuffling.value = true
  for (let i = 0; i < times; i++) {
    shufflePositions()
    await new Promise(resolve => setTimeout(resolve, delay))
  }
  isShuffling.value = false
}

const handleLoginClick = async () => {
  if (isShuffling.value) return
  move.value = true
  isRevealed.value = false
  result.value = ""

  targetCupIndex.value = Math.floor(Math.random() * 3)
  await new Promise(resolve => setTimeout(resolve, 500))

  console.log("Start shuffle")
  startShuffle(6, 300)

  // ======================== TODO call this when authenticated ===============================
  // await authenticate();
}

const handleCupClick = (clickedIndex) => {
  if (isShuffling.value || isRevealed.value) return

  isRevealed.value = true

  if (clickedIndex === targetCupIndex.value) {
    result.value = "Yeah"
  } else {
    result.value = "No"
  }
}

</script>
