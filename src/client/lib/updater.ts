import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";

export type UpdaterStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "current"
  | "error"
  | "portable";

export type Updater = {
  status: UpdaterStatus;
  update: Update | null;
  progress: number;
  notice: string | null;
  version: string;
  checkNow: (manual?: boolean) => Promise<void>;
  install: () => Promise<void>;
  dismiss: () => void;
};

type AppInfo = { version: string; portable: boolean };

/**
 * Update state shared by the header button and the banner. The check is
 * silent on startup and explicit when the user presses the button.
 */
export function useUpdater(): Updater {
  const [status, setStatus] = useState<UpdaterStatus>("idle");
  const [update, setUpdate] = useState<Update | null>(null);
  const [progress, setProgress] = useState(0);
  const [notice, setNotice] = useState<string | null>(null);
  const [version, setVersion] = useState("");
  const portableRef = useRef(false);
  const noticeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flash = useCallback((text: string) => {
    setNotice(text);
    if (noticeTimer.current) clearTimeout(noticeTimer.current);
    noticeTimer.current = setTimeout(() => setNotice(null), 4500);
  }, []);

  const checkNow = useCallback(
    async (manual = true) => {
      if (portableRef.current) {
        await invoke("open_releases").catch(() => undefined);
        flash("Портативная версия обновляется вручную — открыта страница релизов");
        return;
      }

      setStatus("checking");
      try {
        const found = await check();
        if (found) {
          setUpdate(found);
          setStatus("available");
        } else {
          setUpdate(null);
          setStatus("current");
          if (manual) flash("Установлена последняя версия");
        }
      } catch (cause) {
        setStatus("error");
        if (manual) {
          flash("Не удалось проверить обновления. Проверьте подключение к сети.");
        }
        if (import.meta.env.DEV) console.warn("updater", cause);
      }
    },
    [flash],
  );

  useEffect(() => {
    let cancelled = false;
    let startupTimer: ReturnType<typeof setTimeout> | null = null;

    (async () => {
      try {
        const info = await invoke<AppInfo>("app_info");
        if (cancelled) return;
        setVersion(info.version);
        portableRef.current = info.portable;
        if (info.portable) {
          setStatus("portable");
          return;
        }
        if (import.meta.env.DEV) return;

        // Automatic, silent check shortly after the window is usable.
        startupTimer = setTimeout(() => {
          if (!cancelled) void checkNow(false);
        }, 900);
      } catch {
        /* app_info is always available in a real build */
      }
    })();

    return () => {
      cancelled = true;
      if (startupTimer) clearTimeout(startupTimer);
      if (noticeTimer.current) clearTimeout(noticeTimer.current);
    };
  }, [checkNow]);

  const install = useCallback(async () => {
    if (!update) return;
    setStatus("downloading");
    setProgress(0);

    try {
      let total = 0;
      let downloaded = 0;

      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          total = event.data.contentLength ?? 0;
        } else if (event.event === "Progress") {
          downloaded += event.data.chunkLength;
          if (total > 0) setProgress(Math.min(100, Math.round((downloaded / total) * 100)));
        }
      });

      await relaunch();
    } catch {
      setStatus("error");
      flash("Не удалось установить обновление");
    }
  }, [update, flash]);

  const dismiss = useCallback(() => {
    setUpdate(null);
    setStatus("idle");
  }, []);

  return { status, update, progress, notice, version, checkNow, install, dismiss };
}
