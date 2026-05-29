<script setup lang="ts">
import { computed, ref, watch } from "vue";

const props = withDefaults(
  defineProps<{
    initialNickname?: string;
  }>(),
  {
    initialNickname: "",
  },
);

const emit = defineEmits<{
  submit: [nickname: string];
}>();

const nickname = ref(props.initialNickname);
const error = ref("");

const hasError = computed(() => error.value.length > 0);

function onSubmit() {
  const trimmed = nickname.value.trim();
  if (!trimmed) {
    error.value = "請輸入有效暱稱";
    return;
  }
  error.value = "";
  emit("submit", trimmed);
}

watch(
  () => props.initialNickname,
  (nextNickname) => {
    nickname.value = nextNickname;
  },
);
</script>

<template>
  <v-card rounded="xl" elevation="6">
    <v-card-title>加入課堂</v-card-title>
    <v-card-text>
      <v-text-field
        v-model="nickname"
        label="學生暱稱"
        placeholder="請輸入暱稱"
        :error="hasError"
        :error-messages="error"
        variant="outlined"
        hide-details="auto"
      />
      <v-btn color="primary" block class="mt-4" @click="onSubmit"
        >確認加入</v-btn
      >
    </v-card-text>
  </v-card>
</template>
