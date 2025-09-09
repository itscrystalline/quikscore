<script setup lang="ts">
const props = defineProps<{ id: string | undefined }>();
defineEmits<{ (e: "close"): void }>();
</script>

<template>
  <div class="background" v-if="props.id" @click="$emit('close')">
    <img :src="props.id" />
  </div>

</template>

<style lang="css">
.background {
  background-color: rgba(0, 0, 0, 0.5);
  z-index: 10000;

  position: fixed;
  left: 0;
  top: 0;
  width: 100%;
  height: 100%;
}
</style>
