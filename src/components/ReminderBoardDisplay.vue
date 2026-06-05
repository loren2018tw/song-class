<template>
  <div
    class="reminder-display d-flex flex-column align-center justify-center text-center fill-height"
  >
    <div
      v-if="board"
      class="content-container animate__animated animate__fadeIn"
    >
      <v-avatar size="160" color="orange" class="mb-8">
        <v-icon size="80" color="white">{{ board.icon }}</v-icon>
      </v-avatar>
      <h1 class="text-h1 font-weight-black mb-4">{{ board.title }}</h1>
      <p class="text-h3 font-weight-medium">{{ board.subtitle }}</p>
    </div>
    <div v-else class="empty-state">
      <p class="text-h4 text-grey">請選擇一個看板</p>
    </div>
  </div>
</template>

<script setup lang="ts">
import type { ReminderBoard } from "../types/reminderBoard";

defineProps<{
  board: ReminderBoard | null;
}>();
</script>

<style scoped>
.reminder-display {
  color: white;
  text-shadow: 2px 2px 8px rgba(0, 0, 0, 0.6);
  padding: 2rem;
}

.text-h1 {
  font-size: clamp(3rem, 10vh, 8rem) !important;
  line-height: 1.2;
}

.text-h3 {
  font-size: clamp(1.5rem, 4vh, 3rem) !important;
  opacity: 0.9;
}

.v-avatar {
  width: clamp(80px, 20vh, 160px) !important;
  height: clamp(80px, 20vh, 160px) !important;
}

.v-avatar .v-icon {
  font-size: clamp(40px, 10vh, 80px) !important;
}
</style>
