<script setup lang="ts">
import { computed, onMounted, reactive, ref, watch } from "vue";
import type {
  ContactBookTask,
  TaskCompletionFilter,
  TaskSubmissionsPayload,
  TaskTab,
} from "../types/contactBook";

const props = defineProps<{
  baseUrl: string;
  classroomId: number | null;
}>();

const emit = defineEmits<{
  (event: "error", message: string): void;
}>();

const activeTab = ref<TaskTab>("contact-book");
const selectedDate = ref(new Date().toISOString().slice(0, 10));
const completionFilter = ref<TaskCompletionFilter>("all");
const showAllUnfinished = ref(false);

const loadingTasks = ref(false);
const savingTask = ref(false);
const taskDialogVisible = ref(false);
const deleteDialogVisible = ref(false);
const taskToDelete = ref<ContactBookTask | null>(null);
const editingTask = ref<ContactBookTask | null>(null);
const taskFormError = ref("");

const tasks = ref<ContactBookTask[]>([]);
const submissionByTask = reactive(new Map<number, TaskSubmissionsPayload>());
const loadingSubmissionsByTask = reactive(new Map<number, boolean>());

const taskForm = reactive({
  task_date: selectedDate.value,
  title: "",
  show_in_contact_book: true,
  requires_tracking: true,
});

const completionFilterItems = [
  { label: "顯示全部", value: "all" as const },
  { label: "未完成", value: "unfinished" as const },
  { label: "已完成", value: "completed" as const },
];

const tabItems = [
  { title: "聯絡簿", value: "contact-book" as const },
  { title: "作業繳交管理", value: "submission" as const },
];

const canSaveTask = computed(() => {
  return (
    taskForm.title.trim().length > 0 &&
    !!taskForm.task_date &&
    (taskForm.show_in_contact_book || taskForm.requires_tracking)
  );
});

const contactBookDateHeading = computed(() => {
  const isoDate = selectedDate.value;
  const [year, month, day] = isoDate.split("-");
  if (!year || !month || !day) {
    return isoDate;
  }

  return `${year}年${month}月${day}日`;
});

function toApiUrl(path: string) {
  return new URL(path, props.baseUrl).toString();
}

async function apiRequest<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(toApiUrl(path), init);
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `HTTP ${response.status}`);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

function resetTaskForm() {
  taskForm.task_date = selectedDate.value;
  taskForm.title = "";
  taskForm.show_in_contact_book = true;
  taskForm.requires_tracking = true;
  taskFormError.value = "";
}

function openCreateTaskDialog() {
  editingTask.value = null;
  resetTaskForm();
  taskDialogVisible.value = true;
}

function openEditTaskDialog(task: ContactBookTask) {
  editingTask.value = task;
  taskForm.task_date = task.task_date;
  taskForm.title = task.title;
  taskForm.show_in_contact_book = task.show_in_contact_book;
  taskForm.requires_tracking = task.requires_tracking;
  taskFormError.value = "";
  taskDialogVisible.value = true;
}

function openDeleteTaskDialog(task: ContactBookTask) {
  taskToDelete.value = task;
  deleteDialogVisible.value = true;
}

function closeDeleteTaskDialog() {
  deleteDialogVisible.value = false;
  taskToDelete.value = null;
}

function buildListQuery() {
  const params = new URLSearchParams();
  params.set("tab", activeTab.value);

  if (showAllUnfinished.value && activeTab.value === "submission") {
    params.set("show_all_unfinished", "true");
  } else {
    params.set("date", selectedDate.value);
    if (activeTab.value === "submission") {
      params.set("completion", completionFilter.value);
    }
  }

  return params.toString();
}

function shiftSelectedDate(days: number) {
  const [yearRaw, monthRaw, dayRaw] = selectedDate.value.split("-");
  const year = Number(yearRaw);
  const month = Number(monthRaw);
  const day = Number(dayRaw);

  if (
    !Number.isFinite(year) ||
    !Number.isFinite(month) ||
    !Number.isFinite(day)
  ) {
    return;
  }

  const date = new Date(Date.UTC(year, month - 1, day));
  date.setUTCDate(date.getUTCDate() + days);

  const nextYear = date.getUTCFullYear();
  const nextMonth = String(date.getUTCMonth() + 1).padStart(2, "0");
  const nextDay = String(date.getUTCDate()).padStart(2, "0");
  selectedDate.value = `${nextYear}-${nextMonth}-${nextDay}`;
}

async function loadSubmissionsForTask(taskId: number) {
  loadingSubmissionsByTask.set(taskId, true);
  try {
    const payload = await apiRequest<TaskSubmissionsPayload>(
      `/api/contact-book/tasks/${taskId}/submissions`,
    );
    submissionByTask.set(taskId, payload);
  } catch (error) {
    emit("error", `載入任務繳交狀態失敗: ${String(error)}`);
  } finally {
    loadingSubmissionsByTask.set(taskId, false);
  }
}

async function loadTasks() {
  if (!props.classroomId) {
    tasks.value = [];
    submissionByTask.clear();
    return;
  }

  loadingTasks.value = true;
  try {
    const query = buildListQuery();
    const result = await apiRequest<ContactBookTask[]>(
      `/api/contact-book/tasks?${query}`,
    );
    tasks.value = result;

    submissionByTask.clear();
    if (activeTab.value === "submission") {
      await Promise.all(result.map((task) => loadSubmissionsForTask(task.id)));
    }
  } catch (error) {
    emit("error", `載入任務清單失敗: ${String(error)}`);
  } finally {
    loadingTasks.value = false;
  }
}

async function submitTask() {
  if (!canSaveTask.value) {
    taskFormError.value = "請確認任務名稱、日期，且至少勾選一項顯示規則";
    return;
  }

  savingTask.value = true;
  taskFormError.value = "";

  const body = {
    task_date: taskForm.task_date,
    title: taskForm.title.trim(),
    show_in_contact_book: taskForm.show_in_contact_book,
    requires_tracking: taskForm.requires_tracking,
  };

  try {
    if (editingTask.value) {
      await apiRequest<ContactBookTask>(
        `/api/contact-book/tasks/${editingTask.value.id}`,
        {
          method: "PATCH",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(body),
        },
      );
    } else {
      await apiRequest<ContactBookTask>("/api/contact-book/tasks", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
    }

    taskDialogVisible.value = false;
    await loadTasks();
  } catch (error) {
    taskFormError.value = `儲存任務失敗: ${String(error)}`;
  } finally {
    savingTask.value = false;
  }
}

async function confirmDeleteTask() {
  if (!taskToDelete.value) {
    return;
  }

  const deletingId = taskToDelete.value.id;
  try {
    await apiRequest<void>(`/api/contact-book/tasks/${deletingId}`, {
      method: "DELETE",
    });
    closeDeleteTaskDialog();
    await loadTasks();
  } catch (error) {
    emit("error", `刪除任務失敗: ${String(error)}`);
  }
}

async function setTaskCompletion(task: ContactBookTask, completed: boolean) {
  try {
    await apiRequest<ContactBookTask>(
      `/api/contact-book/tasks/${task.id}/completion`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ completed }),
      },
    );
    await loadTasks();
  } catch (error) {
    emit("error", `更新任務完成狀態失敗: ${String(error)}`);
  }
}

async function updateStudentSubmission(
  taskId: number,
  studentId: number,
  submitted: boolean,
) {
  try {
    await apiRequest<ContactBookTask>(
      `/api/contact-book/tasks/${taskId}/submissions/${studentId}`,
      {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ submitted }),
      },
    );

    const payload = submissionByTask.get(taskId);
    if (payload) {
      payload.submissions = payload.submissions.map((entry) =>
        entry.student_id === studentId ? { ...entry, submitted } : entry,
      );
      submissionByTask.set(taskId, { ...payload });
    }

    await loadTasks();
  } catch (error) {
    emit("error", `更新學生繳交狀態失敗: ${String(error)}`);
  }
}

watch(activeTab, async () => {
  if (activeTab.value !== "submission") {
    showAllUnfinished.value = false;
    completionFilter.value = "all";
  }

  await loadTasks();
});

watch([selectedDate, completionFilter, showAllUnfinished], async () => {
  await loadTasks();
});

watch(
  () => props.classroomId,
  async () => {
    submissionByTask.clear();
    await loadTasks();
  },
);

onMounted(async () => {
  await loadTasks();
});
</script>

<template>
  <v-card rounded="xl" elevation="6" class="contact-book-module h-100">
    <v-card-text class="d-flex flex-column ga-3 h-100">
      <v-tabs
        color="primary"
        density="comfortable"
        :model-value="activeTab"
        @update:model-value="activeTab = $event as TaskTab"
      >
        <v-tab v-for="tab in tabItems" :key="tab.value" :value="tab.value">
          {{ tab.title }}
        </v-tab>
      </v-tabs>

      <div class="d-flex flex-wrap align-center ga-3">
        <v-btn
          icon="mdi-chevron-left"
          variant="outlined"
          density="comfortable"
          :disabled="activeTab === 'submission' && showAllUnfinished"
          @click="shiftSelectedDate(-1)"
        />

        <v-text-field
          v-model="selectedDate"
          type="date"
          density="comfortable"
          variant="outlined"
          label="日期"
          hide-details
          :disabled="activeTab === 'submission' && showAllUnfinished"
          style="max-width: 220px"
        />

        <v-btn
          icon="mdi-chevron-right"
          variant="outlined"
          density="comfortable"
          :disabled="activeTab === 'submission' && showAllUnfinished"
          @click="shiftSelectedDate(1)"
        />

        <template v-if="activeTab === 'submission'">
          <v-btn-toggle
            :model-value="completionFilter"
            color="primary"
            divided
            mandatory
            @update:model-value="
              completionFilter = $event as TaskCompletionFilter
            "
          >
            <v-btn
              v-for="item in completionFilterItems"
              :key="item.value"
              :value="item.value"
              :disabled="showAllUnfinished"
            >
              {{ item.label }}
            </v-btn>
          </v-btn-toggle>

          <v-switch
            v-model="showAllUnfinished"
            color="warning"
            density="compact"
            hide-details
            label="顯示不限定日期所有未完成"
          />
        </template>

        <v-spacer />

        <v-btn
          color="primary"
          prepend-icon="mdi-plus"
          @click="openCreateTaskDialog"
        >
          新增任務
        </v-btn>
      </div>

      <v-progress-linear v-if="loadingTasks" indeterminate color="primary" />

      <div
        v-if="activeTab === 'contact-book'"
        class="task-list-wrap contact-book-board-wrap"
      >
        <v-card rounded="lg" class="contact-book-board" variant="flat">
          <v-card-text class="contact-book-board-content">
            <div class="contact-book-date-title">
              {{ contactBookDateHeading }}
            </div>

            <div
              v-if="!loadingTasks && tasks.length === 0"
              class="contact-book-empty"
            >
              本日尚無聯絡簿項目
            </div>

            <div
              v-for="(task, index) in tasks"
              :key="task.id"
              class="contact-book-entry"
            >
              <div class="contact-book-entry-text">
                {{ index + 1 }}. {{ task.title }}
              </div>
              <div class="d-flex align-center ga-1">
                <v-btn
                  icon="mdi-pencil"
                  size="small"
                  variant="outlined"
                  color="grey-lighten-4"
                  @click="openEditTaskDialog(task)"
                />
                <v-btn
                  icon="mdi-delete"
                  size="small"
                  variant="outlined"
                  color="grey-lighten-4"
                  @click="openDeleteTaskDialog(task)"
                />
              </div>
            </div>
          </v-card-text>
        </v-card>
      </div>

      <div v-else class="task-list-wrap d-flex flex-column ga-3">
        <v-card
          v-for="task in tasks"
          :key="task.id"
          rounded="lg"
          variant="outlined"
        >
          <v-card-text class="d-flex flex-column ga-3">
            <div class="d-flex align-start justify-space-between ga-2">
              <div class="submission-summary-row">
                <span class="submission-summary-title">{{ task.title }}</span>
                <span class="submission-summary-meta"
                  >日期 {{ task.task_date }}</span
                >
                <span class="submission-summary-meta"
                  >繳交 {{ task.submitted_count }}/{{
                    task.student_count
                  }}</span
                >
                <span
                  class="submission-summary-status"
                  :class="
                    task.is_completed
                      ? 'submission-summary-status--done'
                      : 'submission-summary-status--pending'
                  "
                >
                  {{ task.is_completed ? "已完成" : "未完成" }}
                </span>
              </div>

              <div class="d-flex align-center ga-1">
                <v-btn
                  v-if="activeTab === 'submission'"
                  size="small"
                  variant="tonal"
                  color="success"
                  @click="setTaskCompletion(task, true)"
                >
                  快速全部完成
                </v-btn>
                <v-btn
                  v-if="activeTab === 'submission'"
                  size="small"
                  variant="outlined"
                  color="warning"
                  @click="setTaskCompletion(task, false)"
                >
                  取消全部完成
                </v-btn>
                <v-btn
                  icon="mdi-pencil"
                  size="small"
                  variant="text"
                  @click="openEditTaskDialog(task)"
                />
                <v-btn
                  icon="mdi-delete"
                  size="small"
                  variant="text"
                  color="error"
                  @click="openDeleteTaskDialog(task)"
                />
              </div>
            </div>

            <div v-if="activeTab === 'submission'">
              <v-progress-linear
                v-if="loadingSubmissionsByTask.get(task.id)"
                indeterminate
                color="teal"
              />

              <div v-else class="d-flex flex-wrap ga-2">
                <div
                  v-for="entry in submissionByTask.get(task.id)?.submissions ??
                  []"
                  :key="`${task.id}-${entry.student_id}`"
                  class="submission-chip"
                >
                  <v-checkbox-btn
                    :model-value="entry.submitted"
                    color="success"
                    density="compact"
                    @update:model-value="
                      updateStudentSubmission(
                        task.id,
                        entry.student_id,
                        $event === true,
                      )
                    "
                  />
                  <span>{{ entry.display_name }}</span>
                </div>
              </div>
            </div>
          </v-card-text>
        </v-card>

        <v-card
          v-if="!loadingTasks && tasks.length === 0"
          rounded="lg"
          variant="tonal"
        >
          <v-card-text class="text-medium-emphasis"
            >目前沒有符合條件的任務</v-card-text
          >
        </v-card>
      </div>
    </v-card-text>
  </v-card>

  <v-dialog v-model="taskDialogVisible" max-width="560">
    <v-card rounded="lg">
      <v-card-title class="text-h6">{{
        editingTask ? "編輯任務" : "新增任務"
      }}</v-card-title>
      <v-card-text class="d-flex flex-column ga-3">
        <v-text-field
          v-model="taskForm.task_date"
          type="date"
          variant="outlined"
          label="日期"
          density="comfortable"
        />
        <v-text-field
          v-model="taskForm.title"
          variant="outlined"
          label="任務名稱"
          density="comfortable"
        />
        <v-switch
          v-model="taskForm.show_in_contact_book"
          color="primary"
          hide-details
          label="顯示在聯絡簿"
        />
        <v-switch
          v-model="taskForm.requires_tracking"
          color="teal"
          hide-details
          label="需要控管繳交"
        />
        <v-alert v-if="taskFormError" type="error" variant="tonal">{{
          taskFormError
        }}</v-alert>
      </v-card-text>
      <v-card-actions class="justify-end">
        <v-btn variant="text" @click="taskDialogVisible = false">取消</v-btn>
        <v-btn :loading="savingTask" color="primary" @click="submitTask"
          >儲存</v-btn
        >
      </v-card-actions>
    </v-card>
  </v-dialog>

  <v-dialog v-model="deleteDialogVisible" max-width="460">
    <v-card rounded="lg">
      <v-card-title class="text-h6">刪除任務</v-card-title>
      <v-card-text>
        確定要刪除「{{ taskToDelete?.title ?? "" }}」嗎？
      </v-card-text>
      <v-card-actions class="justify-end">
        <v-btn variant="text" @click="closeDeleteTaskDialog">取消</v-btn>
        <v-btn color="error" @click="confirmDeleteTask">刪除</v-btn>
      </v-card-actions>
    </v-card>
  </v-dialog>
</template>

<style scoped>
.contact-book-module {
  overflow: hidden;
}

.task-list-wrap {
  min-height: 0;
  overflow: auto;
  padding-right: 2px;
}

.contact-book-board-wrap {
  padding-right: 0;
}

.contact-book-board {
  min-height: 520px;
  border: 12px solid #e5d2ad;
  border-radius: 10px;
  background-image:
    radial-gradient(
      circle at 22% 24%,
      rgba(255, 255, 255, 0.06),
      transparent 38%
    ),
    radial-gradient(
      circle at 70% 65%,
      rgba(255, 255, 255, 0.04),
      transparent 44%
    ),
    linear-gradient(180deg, rgba(20, 70, 55, 0.96), rgba(20, 80, 60, 0.96)),
    url("../assets/bg/empty.png");
  background-size: cover;
  background-position: center;
  box-shadow: inset 0 0 0 2px rgba(0, 0, 0, 0.3);
}

.contact-book-board-content {
  color: rgba(242, 248, 241, 0.98);
}

.contact-book-date-title {
  font-size: 2rem;
  font-weight: 700;
  letter-spacing: 0.05em;
  margin-bottom: 12px;
}

.contact-book-entry {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 10px 0;
  border-bottom: 1px solid rgba(238, 244, 236, 0.18);
}

.contact-book-entry-text {
  font-size: 1.15rem;
  line-height: 1.4;
}

.contact-book-empty {
  color: rgba(236, 243, 233, 0.82);
  font-size: 1.05rem;
}

.submission-chip {
  display: inline-flex;
  align-items: center;
  border: 1px solid rgba(var(--v-theme-on-surface), 0.18);
  border-radius: 12px;
  padding-right: 8px;
  background: rgba(var(--v-theme-surface), 0.86);
}

.submission-summary-row {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  flex-wrap: nowrap;
}

.submission-summary-title {
  font-size: 1.08rem;
  font-weight: 800;
  color: #4b0f0f;
  white-space: nowrap;
}

.submission-summary-meta {
  font-size: 1.02rem;
  color: rgba(var(--v-theme-on-surface), 0.74);
  white-space: nowrap;
}

.submission-summary-status {
  font-size: 1.02rem;
  font-weight: 700;
  white-space: nowrap;
}

.submission-summary-status--done {
  color: #1b8f5a;
}

.submission-summary-status--pending {
  color: #d46b08;
}
</style>
