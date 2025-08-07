<template>
  <header>
    <img class="logo" src="/src/assets/logo_fit.png" alt="quikscore logo">
    <span class="logo_text">uikscore</span>
  </header>
  <main class="container">
    <ul id="red_cups" class="red_cup" ref="redCupRef" :class="{ 'move-down': move }">
      <li v-for="(left, index) in cups" :key="index" class="cup" :class="{ 'cup-raised': raisedCups[index] }" :style="{ left: left + 'px' }"
        @click="handleCupClick(index)">
        <img class="element" src="/src/assets/red_cup.png" alt="cup">
      </li>
    </ul>
    <button
      v-if="showLoginButton"
      id="login_button"
      class="button"
      @click="handleLoginClick"
      :style="buttonStyle"
    >
      Login
    </button>
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
  transition: left 0.3s ease, top 0.3s cubic-bezier(0.4, 0, 0.2, 1);
  top: 0;
}

.cup-raised {
  top: -150px;
  z-index: 3;
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
//fix at w:380px h:480px
//import { appWindow, LogicalSize } from '@tauri-apps/api/window'
import { ref, onMounted, computed } from 'vue'
//appWindow.setResizable(false)
//appWindow.setSize({ width: 380, height: 480 })

const move = ref(false)
const cups = ref([0, 130, 260])
const isShuffling = ref(false)
const targetCupIndex = ref(1)
const isRevealed = ref(false)
const result = ref("")
const buttonZIndex = ref(1)
const showLoginButton = ref(true)
const raisedCups = ref([false, false, false])
let buttonStyle = computed(() => {
  // Depend on resizeTrigger so this recomputes on window resize
  // At the start, button stays at its original place. After move, follow the middle cup.
  const left = move.value ? (cups.value[1] + 35) + 'px' : "145px";
  const top = '320px';
  return { left, top, position: 'absolute', zIndex: buttonZIndex.value };
});

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
  // Reset raised cups after shuffle
  raisedCups.value = Array(cups.value.length).fill(false)
}

const startShuffle = async (times = 5, delay = 300) => {
  isShuffling.value = true
  buttonZIndex.value = 1 // Button behind during shuffle
  for (let i = 0; i < times; i++) {
    shufflePositions()
    await new Promise(resolve => setTimeout(resolve, delay))
  }
  isShuffling.value = false
  showLoginButton.value = true
}

const handleLoginClick = async () => {
  if (isShuffling.value) return
  showLoginButton.value = false
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
  // Raise the selected cup
  raisedCups.value = cups.value.map((_, idx) => idx === clickedIndex)

  if (clickedIndex === targetCupIndex.value) {
    result.value = "Yeah"
  } else {
    result.value = "No"
  }
  // Optionally, bring the button to front after selection
  buttonZIndex.value = 10
}

</script>
