<template>
  <v-sheet
    color="black"
    height="80"
    class="d-flex align-center px-4 rounded-b-xl"
  >
    <div class="d-flex align-center w-100 px-4">
      <!-- 左側：語音開關 -->
      <v-btn
        :icon="isVoiceEnabled ? 'mdi-volume-high' : 'mdi-volume-off'"
        :color="isVoiceEnabled ? 'orange' : 'grey-lighten-1'"
        variant="tonal"
        class="mr-4"
        @click="$emit('toggle-voice')"
      />

      <!-- 中間：分類按鈕 -->
      <v-spacer />
      <div class="d-flex ga-4">
        <v-btn
          :color="category === '移動' ? 'orange' : 'grey-darken-3'"
          :variant="category === '移動' ? 'flat' : 'tonal'"
          rounded="lg"
          prepend-icon="mdi-run-fast"
          class="px-6"
          @click="$emit('change-category', '移動')"
        >
          移動
        </v-btn>
        <v-btn
          :color="category === '作息' ? 'orange' : 'grey-darken-3'"
          :variant="category === '作息' ? 'flat' : 'tonal'"
          rounded="lg"
          prepend-icon="mdi-calendar-clock"
          class="px-6"
          @click="$emit('change-category', '作息')"
        >
          作息
        </v-btn>
        <v-btn
          :color="category === '溫馨提醒' ? 'orange' : 'grey-darken-3'"
          :variant="category === '溫馨提醒' ? 'flat' : 'tonal'"
          rounded="lg"
          prepend-icon="mdi-heart-outline"
          class="px-6"
          @click="$emit('change-category', '溫馨提醒')"
        >
          溫馨提醒
        </v-btn>
        <v-btn
          :color="category === '自訂' ? 'orange' : 'grey-darken-3'"
          :variant="category === '自訂' ? 'flat' : 'tonal'"
          rounded="lg"
          prepend-icon="mdi-star"
          class="px-6"
          @click="$emit('change-category', '自訂')"
        >
          自訂
        </v-btn>
      </div>
      <v-spacer />

      <!-- 右側：設定按鈕 -->
      <v-btn
        icon="mdi-cog"
        color="grey-lighten-1"
        variant="text"
        @click="$emit('open-settings')"
      />
    </div>
  </v-sheet>
</template>

<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{
  isVoiceEnabled: boolean;
  category: "移動" | "作息" | "自訂" | "溫馨提醒";
}>();

const isVoiceEnabled = computed(() => props.isVoiceEnabled);
const category = computed(() => props.category);

defineEmits<{
  (e: "toggle-voice"): void;
  (e: "change-category", category: "移動" | "作息" | "自訂" | "溫馨提醒"): void;
  (e: "open-settings"): void;
}>();
</script>

<style scoped>
.v-app-bar {
  border-top: 1px solid rgba(255, 255, 255, 0.1) !important;
}
</style>
