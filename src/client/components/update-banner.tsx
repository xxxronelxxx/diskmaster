import type { Updater } from "@/client/lib/updater";
import { Button } from "./ui";
import { SparkleIcon } from "./icons";

/**
 * Shows the update that `useUpdater` discovered. The payload signature is
 * verified by the Tauri updater plugin before the installer runs.
 */
export function UpdateBanner({ updater }: { updater: Updater }) {
  const { update, status, progress, install, dismiss, skipVersion } = updater;
  if (!update) return null;

  return (
    <div className="mb-5 flex flex-wrap items-center gap-3 rounded-2xl border border-emerald-400/25 bg-emerald-400/[0.07] px-4 py-3">
      <span className="grid size-8 shrink-0 place-items-center rounded-xl bg-emerald-400/15 text-emerald-300">
        <SparkleIcon className="size-4" />
      </span>

      <div className="min-w-0 flex-1">
        <p className="text-[13px] font-medium text-emerald-100">
          Доступна версия {update.version}
        </p>
        <p className="mt-0.5 text-[11px] text-emerald-200/70">
          {status === "downloading"
            ? `Скачивание обновления… ${progress}%`
            : update.body?.trim() || "Обновление скачается и установится автоматически."}
        </p>
        {status === "downloading" ? (
          <div className="mt-2 h-1 w-full overflow-hidden rounded-full bg-emerald-950/40">
            <div
              className="h-full rounded-full bg-emerald-400 transition-[width]"
              style={{ width: `${progress}%` }}
            />
          </div>
        ) : null}
      </div>

      <div className="flex shrink-0 items-center gap-2">
        <Button
          variant="ghost"
          size="sm"
          onClick={skipVersion}
          disabled={status === "downloading"}
          title="Не предлагать эту версию до следующего релиза"
        >
          Пропустить версию
        </Button>
        <Button variant="ghost" size="sm" onClick={dismiss} disabled={status === "downloading"}>
          Позже
        </Button>
        <Button variant="primary" size="sm" onClick={() => void install()} loading={status === "downloading"}>
          Обновить
        </Button>
      </div>
    </div>
  );
}
