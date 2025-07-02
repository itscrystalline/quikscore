<script setup lang="ts">
type ProgressBarProps = | {
  type: "progress"
  progressA: number,
  progressB: number,
  max: number
} | { type: "indeterminate" };

const props = defineProps<ProgressBarProps>();
</script>

<template>
  <div class="progressbar">
    <div class="layer-a" v-if="props.type == 'progress'">
    </div>
    <div class="layer-b" v-if="props.type == 'progress'">
    </div>
    <div class="indeterminate" v-else-if="props.type == 'indeterminate'">
    </div>
  </div>
</template>

<style></style>
