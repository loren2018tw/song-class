<script setup lang="ts">
import { computed } from "vue";
import StudentView from "./views/StudentView.vue";
import TeacherView from "./views/TeacherView.vue";
import WebTeacherView from "./views/WebTeacherView.vue";

const query = new URLSearchParams(window.location.search);
const mode = query.get("mode") ?? "control";
const baseUrl = query.get("base") ?? window.location.origin;

const isTeacherMode = computed(() => mode === "teacher");
const isStudentMode = computed(() => mode === "student");
</script>

<template>
  <v-app>
    <v-main class="app-bg">
      <TeacherView v-if="!isTeacherMode && !isStudentMode" />
      <WebTeacherView v-else-if="isTeacherMode" :base-url="baseUrl" />
      <StudentView v-else :base-url="baseUrl" />
    </v-main>
  </v-app>
</template>

<style>
:root {
  font-family: "Noto Sans TC", "PingFang TC", sans-serif;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

body {
  margin: 0;
}

.app-bg {
  min-height: 100vh;
  background: radial-gradient(
    circle at 12% 20%,
    #f4f4d7 0%,
    #f6f9ef 48%,
    #dbeefe 100%
  );
}
</style>
