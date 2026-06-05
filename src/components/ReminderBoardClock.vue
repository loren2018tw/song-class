<template>
  <div class="reminder-clock text-center">
    <div class="time-text font-weight-bold mb-1">{{ currentTime }}</div>
    <div class="date-text">{{ currentDate }} {{ currentDay }}</div>
  </div>
</template>

<style scoped>
.reminder-clock {
  color: white;
  text-shadow: 0 2px 4px rgba(0, 0, 0, 0.5);
}

.time-text {
  font-size: clamp(2rem, 6vh, 4rem);
  line-height: 1;
}

.date-text {
  font-size: clamp(1rem, 2.5vh, 1.5rem);
  opacity: 0.8;
}
</style>

<script setup lang="ts">
import { ref, onMounted, onUnmounted } from "vue";

const currentTime = ref("");
const currentDate = ref("");
const currentDay = ref("");

const days = [
  "星期日",
  "星期一",
  "星期二",
  "星期三",
  "星期四",
  "星期五",
  "星期六",
];

const updateTime = () => {
  const now = new Date();

  // 格式: 10:44:45
  currentTime.value = now.toLocaleTimeString("zh-TW", {
    hour12: false,
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });

  // 格式: 6月2日
  currentDate.value = `${now.getMonth() + 1}月${now.getDate()}日`;

  // 格式: 星期二
  currentDay.value = days[now.getDay()];
};

let timer: number;

onMounted(() => {
  updateTime();
  timer = window.setInterval(updateTime, 1000);
});

onUnmounted(() => {
  if (timer) clearInterval(timer);
});
</script>

<style scoped>
.reminder-clock {
  color: white;
  text-shadow: 2px 2px 4px rgba(0, 0, 0, 0.5);
  background: rgba(0, 0, 0, 0.3);
  padding: 1rem 2rem;
  border-radius: 8px;
  display: inline-block;
}
</style>
