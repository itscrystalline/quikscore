<script setup lang="ts">
export type ProgressBarProps = | {
  type: "progress"
  progressTop: number,
  progressBottom: number,
  max: number
} | { type: "indeterminate" };

const props = defineProps<ProgressBarProps>();
</script>

<template>
  <div class="progressbar">
    <div class="layer bottom" v-if="props.type == 'progress'"
      :style="{ width: (props.progressBottom * 100 / props.max) + '%' }">
    </div>
    <div class="layer top" v-if="props.type == 'progress'"
      :style="{ width: (props.progressTop * 100 / props.max) + '%' }">
    </div>
    <div class="layer indeterminate" v-else-if="props.type == 'indeterminate'">
    </div>
  </div>
</template>

<style>
.progressbar {
  width: 80%;
  margin: 0 auto;
  height: 1vh;
  background-color: #445165;
  border-radius: 2vh;
  overflow: hidden;

  display: grid;
  place-items: left;
  grid-template-areas: "inner-div";
}

.layer {
  grid-area: inner-div;
  height: 100%;
  border-radius: 2vh;
}

.top {
  background-color: #3b82f6;
}

.bottom {
  background-color: #10b981;
}

.indeterminate {
  width: 100%;
  background: repeating-linear-gradient(to right,
      #f5e0dc 0%,
      #f2cdcd 7.14%,
      #f5c2e7 14.28%,
      #cba6f7 21.42%,
      #f38ba8 28.56%,
      #eba0ac 35.7%,
      #fab387 42.84%,
      #f9e2af 49.98%,
      #a6e3a1 57.12%,
      #94e2d5 64.26%,
      #89dceb 71.4%,
      #74c7ec 78.54%,
      #89b4fa 85.68%,
      #b4befe 92.82%,
      #f5e0dc 100%);
  background-size: 500% auto;
  background-position: 0 100%;
  animation: gradient 10s infinite;
  animation-fill-mode: forwards;
  animation-timing-function: linear;
}

@keyframes gradient {
  0% {
    background-position: 0 0;
  }

  100% {
    background-position: -500% 0;
  }
}
</style>
