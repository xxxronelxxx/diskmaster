import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "@/client/api";
import type { ScanState } from "@/shared/types";
import { queryClient, rpcClient } from "@/client/rpc-client";
import { formatBytes, formatDate } from "@/client/lib/format";
import { Badge, Button, Card, EmptyState, Modal, Progress, SectionTitle } from "./ui";
import { CheckIcon, FilesIcon, FolderIcon, SpinnerIcon, TrashIcon } from "./icons";
import { t, translateError } from "@/client/i18n";

const THRESHOLDS = [100, 250, 500, 1024];

export function LargeFilesTab({
  job,
  starting,
  onStart,
  onCancel,
  onDeleted,
}: {
  job: ScanState | null;
  starting: boolean;
  onStart: (thresholdMb: number) => void;
  onCancel: () => void;
  onDeleted: () => void;
}) {
  const [pickedThreshold, setPickedThreshold] = useState<number | null>(null);
  const [selection, setSelection] = useState<{ jobId: string; paths: Set<string> } | null>(null);
  const [removal, setRemoval] = useState<{ jobId: string; paths: Set<string> } | null>(null);
  const [confirming, setConfirming] = useState(false);
  const [freed, setFreed] = useState<{
    jobId: string;
    bytes: number;
    count: number;
    errors: string[];
  } | null>(null);

  const deleteMutation = useMutation(queryClient.system.deleteLargeFiles.mutationOptions());

  const [query, setQuery] = useState("");
  const [renderLimit, setRenderLimit] = useState(200);
  const [extension, setExtension] = useState<string | null>(null);
  const [age, setAge] = useState<"all" | "6" | "12">("all");
  const [sort, setSort] = useState<"size" | "date" | "name">("size");
  const [sortAsc, setSortAsc] = useState(false);
  const [exporting, setExporting] = useState(false);

  const scanning = Boolean(job && !job.finishedAt);
  const finished = Boolean(job?.finishedAt);
  const jobId = job?.jobId ?? "";

  // Threshold and per-scan progress are keyed by job, so a new scan resets
  // them without synchronising state in an effect.
  const threshold =
    pickedThreshold ??
    (job?.largeFileThreshold ? Math.round(job.largeFileThreshold / (1024 * 1024)) : 250);
  const removed = removal?.jobId === jobId ? removal.paths : new Set<string>();
  const selected = selection?.jobId === jobId ? selection.paths : new Set<string>();
  const freedNotice = freed?.jobId === jobId ? freed : null;

  function setSelected(next: Set<string>) {
    setSelection({ jobId, paths: next });
  }

  const files = (job?.largeFiles ?? []).filter((file) => !removed.has(file.path));
  const extensions = [...new Set(files.map((file) => {
    const dot = file.name.lastIndexOf(".");
    return dot > 0 ? file.name.slice(dot + 1).toLowerCase() : "";
  }).filter(Boolean))].sort();

  const visibleFiles = files
    .filter((file) => {
      if (extension && !file.name.toLowerCase().endsWith(`.${extension}`)) return false;
      if (age !== "all") {
        const months = age === "6" ? 6 : 12;
        if (Date.now() - file.modified < months * 30 * 24 * 60 * 60 * 1000) return false;
      }
      if (query && !file.path.toLowerCase().includes(query.toLowerCase())) return false;
      return true;
    })
    .sort((left, right) => {
      const direction = sortAsc ? 1 : -1;
      if (sort === "size") return (left.size - right.size) * direction;
      if (sort === "date") return (left.modified - right.modified) * direction;
      return left.name.localeCompare(right.name, "ru") * direction;
    });

  const selectedFiles = visibleFiles.filter((file) => selected.has(file.path));
  const renderedFiles = visibleFiles.slice(0, renderLimit);
  const selectedBytes = selectedFiles.reduce((sum, file) => sum + file.size, 0);
  const totalBytes = visibleFiles.reduce((sum, file) => sum + file.size, 0);
  const allSelected = visibleFiles.length > 0 && visibleFiles.every((file) => selected.has(file.path));

  async function exportCsv() {
    const path = await save({
      defaultPath: "large-files.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    setExporting(true);
    try {
      const escape = (value: string) => `"${value.replace(/"/g, '""')}"`;
      const rows = [
        ["path", "name", "size", "modified"].join(","),
        ...visibleFiles.map((file) =>
          [escape(file.path), escape(file.name), String(file.size), String(file.modified)].join(","),
        ),
      ];
      await api.writeTextFile({
        path,
        content: String.fromCharCode(0xfeff) + rows.join(String.fromCharCode(13, 10)),
      });
    } finally {
      setExporting(false);
    }
  }

  function toggle(path: string) {
    const next = new Set(selected);
    if (next.has(path)) next.delete(path);
    else next.add(path);
    setSelected(next);
  }

  async function handleDelete() {
    const paths = [...selected];
    if (paths.length === 0) return;
    const response = await deleteMutation.mutateAsync({ paths });
    setFreed({ jobId, bytes: response.bytes, count: response.removed, errors: response.errors });
    setRemoval((current) => {
      const next = current?.jobId === jobId ? new Set(current.paths) : new Set<string>();
      for (const path of paths) next.add(path);
      return { jobId, paths: next };
    });
    setSelected(new Set());
    setConfirming(false);
    onDeleted();
  }

  return (
    <div className="space-y-5">
      <SectionTitle
        title={t("Большие файлы")}
        subtitle={t("Крупные файлы в профиле пользователя и на других дисках — их удаление освобождает больше всего места")}
        action={
          <Button variant="secondary" onClick={() => onStart(threshold)} loading={starting || scanning}>
            {scanning ? <SpinnerIcon className="size-4" /> : <FilesIcon className="size-4" />}
            {scanning ? t("Сканирование…") : t("Сканировать")}
          </Button>
        }
      />

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-slate-500">{t("Минимальный размер:")}</span>
        {THRESHOLDS.map((value) => (
          <button
            key={value}
            onClick={() => {
              setPickedThreshold(value);
              onStart(value);
            }}
            disabled={scanning}
            className={`rounded-lg border px-2.5 py-1 text-xs transition-colors disabled:opacity-40 ${
              threshold === value
                ? "border-emerald-400/40 bg-emerald-500/15 text-emerald-200"
                : "border-white/10 bg-white/[0.04] text-slate-300 hover:bg-white/10"
            }`}
          >
            {value >= 1024 ? t("1 ГБ") : `${value} МБ`}
          </button>
        ))}
      </div>

      {finished && files.length > 0 ? (
        <div className="flex flex-wrap items-center gap-2">
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("Поиск по пути")}
            className="min-w-48 flex-1 rounded-lg border border-white/10 bg-white/[0.04] px-3 py-1.5 text-xs text-slate-200 outline-none placeholder:text-slate-600 focus:border-emerald-400/40"
          />
          <button
            onClick={() => setExtension(null)}
            className={`rounded-lg border px-2.5 py-1 text-xs ${extension === null ? "border-emerald-400/40 bg-emerald-500/15 text-emerald-200" : "border-white/10 bg-white/[0.04] text-slate-300"}`}
          >
            {t("Все")}
          </button>
          {extensions.slice(0, 8).map((value) => (
            <button
              key={value}
              onClick={() => setExtension(extension === value ? null : value)}
              className={`rounded-lg border px-2.5 py-1 text-xs ${extension === value ? "border-emerald-400/40 bg-emerald-500/15 text-emerald-200" : "border-white/10 bg-white/[0.04] text-slate-300"}`}
            >
              .{value}
            </button>
          ))}
          <select
            value={age}
            onChange={(event) => setAge(event.target.value as "all" | "6" | "12")}
            className="rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-300 outline-none"
          >
            <option value="all">{t("Любая дата")}</option>
            <option value="6">{t("старше 6 мес.")}</option>
            <option value="12">{t("старше 12 мес.")}</option>
          </select>
          <select
            value={sort}
            onChange={(event) => setSort(event.target.value as "size" | "date" | "name")}
            className="rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-300 outline-none"
          >
            <option value="size">{t("Размер")}</option>
            <option value="date">{t("Дата")}</option>
            <option value="name">{t("Имя")}</option>
          </select>
          <button
            onClick={() => setSortAsc((current) => !current)}
            className="rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-300"
            title={t("Направление сортировки")}
          >
            {sortAsc ? "↑" : "↓"}
          </button>
          <Button size="sm" variant="secondary" onClick={() => void exportCsv()} loading={exporting}>
            CSV
          </Button>
        </div>
      ) : null}

      {scanning && job ? (
        <Card className="p-4">
          <div className="flex items-center justify-between gap-2 text-sm">
            <span className="flex items-center gap-2 text-slate-100">
              <SpinnerIcon className="size-4 text-sky-300" />
              {t("Поиск файлов больше")} {threshold} {t("МБ…")}
            </span>
            <div className="flex items-center gap-2">
              <span className="tabular-nums text-slate-400">
                {Math.min(100, Math.max(0, job.percent))}%
              </span>
              <Button size="sm" variant="danger" onClick={onCancel}>
                {t("Остановить")}
              </Button>
            </div>
          </div>
          <Progress className="mt-3" value={job.percent} tone="violet" />
          <p className="mt-2 truncate font-mono text-[11px] text-slate-500">{job.scanPath}</p>
        </Card>
      ) : null}

      {job?.canceled ? (
        <Card className="border-amber-400/25 bg-amber-400/[0.06] p-4 text-sm text-amber-100">
          {t("Сканирование отменено.")}
        </Card>
      ) : null}

      {!job ? (
        <EmptyState
          icon={<FilesIcon className="size-6" />}
          title={t("Ещё не сканировали")}
          description={t("Запустим поиск файлов больше {size} МБ в вашем профиле и на остальных локальных дисках. Сканирование выводит и категории мусора — оба раздела используют один проход.", { size: threshold })}
          action={
            <Button variant="primary" onClick={() => onStart(threshold)} loading={starting}>
              {t("Сканировать диск")}
            </Button>
          }
        />
      ) : null}

      {freedNotice ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-4">
          <p className="flex items-center gap-2 text-sm font-medium text-emerald-100">
            <CheckIcon className="size-4" />
            {t("Перемещено в корзину:")} {freedNotice.count} {t("файлов ·")} {formatBytes(freedNotice.bytes)}
          </p>
          {freedNotice.errors.length > 0 ? (
            <ul className="mt-1.5 space-y-0.5 text-[11px] text-amber-200/80">
              {freedNotice.errors.map((error) => (
                <li key={error}>· {translateError(error)}</li>
              ))}
            </ul>
          ) : null}
        </Card>
      ) : null}

      {finished && visibleFiles.length === 0 ? (
        <EmptyState
          icon={<CheckIcon className="size-6" />}
          title={t("Больших файлов не найдено")}
          description={t("Файлов больше {size} МБ на дисках нет. Можно снизить порог или проверить категории мусора.", { size: threshold })}
        />
      ) : null}

      {visibleFiles.length > 0 ? (
        <Card className="overflow-hidden">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-white/[0.08] px-4 py-3">
            <div className="flex items-center gap-3">
              <input
                type="checkbox"
                className="size-4 accent-emerald-500"
                checked={allSelected}
                onChange={() =>
                  setSelected(allSelected ? new Set() : new Set(visibleFiles.map((file) => file.path)))
                }
              />
              <div>
                <p className="text-sm text-slate-100">
                  {visibleFiles.length} {t("файлов ·")} {formatBytes(totalBytes)}
                </p>
                {job?.largeFilesTruncated ? (
                  <p className="text-[11px] text-slate-500">{t("показаны самые крупные")}</p>
                ) : null}
              </div>
            </div>
            <div className="flex items-center gap-3">
              <Badge variant="neutral">{t("в корзину")}</Badge>
              <Button
                variant="danger"
                size="sm"
                disabled={selectedBytes === 0 || deleteMutation.isPending}
                onClick={() => setConfirming(true)}
              >
                <TrashIcon className="size-3.5" />
                {t("Удалить")} {selectedBytes > 0 ? formatBytes(selectedBytes) : ""}
              </Button>
            </div>
          </div>

          <div className="divide-y divide-white/[0.06]">
            {renderedFiles.map((file) => {
              const checked = selected.has(file.path);
              return (
                <label
                  key={file.path}
                  className={`flex cursor-pointer items-center gap-3 px-4 py-2.5 transition-colors [contain-intrinsic-size:56px] [content-visibility:auto] ${
                    checked ? "bg-emerald-500/[0.06]" : "hover:bg-white/[0.03]"
                  }`}
                >
                  <input
                    type="checkbox"
                    className="size-4 shrink-0 accent-emerald-500"
                    checked={checked}
                    onChange={() => toggle(file.path)}
                  />
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-sm text-slate-100" title={file.name}>
                      {file.name}
                    </p>
                    <p className="truncate font-mono text-[11px] text-slate-500" title={file.dir}>
                      {file.dir}
                    </p>
                  </div>
                  <div className="shrink-0 text-right">
                    <p className="text-sm font-semibold tabular-nums text-white">
                      {formatBytes(file.size)}
                    </p>
                    <p className="text-[11px] text-slate-500">{formatDate(file.modified)}</p>
                  </div>
                  <button
                    title={t("Показать в проводнике")}
                    onClick={(event) => {
                      event.preventDefault();
                      void rpcClient.system.reveal({ path: file.path });
                    }}
                    className="grid size-7 shrink-0 place-items-center rounded-lg text-slate-500 transition-colors hover:bg-white/10 hover:text-slate-200"
                  >
                    <FolderIcon className="size-4" />
                  </button>
                </label>
              );
            })}
            {visibleFiles.length > renderedFiles.length ? (
              <div className="flex justify-center border-t border-white/[0.06] p-2">
                <Button
                  size="sm"
                  variant="secondary"
                  onClick={() => setRenderLimit((current) => current + 200)}
                >
                  {t("Показать ещё 200")}
                </Button>
              </div>
            ) : null}
          </div>
        </Card>
      ) : null}

      <Modal
        open={confirming}
        onClose={() => setConfirming(false)}
        title={t("Переместить файлы в корзину?")}
        description={
          <>
            {selectedFiles.length} {t("файлов на")} <b className="text-white">{formatBytes(selectedBytes)}</b>{t(". Их можно будет восстановить из корзины, пока она не очищена.")}
          </>
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirming(false)}>
              {t("Отмена")}
            </Button>
            <Button variant="danger" loading={deleteMutation.isPending} onClick={handleDelete}>
              <TrashIcon className="size-4" />
              {t("В корзину")}
            </Button>
          </>
        }
      />
    </div>
  );
}
