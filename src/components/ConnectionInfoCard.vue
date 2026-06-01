<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  title: string;
  statusLabel: string;
  serverUrl: string;
  ip: string;
  errorMessage?: string | null;
}>();

const statusColor = computed(() => {
  if (props.errorMessage) return "error";
  if (props.statusLabel === "可連線") return "success";
  if (props.statusLabel === "啟動中") return "warning";
  return "grey";
});
</script>

<template>
  <v-card rounded="xl" elevation="6" class="h-100">
    <v-card-title class="d-flex justify-space-between align-center">
      <span>{{ title }}</span>
      <v-chip :color="statusColor" size="small" variant="flat">{{
        statusLabel
      }}</v-chip>
    </v-card-title>
    <v-card-text>
      <v-alert v-if="errorMessage" type="error" variant="tonal" class="mb-3">
        {{ errorMessage }}
      </v-alert>
      <div class="text-caption text-medium-emphasis">連線網址</div>
      <div class="text-body-1 font-weight-bold mb-3">{{ serverUrl }}</div>
      <div class="text-caption text-medium-emphasis">本機 IP</div>
      <div class="text-body-1 font-weight-bold">{{ ip }}</div>
    </v-card-text>
  </v-card>
</template>
