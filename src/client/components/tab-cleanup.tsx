import { useMemo, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import type { CleanPreview, CleanResult, ScanCategoryResult, ScanState } from "@/shared/types";
import { queryClient } from "@/client/rpc-client";
import { formatBytes, formatCount } from "@/client/lib/format";
import { Badge, Button, Card, EmptyState, Modal, Progress, SectionTitle } from "./ui";
import {
  AlertIcon,
  CheckIcon,
  ShieldIcon,
  SparkleIcon,
  SpinnerIcon,
  TrashIcon,
} from "./icons";
import { t, translateError } from "@/client/i18n";

function CategoryRow({
  category,
  checked,
  disabled,
  onToggle,
  admin,
}: {
  category: ScanCategoryResult;
  checked: boolean;
  disabled: boolean;
  onToggle: () => void;
  admin: boolean;
}) {
  const scanning = category.status === "scanning";
  const pending = category.status === "pending";
  const error = category.status === "error";

  return (
    <label
      className={`flex cursor-pointer items-start gap-3 rounded-xl border p-3 transition-colors ${
        checked
          ? "border-emerald-400/30 bg-emerald-500/[0.07]"
          : "border-white/[0.07] bg-white/[0.02] hover:bg-white/[0.04]"
      } ${disabled ? "cursor-not-allowed opacity-60" : ""}`}
    >
      <input
        type="checkbox"
        className="mt-1 size-4 shrink-0 accent-emerald-500"
        checked={checked}
        disabled={disabled || error}
        onChange={onToggle}
      />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="text-sm font-medium text-slate-100">{t(category.name)}</span>
          {category.risk === "caution" ? (
            <Badge variant="warn">
              <AlertIcon className="size-3" /> {t("осторожно")}
            </Badge>
          ) : null}
          {category.needsAdmin && !admin ? (
            <Badge variant="neutral">
              <ShieldIcon className="size-3" /> {t("нужен админ")}
            </Badge>
          ) : null}
          {category.status === "partial" ? <Badge variant="info">{t("частично")}</Badge> : null}
          {error ? <Badge variant="danger">{category.error}</Badge> : null}
        </div>
        <p className="mt-0.5 text-xs leading-relaxed text-slate-400">{category.description}</p>
        {category.paths[0] ? (
          <p className="mt-1 truncate font-mono text-[10px] text-slate-600" title={category.paths[0]}>
            {category.paths[0]}
          </p>
        ) : null}
      </div>
      <div className="w-24 shrink-0 text-right">
        {pending || scanning ? (
          <span className="inline-flex items-center gap-1 text-[11px] text-slate-500">
            <SpinnerIcon className="size-3" />
            {scanning ? t("счёт…") : t("ожидание")}
          </span>
        ) : (
          <>
            <p className="text-sm font-semibold tabular-nums text-white">
              {formatBytes(category.bytes)}
            </p>
            <p className="text-[11px] text-slate-500">{formatCount(category.files)} {t("файлов")}</p>
          </>
        )}
      </div>
    </label>
  );
}

const EMPTY_CATEGORIES: ScanCategoryResult[] = [];

export function CleanupTab({
  job,
  starting,
  onStart,
  onCancel,
  onCleaned,
  admin,
}: {
  job: ScanState | null;
  starting: boolean;
  onStart: () => void;
  onCancel: () => void;
  onCleaned: () => void;
  admin: boolean;
}) {
  // All transient UI state is tagged with the scan it belongs to, so a new
  // scan naturally invalidates selection/cleanup progress without effects.
  const [selection, setSelection] = useState<{ jobId: string; ids: Set<string> } | null>(null);
  const [cleanLog, setCleanLog] = useState<{ jobId: string; bytes: Record<string, number> } | null>(null);
  const [cleanResult, setCleanResult] = useState<{ jobId: string; data: CleanResult } | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [preview, setPreview] = useState<CleanPreview | null>(null);
  const [dryRun, setDryRun] = useState<{ jobId: string; data: CleanResult } | null>(null);

  const cleanMutation = useMutation(queryClient.system.cleanJunk.mutationOptions());
  const previewMutation = useMutation(queryClient.system.previewClean.mutationOptions());
  const dryRunMutation = useMutation(queryClient.system.cleanDryRun.mutationOptions());

  const jobId = job?.jobId ?? "";
  const scanning = Boolean(job && !job.finishedAt);
  const finished = Boolean(job?.finishedAt);

  const visible = job?.categories ?? EMPTY_CATEGORIES;
  const cleaned = cleanLog?.jobId === jobId ? cleanLog.bytes : {};
  const result = cleanResult?.jobId === jobId ? cleanResult.data : null;

  const selected = useMemo(() => {
    if (selection?.jobId === jobId) return selection.ids;
    return new Set(
      visible
        .filter(
          (category) =>
            category.status === "done" &&
            category.bytes > 0 &&
            category.risk === "safe" &&
            // Emptying the recycle bin is irreversible, so it is never
            // preselected — the user has to opt in explicitly.
            category.id !== "recycle-bin",
        )
        .map((category) => category.id),
    );
  }, [selection, jobId, visible]);

  function setSelected(ids: Set<string>) {
    setSelection({ jobId, ids });
  }

  const remaining = (category: ScanCategoryResult) =>
    cleaned[category.id] !== undefined ? 0 : category.bytes;

  const selectable = visible.filter((category) => remaining(category) > 0 && category.status !== "error");
  // Bulk actions never touch the recycle bin: emptying it is irreversible.
  const safeSelectable = selectable.filter((category) => category.id !== "recycle-bin");
  const selectedBytes = visible
    .filter((category) => selected.has(category.id))
    .reduce((sum, category) => sum + remaining(category), 0);
  const totalBytes = visible.reduce((sum, category) => sum + remaining(category), 0);
  const totalFiles = visible.reduce((sum, category) => sum + (cleaned[category.id] !== undefined ? 0 : category.files), 0);
  const allSelected =
    safeSelectable.length > 0 && safeSelectable.every((category) => selected.has(category.id));

  const selectedCategories = visible.filter((category) => selected.has(category.id));

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setSelected(next);
  }

  async function handleDryRun() {
    const ids = [...selected].filter((id) => {
      const category = visible.find((item) => item.id === id);
      return category ? remaining(category) > 0 : false;
    });
    if (ids.length === 0) return;
    const response = await dryRunMutation.mutateAsync({ categories: ids });
    setDryRun({ jobId, data: response });
  }

  async function handleClean() {
    const ids = [...selected].filter((id) => {
      const category = visible.find((item) => item.id === id);
      return category ? remaining(category) > 0 : false;
    });
    if (ids.length === 0) return;

    const response = await cleanMutation.mutateAsync({ categories: ids });
    setCleanResult({ jobId, data: response });
    setCleanLog((current) => {
      const next = current?.jobId === jobId ? { ...current.bytes } : {};
      for (const category of response.categories) next[category.id] = category.bytes;
      return { jobId, bytes: next };
    });
    setSelected(new Set());
    setConfirming(false);
    onCleaned();
  }

  return (
    <div className="space-y-5">
      <SectionTitle
        title={t("Очистка диска")}
        subtitle={t("Временные файлы, кэши, корзина и журналы — то, что можно удалить без потери данных")}
        action={
          job ? (
            <Button variant="secondary" onClick={onStart} loading={starting || scanning}>
              <SparkleIcon className="size-4" />
              {scanning ? t("Сканирование…") : t("Проверить снова")}
            </Button>
          ) : (
            <Button variant="primary" onClick={onStart} loading={starting}>
              <SparkleIcon className="size-4" />
              {t("Начать проверку")}
            </Button>
          )
        }
      />

      {!job ? (
        <EmptyState
          icon={<SparkleIcon className="size-6" />}
          title={t("Проверим, что можно удалить")}
          description={t("Сканируем 12 категорий: временные файлы, кэш браузеров, кэш обновлений Windows, дампы, эскизы, журналы и корзину. Обычно занимает меньше минуты.")}
          action={
            <Button variant="primary" onClick={onStart} loading={starting}>
              {t("Начать проверку")}
            </Button>
          }
        />
      ) : null}

      {scanning && job ? (
        <Card className="p-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-2 text-sm text-slate-100">
              <SpinnerIcon className="size-4 text-sky-300" />
              {job.phase === "large-files" ? t("Ищем большие файлы…") : t("Сканируем диск…")}
              <span className="font-semibold tabular-nums text-white">
                {Math.min(100, Math.max(0, job.percent))}%
              </span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-xs text-slate-400">
                {formatCount(job.filesScanned)} {t("файлов ·")} {formatBytes(job.bytesScanned)}
              </span>
              <Button size="sm" variant="danger" onClick={onCancel}>
                {t("Остановить")}
              </Button>
            </div>
          </div>
          <Progress className="mt-3" value={job.percent} tone="sky" />
          <p className="mt-2 truncate font-mono text-[11px] text-slate-500" title={job.scanPath}>
            {job.scanPath || t("Подготовка…")}
          </p>
        </Card>
      ) : null}

      {job?.phase === "error" ? (
        <Card className="border-rose-400/30 bg-rose-500/[0.07] p-4 text-sm text-rose-200">
          {t("Сканирование прервано:")} {job.error}
        </Card>
      ) : null}

      {job?.canceled ? (
        <Card className="border-amber-400/25 bg-amber-400/[0.06] p-4 text-sm text-amber-100">
          {t("Сканирование отменено.")}
        </Card>
      ) : null}

      {result ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-4">
          <div className="flex items-start gap-3">
            <div className="mt-0.5 grid size-8 shrink-0 place-items-center rounded-full bg-emerald-500/20 text-emerald-300">
              <CheckIcon className="size-4" />
            </div>
            <div className="min-w-0">
              <p className="text-sm font-medium text-emerald-100">
                {t("Освобождено")} {formatBytes(result.freedBytes)} {t("· удалено")} {formatCount(result.deletedFiles)} {t("объектов")}
              </p>
              {result.errors.length > 0 ? (
                <ul className="mt-1.5 space-y-0.5 text-[11px] text-amber-200/80">
                  {result.errors.slice(0, 5).map((error) => (
                    <li key={error}>· {translateError(error)}</li>
                  ))}
                  {result.errors.length > 5 ? <li>{t("· и ещё")} {result.errors.length - 5}…</li> : null}
                </ul>
              ) : (
                <p className="mt-1 text-xs text-emerald-200/70">{t("Всё выбранное удалено без ошибок.")}</p>
              )}
            </div>
            <button
              className="ml-auto text-xs text-emerald-200/60 hover:text-emerald-100"
              onClick={() => setCleanResult(null)}
            >
              {t("скрыть")}
            </button>
          </div>
        </Card>
      ) : null}

      {finished && job ? (
        <div className="flex flex-wrap items-end justify-between gap-3 rounded-2xl border border-white/10 bg-gradient-to-br from-sky-500/[0.10] via-white/[0.03] to-transparent p-4">
          <div>
            <p className="text-[11px] uppercase tracking-wide text-sky-300/70">{t("Можно освободить")}</p>
            <p className="text-2xl font-semibold tracking-tight text-white">{formatBytes(totalBytes)}</p>
            <p className="text-xs text-slate-400">
              {formatCount(totalFiles)} {t("файлов · проверено")} {formatCount(job.filesScanned)} {t("файлов на диске")}
            </p>
          </div>
          <div className="flex gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setSelected(new Set(safeSelectable.map((category) => category.id)))}
              disabled={safeSelectable.length === 0}
            >
              {t("Выбрать всё безопасное")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => void handleDryRun()}
              loading={dryRunMutation.isPending}
              disabled={selectedBytes === 0 || dryRunMutation.isPending}
            >
              {t("Пробный запуск")}
            </Button>
            <Button
              variant="primary"
              onClick={() => setConfirming(true)}
              disabled={selectedBytes === 0 || cleanMutation.isPending}
            >
              <TrashIcon className="size-4" />
              {t("Очистить")} {selectedBytes > 0 ? formatBytes(selectedBytes) : ""}
            </Button>
          </div>
        </div>
      ) : null}

      {dryRun?.jobId === jobId ? (
        <Card className="border-sky-400/25 bg-sky-500/[0.06] p-3.5">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <p className="text-xs text-sky-100">
              {t("Пробный запуск: будет удалено")} <b>{formatBytes(dryRun.data.freedBytes)}</b> ·{" "}
              {formatCount(dryRun.data.deletedFiles)} {t("файлов. Ничего не удалено.")}
            </p>
            <button
              className="text-[11px] text-slate-400 hover:text-white"
              onClick={() => setDryRun(null)}
            >
              {t("скрыть")}
            </button>
          </div>
          <div className="mt-2 grid gap-1 sm:grid-cols-2">
            {dryRun.data.categories.map((category) => (
              <div key={category.id} className="flex justify-between gap-2 text-[11px]">
                <span className="truncate text-slate-400">{t(category.name)}</span>
                <span className="shrink-0 tabular-nums text-slate-300">{formatBytes(category.bytes)}</span>
              </div>
            ))}
          </div>
        </Card>
      ) : null}

      {job ? (
        <div className="space-y-2">
          <div className="flex items-center justify-between px-1">
            <p className="text-xs font-medium uppercase tracking-wide text-slate-500">
              {t("Категории")}
            </p>
            {finished && safeSelectable.length > 0 ? (
              <button
                className="text-[11px] text-slate-400 hover:text-white"
                onClick={() =>
                  setSelected(
                    allSelected ? new Set() : new Set(safeSelectable.map((category) => category.id)),
                  )
                }
              >
                {allSelected ? t("снять выделение") : t("выбрать все")}
              </button>
            ) : null}
          </div>

          {visible.map((category) => {
            const isCleaned = cleaned[category.id] !== undefined;
            return (
              <div key={category.id} className="relative">
                <CategoryRow
                  category={
                    isCleaned
                      ? { ...category, bytes: 0, files: 0, status: "done" }
                      : category
                  }
                  checked={selected.has(category.id)}
                  disabled={!finished || isCleaned}
                  onToggle={() => toggle(category.id)}
                  admin={admin}
                />
                {isCleaned ? (
                  <span className="absolute right-28 top-3 text-[10px] text-emerald-300/80">
                    {t("очищено ·")} {formatBytes(cleaned[category.id])}
                  </span>
                ) : null}
              </div>
            );
          })}
        </div>
      ) : null}

      {job && job.largeFiles.length > 0 ? (
        <p className="px-1 text-[11px] text-slate-500">
          {t("Дополнительно найдено")} {job.largeFiles.length} {t("больших файлов — смотрите раздел «Большие файлы».")}
        </p>
      ) : null}

      <Modal
        open={confirming}
        onClose={() => setConfirming(false)}
        title={t("Удалить выбранный мусор?")}
        description={
          <>
            {t("Будет удалено")} <b className="text-white">{formatBytes(selectedBytes)}</b> {t("в")}{" "}
            {selectedCategories.length} {t("категориях. Файлы удаляются безвозвратно")}
            {selectedCategories.some((category) => category.id === "recycle-bin")
              ? t(", корзина очищается полностью.")
              : "."}
          </>
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirming(false)}>
              {t("Отмена")}
            </Button>
            <Button
              variant="danger"
              onClick={handleClean}
              loading={cleanMutation.isPending}
            >
              <TrashIcon className="size-4" />
              {t("Удалить")}
            </Button>
          </>
        }
      >
        <ul className="max-h-48 space-y-1 overflow-auto rounded-xl border border-white/10 bg-white/[0.03] p-3 text-xs text-slate-300">
          {selectedCategories.map((category) => (
            <li key={category.id} className="flex justify-between gap-3">
              <span className="truncate">{t(category.name)}</span>
              <span className="tabular-nums text-slate-400">{formatBytes(remaining(category))}</span>
            </li>
          ))}
        </ul>

        <div className="mt-3">
          <Button
            size="sm"
            variant="secondary"
            loading={previewMutation.isPending}
            onClick={async () => {
              const ids = selectedCategories.map((category) => category.id);
              const result = await previewMutation.mutateAsync({ categories: ids });
              setPreview(result);
            }}
          >
            {t("Показать, что будет удалено")}
          </Button>

          {preview ? (
            <div className="mt-3 max-h-64 space-y-3 overflow-auto rounded-xl border border-white/10 bg-white/[0.03] p-3">
              {preview.categories.map((category) => (
                <div key={category.id}>
                  <p className="text-[11px] font-medium text-slate-200">
                    {t(category.name)} · {formatBytes(category.totalBytes)} · {formatCount(category.totalFiles)} {t("файлов")}
                  </p>
                  <ul className="mt-1 space-y-0.5">
                    {category.entries.slice(0, 20).map((entry) => (
                      <li key={entry.path} className="truncate font-mono text-[10px] text-slate-500" title={entry.path}>
                        {formatBytes(entry.size)} · {entry.path}
                      </li>
                    ))}
                  </ul>
                  {category.truncated ? (
                    <p className="mt-0.5 text-[10px] text-slate-600">{t("…показаны крупнейшие элементы")}</p>
                  ) : null}
                </div>
              ))}
            </div>
          ) : null}
        </div>
      </Modal>
    </div>
  );
}
