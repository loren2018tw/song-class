<script setup lang="ts">
import type { ClassroomStudent, ClassroomSummary } from "../types/session";

const props = defineProps<{
  classroom: ClassroomSummary | null;
  students: ClassroomStudent[];
  loading?: boolean;
}>();

const emit = defineEmits<{
  submit: [student: ClassroomStudent];
}>();

function onJoin(student: ClassroomStudent) {
  if (student.occupied || props.loading) {
    return;
  }

  emit("submit", student);
}

function studentButtonLabel(student: ClassroomStudent): string {
  const nickname = student.nickname.trim();
  return nickname ? `${student.seat_no_text}${nickname}` : student.seat_no_text;
}
</script>

<template>
  <v-card rounded="xl" elevation="6">
    <v-card-title class="d-flex align-center justify-space-between">
      <span>加入課堂</span>
      <v-chip size="small" variant="tonal" color="primary">
        {{ classroom?.name ?? "未選擇班級" }}
      </v-chip>
    </v-card-title>
    <v-card-text>
      <v-alert
        v-if="students.length === 0"
        type="warning"
        variant="tonal"
        class="mb-3"
      >
        目前班級尚無學生名單
      </v-alert>

      <div v-else class="roster-grid">
        <v-btn
          v-for="student in students"
          :key="student.id"
          class="roster-btn"
          :disabled="student.occupied || loading"
          :variant="student.occupied ? 'outlined' : 'tonal'"
          :color="student.occupied ? 'grey-darken-1' : 'primary'"
          @click="onJoin(student)"
        >
          {{ studentButtonLabel(student) }}
        </v-btn>
      </div>
    </v-card-text>
  </v-card>
</template>

<style scoped>
.roster-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(104px, 1fr));
  gap: 8px;
}

.roster-btn {
  min-height: 38px;
}
</style>
