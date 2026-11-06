import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { relaunch } from "@tauri-apps/plugin-process";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { t } from "@/client/i18n";

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
  portable: boolean;
  checkNow: (manual?: boolean) => Promise<void>;
  install: () => Promise<void>;
  dismiss: () => void;
  skipVersion: () => void;
};

const SKIPPED_KEY = "diskmaster.skippedVersion";
const LAST_CHECK_KEY = "diskmaster.lastUpdateCheck";
const CHECK_INTERVAL_MS = 24 * 60 * 60 * 1000;

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
  const [portable, setPortable] = useState(false);
  const portableRef = useRef(false);
  const skippedRef = useRef<string | null>(
    typeof localStorage === "undefined" ? null : localStorage.getItem(SKIPPED_KEY),
  );
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
        flash(t("Портативная версия обновляется вручную — открыта страница релизов"));
        return;
      }

      setStatus("checking");
      try {
        const found = await check();
        localStorage.setItem(LAST_CHECK_KEY, String(Date.now()));

        if (found) {
          if (!manual && skippedRef.current === found.version) {
            setUpdate(null);
            setStatus("idle");
          } else {
            setUpdate(found);
            setStatus("available");
          }
        } else {
          setUpdate(null);
          setStatus("current");
          if (manual) flash(t("Установлена последняя версия"));
        }
      } catch (cause) {
        setStatus("error");
        if (manual) {
          flash(t("Не удалось проверить обновления. Проверьте подключение к сети."));
        }
        void invoke("log_event", {
          level: "warn",
          message: `update check failed: ${String(cause)}`,
        }).catch(() => undefined);
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
        setPortable(info.portable);
        portableRef.current = info.portable;
        if (info.portable) {
          setStatus("portable");
          return;
        }
        if (import.meta.env.DEV) return;

        // Automatic check at most once per 24 hours.
        const last = Number(localStorage.getItem(LAST_CHECK_KEY) ?? 0);
        if (Date.now() - last > CHECK_INTERVAL_MS) {
          startupTimer = setTimeout(() => {
            if (!cancelled) void checkNow(false);
          }, 900);
        }
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
    } catch (cause) {
      setStatus("error");
      flash(t("Не удалось установить обновление"));
      void invoke("log_event", {
        level: "error",
        message: `update install failed: ${String(cause)}`,
      }).catch(() => undefined);
    }
  }, [update, flash]);

  const dismiss = useCallback(() => {
    setUpdate(null);
    setStatus("idle");
  }, []);

  const skipVersion = useCallback(() => {
    if (!update) return;
    localStorage.setItem(SKIPPED_KEY, update.version);
    skippedRef.current = update.version;
    setUpdate(null);
    setStatus("idle");
  }, [update]);

  // Keep checking while the app stays open for days.
  useEffect(() => {
    const timer = setInterval(() => void checkNow(false), CHECK_INTERVAL_MS);
    return () => clearInterval(timer);
  }, [checkNow]);

  return {
    status,
    update,
    progress,
    notice,
    version,
    portable,
    checkNow,
    install,
    dismiss,
    skipVersion,
  };
}
