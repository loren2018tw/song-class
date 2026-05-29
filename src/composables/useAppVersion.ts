import { invoke } from "@tauri-apps/api/core";
import { computed, onMounted, ref } from "vue";

const VERSION_FOOTER = "Loren(loren@.gmail.com)";

type AppVersionResponse = {
  version: string;
};

function normalizeBaseUrl(baseUrl?: string): string {
  if (!baseUrl || baseUrl.trim() === "") {
    return window.location.origin;
  }

  return baseUrl;
}

async function fetchVersionFromHttp(baseUrl?: string): Promise<string | null> {
  try {
    const target = new URL("/api/app/version", normalizeBaseUrl(baseUrl));
    const response = await fetch(target.toString());

    if (!response.ok) {
      return null;
    }

    const payload = (await response.json()) as AppVersionResponse;
    if (!payload.version) {
      return null;
    }

    return payload.version;
  } catch {
    return null;
  }
}

export function useAppVersion(baseUrl?: string) {
  const version = ref<string>("-");

  const label = computed(() => `V${version.value} ${VERSION_FOOTER}`);

  async function resolveVersion() {
    try {
      const tauriVersion = await invoke<string>("get_app_version");
      if (tauriVersion) {
        version.value = tauriVersion;
        return;
      }
    } catch {
      // In browser-only contexts, fall back to HTTP endpoint served by Rust.
    }

    const httpVersion = await fetchVersionFromHttp(baseUrl);
    if (httpVersion) {
      version.value = httpVersion;
    }
  }

  onMounted(() => {
    void resolveVersion();
  });

  return {
    appVersionLabel: label,
  };
}
