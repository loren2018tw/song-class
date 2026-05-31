<script setup lang="ts">
import type { StudentSession } from "../types/session";

defineProps<{
  title: string;
  students: StudentSession[];
}>();

function normalizeFocusStatus(student: StudentSession): "focused" | "away" {
  return student.focus_status === "away" ? "away" : "focused";
}

function nicknameChipColor(student: StudentSession): string {
  if (!student.connected) {
    return "grey-darken-1";
  }

  return normalizeFocusStatus(student) === "away" ? "warning" : "success";
}

function nicknameChipLabel(student: StudentSession): string {
  return student.nickname || "（未提供暱稱）";
}
</script>

<template>
  <v-card rounded="xl" elevation="6" class="h-100">
    <v-card-title class="d-flex justify-space-between align-center">
      <span>{{ title }}</span>
      <v-chip color="primary" size="small" variant="tonal"
        >{{ students.length }} 人</v-chip
      >
    </v-card-title>
    <v-card-text>
      <div v-if="students.length > 0" class="student-chip-grid">
        <v-chip
          v-for="student in students"
          :key="student.connection_id"
          :color="nicknameChipColor(student)"
          size="small"
          variant="flat"
          class="student-chip-item"
        >
          {{ nicknameChipLabel(student) }}
        </v-chip>
      </div>
      <v-alert v-else type="info" variant="tonal">尚無學生連入</v-alert>
    </v-card-text>
  </v-card>
</template>

<style scoped>
.student-chip-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.student-chip-item {
  max-width: 100%;
}
</style>
