<script setup lang="ts">
import { ref } from 'vue';
import VueZoomable from 'vue-zoomable';

const zoom = ref(0.8);
const pan = ref({ x: 0, y: 0 });

const props = defineProps<{ id: string | undefined }>();
const emit = defineEmits<{ (e: "close"): void }>();

function close(_event: KeyboardEvent | MouseEvent) {
  zoom.value = 0.8;
  pan.value = { x: 0, y: 0 };
  emit("close")
}

</script>

<template>
  <Transition @keyup.escape="close">
    <div class="background" v-if="props.id" @keyup.escape="close" tabindex="0">
      <VueZoomable :min-zoom="0.5" :max-zoom="5" :enable-control-button="false" zoom-origin="pointer" selector="img"
        v-model:zoom="zoom" v-model:pan="pan" @click.self="close" @keyup.escape="close" class="center"
        title="Click outside the image to exit">
        <img title="Click outside the image to exit" :src="props.id" @keyup.escape="close" />
      </VueZoomable>
    </div>
  </Transition>
</template>

<style lang="css">
.background {
  transition: all 0.3s;

  background-color: rgba(0, 0, 0, 0.5);
  z-index: 10000;

  position: fixed;
  left: 0;
  top: 0;
  width: 100%;
  height: 100%;
}

.center {
  width: 100%;
  height: 100%;
}

.center>img {
  cursor: zoom-out;
}

.v-enter-active,
.v-leave-active {
  transition: opacity 0.3s ease;
}

.v-enter-from,
.v-leave-to {
  opacity: 0;
}
</style>
