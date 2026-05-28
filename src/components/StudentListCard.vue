<script setup lang="ts">
import type { StudentSession } from "../types/session";

defineProps<{
  title: string;
  students: StudentSession[];
}>();
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
      <v-list v-if="students.length > 0" density="comfortable" lines="one">
        <v-list-item
          v-for="student in students"
          :key="student.connection_id"
          rounded="lg"
        >
          <v-list-item-title class="font-weight-medium">
            {{ student.nickname || "（未提供暱稱）" }}
          </v-list-item-title>
          <template #append>
            <v-chip color="success" size="x-small" variant="flat"
              >連線中</v-chip
            >
          </template>
        </v-list-item>
      </v-list>
      <v-alert v-else type="info" variant="tonal">尚無學生連入</v-alert>
    </v-card-text>
  </v-card>
</template>
