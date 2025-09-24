

<template>
  <header>
    <img class="logo" src="/src/assets/logo_fit.png" alt="quikscore logo">
    <span class="logo_text">uikscore</span>
  </header>
  <main class="container">
    <form class="login_form" @submit.prevent="handleLogin">
      <div class="form_group">
        <label for="username">Username</label>
        <input 
          v-model="username" 
          type="text" 
          id="username" 
          placeholder="Enter username"
          required 
        />
      </div>

      <div class="form_group">
        <label for="password">Password</label>
        <input 
          v-model="password" 
          type="password" 
          id="password" 
          placeholder="Enter password"
          required 
        />
      </div>

      <button type="submit" class="button">Login</button>
    </form>
  </main>
</template>

<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 19px;
  line-height: 1.5;
  font-weight: 400;

  color: #cdd6f4;
  background-color: #111827;

  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

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

.login_form {
  display: flex;
  flex-direction: column;
  background: #1e293b;
  padding: 2em;
  border-radius: 1rem;
  box-shadow: 0 8px 20px rgba(0,0,0,0.4);
  width: 300px;
  gap: 1.2em;
}

.form_group {
  display: flex;
  flex-direction: column;
}

label {
  margin-bottom: 0.4em;
  font-weight: 500;
  font-size: 0.9em;
  color: #cdd6f4;
}

input {
  padding: 0.6em;
  border-radius: 0.5em;
  border: 1px solid #334155;
  background: #0f172a;
  color: #cdd6f4;
  font-size: 1em;
}

input:focus {
  outline: none;
  border-color: #4f46e5;
  box-shadow: 0 0 0 2px rgba(79,70,229,0.5);
}

.button {
  padding: 0.8em;
  border: none;
  border-radius: 0.8em;
  background: #4f46e5;
  color: white;
  font-weight: 600;
  font-size: 1em;
  cursor: pointer;
  transition: background 0.2s ease;
}

.button:hover {
  background: #4338ca;
}
</style>

<script setup>
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const username = ref("")
const password = ref("")

async function handleLogin() {
  await invoke("login", {
    username: username.value,
    password: password.value
  });
}
</script>
