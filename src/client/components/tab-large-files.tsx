import { useState } from "react";
import { useMutation } from "@tanstack/react-query";
import type { ScanState } from "@/shared/types";
import { queryClient, rpcClient } from "@/client/rpc-client";
import { formatBytes, formatDate } from "@/client/lib/format";
import { Badge, Button, Card, EmptyState, Modal, Progress, SectionTitle } from "./ui";
import { CheckIcon, FilesIcon, FolderIcon, SpinnerIcon, TrashIcon } from "./icons";

const THRESHOLDS = [100, 250, 500, 1024];

export function LargeFilesTab({
  job,
  starting,
  onStart,
  onDeleted,
}: {
  job: ScanState | null;
  starting: boolean;
  onStart: (thresholdMb: number) => void;
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

  const selectedFiles = files.filter((file) => selected.has(file.path));
  const selectedBytes = selectedFiles.reduce((sum, file) => sum + file.size, 0);
  const totalBytes = files.reduce((sum, file) => sum + file.size, 0);
  const allSelected = files.length > 0 && selected.size === files.length;

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
        title="Большие файлы"
        subtitle="Крупные файлы в профиле пользователя и на других дисках — их удаление освобождает больше всего места"
        action={
          <Button variant="secondary" onClick={() => onStart(threshold)} loading={starting || scanning}>
            {scanning ? <SpinnerIcon className="size-4" /> : <FilesIcon className="size-4" />}
            {scanning ? "Сканирование…" : "Сканировать"}
          </Button>
        }
      />

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-slate-500">Минимальный размер:</span>
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
            {value >= 1024 ? "1 ГБ" : `${value} МБ`}
          </button>
        ))}
      </div>

      {scanning && job ? (
        <Card className="p-4">
          <div className="flex items-center justify-between text-sm">
            <span className="flex items-center gap-2 text-slate-100">
              <SpinnerIcon className="size-4 text-sky-300" />
              Поиск файлов больше {threshold} МБ…
            </span>
            <span className="tabular-nums text-slate-400">{job.percent}%</span>
          </div>
          <Progress className="mt-3" value={job.percent} tone="violet" />
          <p className="mt-2 truncate font-mono text-[11px] text-slate-500">{job.scanPath}</p>
        </Card>
      ) : null}

      {!job ? (
        <EmptyState
          icon={<FilesIcon className="size-6" />}
          title="Ещё не сканировали"
          description={`Запустим поиск файлов больше ${threshold} МБ в вашем профиле и на остальных локальных дисках. Сканирование выводит и категории мусора — оба раздела используют один проход.`}
          action={
            <Button variant="primary" onClick={() => onStart(threshold)} loading={starting}>
              Сканировать диск
            </Button>
          }
        />
      ) : null}

      {freedNotice ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-4">
          <p className="flex items-center gap-2 text-sm font-medium text-emerald-100">
            <CheckIcon className="size-4" />
            Перемещено в корзину: {freedNotice.count} файлов · {formatBytes(freedNotice.bytes)}
          </p>
          {freedNotice.errors.length > 0 ? (
            <ul className="mt-1.5 space-y-0.5 text-[11px] text-amber-200/80">
              {freedNotice.errors.map((error) => (
                <li key={error}>· {error}</li>
              ))}
            </ul>
          ) : null}
        </Card>
      ) : null}

      {finished && files.length === 0 ? (
        <EmptyState
          icon={<CheckIcon className="size-6" />}
          title="Больших файлов не найдено"
          description={`Файлов больше ${threshold} МБ на дисках нет. Можно снизить порог или проверить категории мусора.`}
        />
      ) : null}

      {files.length > 0 ? (
        <Card className="overflow-hidden">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-white/[0.08] px-4 py-3">
            <div className="flex items-center gap-3">
              <input
                type="checkbox"
                className="size-4 accent-emerald-500"
                checked={allSelected}
                onChange={() =>
                  setSelected(allSelected ? new Set() : new Set(files.map((file) => file.path)))
                }
              />
              <div>
                <p className="text-sm text-slate-100">
                  {files.length} файлов · {formatBytes(totalBytes)}
                </p>
                {job?.largeFilesTruncated ? (
                  <p className="text-[11px] text-slate-500">показаны самые крупные</p>
                ) : null}
              </div>
            </div>
            <div className="flex items-center gap-3">
              <Badge variant="neutral">в корзину</Badge>
              <Button
                variant="danger"
                size="sm"
                disabled={selectedBytes === 0 || deleteMutation.isPending}
                onClick={() => setConfirming(true)}
              >
                <TrashIcon className="size-3.5" />
                Удалить {selectedBytes > 0 ? formatBytes(selectedBytes) : ""}
              </Button>
            </div>
          </div>

          <div className="divide-y divide-white/[0.06]">
            {files.map((file) => {
              const checked = selected.has(file.path);
              return (
                <label
                  key={file.path}
                  className={`flex cursor-pointer items-center gap-3 px-4 py-2.5 transition-colors ${
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
                    title="Показать в проводнике"
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
          </div>
        </Card>
      ) : null}

      <Modal
        open={confirming}
        onClose={() => setConfirming(false)}
        title="Переместить файлы в корзину?"
        description={
          <>
            {selectedFiles.length} файлов на <b className="text-white">{formatBytes(selectedBytes)}</b>.
            Их можно будет восстановить из корзины, пока она не очищена.
          </>
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirming(false)}>
              Отмена
            </Button>
            <Button variant="danger" loading={deleteMutation.isPending} onClick={handleDelete}>
              <TrashIcon className="size-4" />
              В корзину
            </Button>
          </>
        }
      />
    </div>
  );
}
