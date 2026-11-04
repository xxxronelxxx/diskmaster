import { useMemo, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import type { CleanResult, ScanCategoryResult, ScanState } from "@/shared/types";
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
          <span className="text-sm font-medium text-slate-100">{category.name}</span>
          {category.risk === "caution" ? (
            <Badge variant="warn">
              <AlertIcon className="size-3" /> осторожно
            </Badge>
          ) : null}
          {category.needsAdmin && !admin ? (
            <Badge variant="neutral">
              <ShieldIcon className="size-3" /> нужен админ
            </Badge>
          ) : null}
          {category.status === "partial" ? <Badge variant="info">частично</Badge> : null}
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
            {scanning ? "счёт…" : "ожидание"}
          </span>
        ) : (
          <>
            <p className="text-sm font-semibold tabular-nums text-white">
              {formatBytes(category.bytes)}
            </p>
            <p className="text-[11px] text-slate-500">{formatCount(category.files)} файлов</p>
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
  onCleaned,
  admin,
}: {
  job: ScanState | null;
  starting: boolean;
  onStart: () => void;
  onCleaned: () => void;
  admin: boolean;
}) {
  // All transient UI state is tagged with the scan it belongs to, so a new
  // scan naturally invalidates selection/cleanup progress without effects.
  const [selection, setSelection] = useState<{ jobId: string; ids: Set<string> } | null>(null);
  const [cleanLog, setCleanLog] = useState<{ jobId: string; bytes: Record<string, number> } | null>(null);
  const [cleanResult, setCleanResult] = useState<{ jobId: string; data: CleanResult } | null>(null);
  const [confirming, setConfirming] = useState(false);

  const cleanMutation = useMutation(queryClient.system.cleanJunk.mutationOptions());

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
        title="Очистка диска"
        subtitle="Временные файлы, кэши, корзина и журналы — то, что можно удалить без потери данных"
        action={
          job ? (
            <Button variant="secondary" onClick={onStart} loading={starting || scanning}>
              <SparkleIcon className="size-4" />
              {scanning ? "Сканирование…" : "Проверить снова"}
            </Button>
          ) : (
            <Button variant="primary" onClick={onStart} loading={starting}>
              <SparkleIcon className="size-4" />
              Начать проверку
            </Button>
          )
        }
      />

      {!job ? (
        <EmptyState
          icon={<SparkleIcon className="size-6" />}
          title="Проверим, что можно удалить"
          description="Сканируем 12 категорий: временные файлы, кэш браузеров, кэш обновлений Windows, дампы, эскизы, журналы и корзину. Обычно занимает меньше минуты."
          action={
            <Button variant="primary" onClick={onStart} loading={starting}>
              Начать проверку
            </Button>
          }
        />
      ) : null}

      {scanning && job ? (
        <Card className="p-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div className="flex items-center gap-2 text-sm text-slate-100">
              <SpinnerIcon className="size-4 text-sky-300" />
              {job.phase === "large-files" ? "Ищем большие файлы…" : "Сканируем диск…"}
              <span className="font-semibold tabular-nums text-white">{job.percent}%</span>
            </div>
            <span className="text-xs text-slate-400">
              {formatCount(job.filesScanned)} файлов · {formatBytes(job.bytesScanned)}
            </span>
          </div>
          <Progress className="mt-3" value={job.percent} tone="sky" />
          <p className="mt-2 truncate font-mono text-[11px] text-slate-500" title={job.scanPath}>
            {job.scanPath || "Подготовка…"}
          </p>
        </Card>
      ) : null}

      {job?.phase === "error" ? (
        <Card className="border-rose-400/30 bg-rose-500/[0.07] p-4 text-sm text-rose-200">
          Сканирование прервано: {job.error}
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
                Освобождено {formatBytes(result.freedBytes)} · удалено {formatCount(result.deletedFiles)} объектов
              </p>
              {result.errors.length > 0 ? (
                <ul className="mt-1.5 space-y-0.5 text-[11px] text-amber-200/80">
                  {result.errors.slice(0, 5).map((error) => (
                    <li key={error}>· {error}</li>
                  ))}
                  {result.errors.length > 5 ? <li>· и ещё {result.errors.length - 5}…</li> : null}
                </ul>
              ) : (
                <p className="mt-1 text-xs text-emerald-200/70">Всё выбранное удалено без ошибок.</p>
              )}
            </div>
            <button
              className="ml-auto text-xs text-emerald-200/60 hover:text-emerald-100"
              onClick={() => setCleanResult(null)}
            >
              скрыть
            </button>
          </div>
        </Card>
      ) : null}

      {finished && job ? (
        <div className="flex flex-wrap items-end justify-between gap-3 rounded-2xl border border-white/10 bg-gradient-to-br from-sky-500/[0.10] via-white/[0.03] to-transparent p-4">
          <div>
            <p className="text-[11px] uppercase tracking-wide text-sky-300/70">Можно освободить</p>
            <p className="text-2xl font-semibold tracking-tight text-white">{formatBytes(totalBytes)}</p>
            <p className="text-xs text-slate-400">
              {formatCount(totalFiles)} файлов · проверено {formatCount(job.filesScanned)} файлов на диске
            </p>
          </div>
          <div className="flex gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setSelected(new Set(safeSelectable.map((category) => category.id)))}
              disabled={safeSelectable.length === 0}
            >
              Выбрать всё безопасное
            </Button>
            <Button
              variant="primary"
              onClick={() => setConfirming(true)}
              disabled={selectedBytes === 0 || cleanMutation.isPending}
            >
              <TrashIcon className="size-4" />
              Очистить {selectedBytes > 0 ? formatBytes(selectedBytes) : ""}
            </Button>
          </div>
        </div>
      ) : null}

      {job ? (
        <div className="space-y-2">
          <div className="flex items-center justify-between px-1">
            <p className="text-xs font-medium uppercase tracking-wide text-slate-500">
              Категории
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
                {allSelected ? "снять выделение" : "выбрать все"}
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
                    очищено · {formatBytes(cleaned[category.id])}
                  </span>
                ) : null}
              </div>
            );
          })}
        </div>
      ) : null}

      {job && job.largeFiles.length > 0 ? (
        <p className="px-1 text-[11px] text-slate-500">
          Дополнительно найдено {job.largeFiles.length} больших файлов — смотрите раздел
          «Большие файлы».
        </p>
      ) : null}

      <Modal
        open={confirming}
        onClose={() => setConfirming(false)}
        title="Удалить выбранный мусор?"
        description={
          <>
            Будет удалено <b className="text-white">{formatBytes(selectedBytes)}</b> в{" "}
            {selectedCategories.length} категориях. Файлы удаляются безвозвратно
            {selectedCategories.some((category) => category.id === "recycle-bin")
              ? ", корзина очищается полностью."
              : "."}
          </>
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirming(false)}>
              Отмена
            </Button>
            <Button
              variant="danger"
              onClick={handleClean}
              loading={cleanMutation.isPending}
            >
              <TrashIcon className="size-4" />
              Удалить
            </Button>
          </>
        }
      >
        <ul className="max-h-48 space-y-1 overflow-auto rounded-xl border border-white/10 bg-white/[0.03] p-3 text-xs text-slate-300">
          {selectedCategories.map((category) => (
            <li key={category.id} className="flex justify-between gap-3">
              <span className="truncate">{category.name}</span>
              <span className="tabular-nums text-slate-400">{formatBytes(remaining(category))}</span>
            </li>
          ))}
        </ul>
      </Modal>
    </div>
  );
}
