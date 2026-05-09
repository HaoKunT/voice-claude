import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";
import { checkForUpdate, relaunchApp, UpdateHandle, UpdateInfo } from "../lib/updater";

export type DownloadPhase =
  | "idle"
  | "downloading"
  | "installing"
  | "finished"
  | "error";

interface UpdateContextValue {
  hasUpdate: boolean;
  updateInfo: UpdateInfo | null;
  isChecking: boolean;
  error: string | null;

  phase: DownloadPhase;
  progress: number; // 0..1
  downloadedBytes: number;
  totalBytes: number;

  checkUpdate: () => Promise<void>;
  downloadAndInstall: () => Promise<void>;
  relaunch: () => Promise<void>;
}

const UpdateContext = createContext<UpdateContextValue | undefined>(undefined);

export function UpdateProvider({ children }: { children: ReactNode }) {
  const [hasUpdate, setHasUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [isChecking, setIsChecking] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [phase, setPhase] = useState<DownloadPhase>("idle");
  const [downloadedBytes, setDownloadedBytes] = useState(0);
  const [totalBytes, setTotalBytes] = useState(0);

  const handleRef = useRef<UpdateHandle | null>(null);
  const checkingRef = useRef(false);

  const checkUpdate = useCallback(async () => {
    if (checkingRef.current) return;
    checkingRef.current = true;
    setIsChecking(true);
    setError(null);
    try {
      const result = await checkForUpdate();
      if (result.status === "available") {
        setHasUpdate(true);
        setUpdateInfo(result.info);
        handleRef.current = result.update;
      } else {
        setHasUpdate(false);
        setUpdateInfo(null);
        handleRef.current = null;
      }
    } catch (e) {
      // 离线 / endpoint 没 latest.json / 首发版本 都会走到这里，静默降级
      console.warn("checkForUpdate failed:", e);
      setError(e instanceof Error ? e.message : String(e));
      setHasUpdate(false);
    } finally {
      setIsChecking(false);
      checkingRef.current = false;
    }
  }, []);

  const downloadAndInstall = useCallback(async () => {
    if (!handleRef.current) return;
    setPhase("downloading");
    setDownloadedBytes(0);
    setTotalBytes(0);
    setError(null);
    try {
      await handleRef.current.downloadAndInstall((evt) => {
        if (evt.event === "Started") {
          setTotalBytes(evt.total ?? 0);
          setDownloadedBytes(0);
        } else if (evt.event === "Progress") {
          setDownloadedBytes((n) => n + (evt.downloaded ?? 0));
        } else if (evt.event === "Finished") {
          setPhase("installing");
        }
      });
      setPhase("finished");
    } catch (e) {
      setPhase("error");
      setError(e instanceof Error ? e.message : String(e));
    }
  }, []);

  const relaunch = useCallback(async () => {
    await relaunchApp();
  }, []);

  // 启动后 1s 自动检查一次
  useEffect(() => {
    const timer = setTimeout(() => {
      checkUpdate();
    }, 1000);
    return () => clearTimeout(timer);
  }, [checkUpdate]);

  const progress = totalBytes > 0 ? Math.min(1, downloadedBytes / totalBytes) : 0;

  return (
    <UpdateContext.Provider
      value={{
        hasUpdate,
        updateInfo,
        isChecking,
        error,
        phase,
        progress,
        downloadedBytes,
        totalBytes,
        checkUpdate,
        downloadAndInstall,
        relaunch,
      }}
    >
      {children}
    </UpdateContext.Provider>
  );
}

export function useUpdate(): UpdateContextValue {
  const ctx = useContext(UpdateContext);
  if (!ctx) throw new Error("useUpdate must be used within UpdateProvider");
  return ctx;
}
