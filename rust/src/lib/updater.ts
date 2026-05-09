import { getVersion } from "@tauri-apps/api/app";
import type { Update } from "@tauri-apps/plugin-updater";

export interface UpdateInfo {
  currentVersion: string;
  availableVersion: string;
  notes?: string;
  pubDate?: string;
}

export interface UpdateProgressEvent {
  event: "Started" | "Progress" | "Finished";
  total?: number;
  downloaded?: number;
}

export interface UpdateHandle {
  version: string;
  notes?: string;
  date?: string;
  downloadAndInstall: (
    onProgress?: (e: UpdateProgressEvent) => void,
  ) => Promise<void>;
}

function mapUpdateHandle(raw: Update): UpdateHandle {
  return {
    version: raw.version ?? "",
    notes: raw.body ?? undefined,
    date: raw.date ?? undefined,
    async downloadAndInstall(onProgress) {
      await raw.downloadAndInstall((evt) => {
        if (!onProgress) return;
        if (evt.event === "Started") {
          onProgress({ event: "Started", total: evt.data.contentLength ?? 0, downloaded: 0 });
        } else if (evt.event === "Progress") {
          onProgress({ event: "Progress", downloaded: evt.data.chunkLength ?? 0 });
        } else if (evt.event === "Finished") {
          onProgress({ event: "Finished" });
        }
      });
    },
  };
}

export async function getCurrentVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "";
  }
}

export async function checkForUpdate(): Promise<
  | { status: "up-to-date" }
  | { status: "available"; info: UpdateInfo; update: UpdateHandle }
> {
  // 动态 import，避免开发环境 vite HMR 时未注册插件报错
  const { check } = await import("@tauri-apps/plugin-updater");
  const currentVersion = await getCurrentVersion();
  const update = await check({ timeout: 30000 });
  if (!update) return { status: "up-to-date" };

  const handle = mapUpdateHandle(update);
  return {
    status: "available",
    info: {
      currentVersion,
      availableVersion: handle.version,
      notes: handle.notes,
      pubDate: handle.date,
    },
    update: handle,
  };
}

export async function relaunchApp(): Promise<void> {
  const { relaunch } = await import("@tauri-apps/plugin-process");
  await relaunch();
}
