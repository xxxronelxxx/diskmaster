import { useMemo, useState } from "react";
import { useMutation } from "@tanstack/react-query";
import type { DuplicateGroup, DuplicateScan } from "@/shared/types";
import { queryClient, rpcClient } from "@/client/rpc-client";
import { formatBytes, formatCount, formatDate } from "@/client/lib/format";
import { Badge, Button, Card, EmptyState, Modal, SectionTitle } from "./ui";
import { AlertIcon, CheckIcon, FilesIcon, RefreshIcon, TrashIcon } from "./icons";

const MIN_SIZES = [16, 64, 256, 1024];

export function DuplicatesTab() {
  const [minSize, setMinSize] = useState(64);
  const [scan, setScan] = useState<DuplicateScan | null>(null);
  const [keepFirst, setKeepFirst] = useState<Set<string>>(new Set());
  const [removed, setRemoved] = useState<Set<string>>(new Set());
  const [pending, setPending] = useState<{ paths: string[]; bytes: number } | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const scanMutation = useMutation(queryClient.system.duplicatesScan.mutationOptions());
  const deleteMutation = useMutation(queryClient.system.folderDelete.mutationOptions());

  const groups = useMemo(() => {
    if (!scan) return [];
    return scan.groups
      .map((group) => ({
        ...group,
        files: group.files.filter((file) => !removed.has(file.path)),
      }))
      .map((group) => ({ ...group, count: group.files.length, wasted: group.size * Math.max(0, group.files.length - 1) }))
      .filter((group) => group.files.length > 1);
  }, [scan, removed]);

  const totalWasted = groups.reduce((sum, group) => sum + group.wasted, 0);

  async function runScan() {
    setNote(null);
    setRemoved(new Set());
    setKeepFirst(new Set());
    const result = (await scanMutation.mutateAsync({ minSizeKb: minSize })) as DuplicateScan;
    setScan(result);
  }

  function filesToDelete(group: DuplicateGroup): string[] {
    if (keepFirst.has(group.hash)) {
      return group.files.slice(1).map((file) => file.path);
    }
    return group.files.map((file) => file.path);
  }

  function markDuplicatesOf(group: DuplicateGroup) {
    const paths = filesToDelete(group);
    setPending({ paths, bytes: group.size * paths.length });
  }

  async function markAllDuplicates(keepOne: boolean) {
    const paths: string[] = [];
    let bytes = 0;
    for (const group of groups) {
      const targets = keepOne ? group.files.slice(1) : group.files;
      for (const file of targets) {
        paths.push(file.path);
        bytes += file.size;
      }
    }
    if (paths.length === 0) return;
    setPending({ paths, bytes });
  }

  async function confirmDelete() {
    if (!pending) return;
    const response = await deleteMutation.mutateAsync({ paths: pending.paths, permanent: false });
    setRemoved((current) => {
      const next = new Set(current);
      for (const path of pending.paths) next.add(path);
      return next;
    });
    setNote(
      response.ok
        ? `В корзину отправлено ${formatCount(response.files)} файлов · ${formatBytes(response.bytes)}`
        : `Ошибки: ${response.errors.slice(0, 2).join("; ")}`,
    );
    setPending(null);
  }

  return (
    <div className="space-y-5">
      <SectionTitle
        title="Дубликаты"
        subtitle="Поиск одинаковых файлов по содержимому (SHA-256), а не по имени"
        action={
          <Button variant="primary" onClick={runScan} loading={scanMutation.isPending}>
            <RefreshIcon className="size-4" />
            {scan ? "Сканировать снова" : "Найти дубликаты"}
          </Button>
        }
      />

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-slate-500">Минимальный размер файла:</span>
        {MIN_SIZES.map((value) => (
          <button
            key={value}
            onClick={() => setMinSize(value)}
            disabled={scanMutation.isPending}
            className={`rounded-lg border px-2.5 py-1 text-xs transition-colors disabled:opacity-40 ${
              minSize === value
                ? "border-emerald-400/40 bg-emerald-500/15 text-emerald-200"
                : "border-white/10 bg-white/[0.04] text-slate-300 hover:bg-white/10"
            }`}
          >
            {value >= 1024 ? "1 МБ" : `${value} КБ`}
          </button>
        ))}
      </div>

      {scanMutation.isPending ? (
        <Card className="flex items-center gap-2 p-4 text-xs text-slate-300">
          <RefreshIcon className="size-4 animate-spin text-emerald-300" />
          Считаем хэши файлов — на большом профиле это занимает до минуты…
        </Card>
      ) : null}

      {note ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-3.5">
          <p className="flex items-center gap-2 text-xs text-emerald-100">
            <CheckIcon className="size-3.5" /> {note}
          </p>
        </Card>
      ) : null}

      {scan && !scanMutation.isPending ? (
        <div className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-white/10 bg-gradient-to-br from-violet-500/[0.10] via-white/[0.03] to-transparent p-4">
          <div>
            <p className="text-[11px] uppercase tracking-wide text-violet-300/70">Можно освободить</p>
            <p className="text-2xl font-semibold tracking-tight text-white">{formatBytes(totalWasted)}</p>
            <p className="text-xs text-slate-400">
              {groups.length} групп · проверено {formatCount(scan.scannedFiles)} файлов за{" "}
              {(scan.elapsedMs / 1000).toFixed(1)} с
              {scan.truncated ? " · поиск ограничен по времени" : ""}
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              variant="secondary"
              size="sm"
              disabled={groups.length === 0}
              onClick={() => void markAllDuplicates(true)}
            >
              Удалить копии, оставить по одной
            </Button>
            <Button
              variant="danger"
              size="sm"
              disabled={groups.length === 0}
              onClick={() => void markAllDuplicates(false)}
            >
              <TrashIcon className="size-3.5" />
              Удалить все найденные
            </Button>
          </div>
        </div>
      ) : null}

      {scan?.error ? (
        <Card className="border-amber-400/25 bg-amber-400/[0.06] p-3.5 text-xs text-amber-200">
          <span className="flex items-center gap-2">
            <AlertIcon className="size-3.5" /> {scan.error}
          </span>
        </Card>
      ) : null}

      {scan && groups.length === 0 && !scanMutation.isPending ? (
        <EmptyState
          icon={<CheckIcon className="size-6" />}
          title="Дубликатов не найдено"
          description={`Файлов больше ${minSize} КБ с одинаковым содержимым нет. Можно снизить минимальный размер и проверить снова.`}
        />
      ) : null}

      {!scan && !scanMutation.isPending ? (
        <EmptyState
          icon={<FilesIcon className="size-6" />}
          title="Ещё не искали"
          description="Просканируем домашнюю папку: сначала группировка по размеру, потом сравнение хэшей — так поиск остаётся быстрым."
          action={
            <Button variant="primary" onClick={runScan}>
              Найти дубликаты
            </Button>
          }
        />
      ) : null}

      <div className="space-y-2">
        {groups.map((group) => (
          <Card key={group.hash} className="overflow-hidden">
            <div className="flex flex-wrap items-center gap-2 border-b border-white/[0.07] px-4 py-2.5">
              <Badge variant="info">{formatBytes(group.size)}</Badge>
              <span className="text-xs text-slate-300">{group.files.length} копии</span>
              <span className="text-[11px] text-slate-500">
                лишних {formatBytes(group.wasted)}
              </span>
              <div className="ml-auto flex items-center gap-2">
                <label className="flex cursor-pointer items-center gap-1.5 text-[11px] text-slate-400">
                  <input
                    type="checkbox"
                    className="size-3.5 accent-emerald-500"
                    checked={keepFirst.has(group.hash)}
                    onChange={(event) =>
                      setKeepFirst((current) => {
                        const next = new Set(current);
                        if (event.target.checked) next.add(group.hash);
                        else next.delete(group.hash);
                        return next;
                      })
                    }
                  />
                  оставить первый
                </label>
                <Button size="sm" variant="danger" onClick={() => markDuplicatesOf(group)}>
                  <TrashIcon className="size-3.5" />
                  Удалить {keepFirst.has(group.hash) ? group.files.length - 1 : group.files.length}
                </Button>
              </div>
            </div>
            <div className="divide-y divide-white/[0.05]">
              {group.files.map((file, index) => (
                <div key={file.path} className="flex items-center gap-2 px-4 py-1.5">
                  <span
                    className={`shrink-0 rounded px-1.5 py-0.5 text-[10px] ${
                      index === 0 && keepFirst.has(group.hash)
                        ? "bg-emerald-500/15 text-emerald-300"
                        : "bg-white/[0.05] text-slate-500"
                    }`}
                  >
                    {index === 0 && keepFirst.has(group.hash) ? "оставить" : `копия ${index + 1}`}
                  </span>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-xs text-slate-200" title={file.name}>
                      {file.name}
                    </p>
                    <p className="truncate font-mono text-[10px] text-slate-600" title={file.dir}>
                      {file.dir}
                    </p>
                  </div>
                  <span className="shrink-0 text-[10px] text-slate-500">{formatDate(file.modified)}</span>
                  <button
                    onClick={() => void rpcClient.system.reveal({ path: file.path })}
                    className="shrink-0 rounded px-1.5 py-0.5 text-[10px] text-slate-500 hover:bg-white/10 hover:text-slate-200"
                    title="Показать в проводнике"
                  >
                    открыть
                  </button>
                </div>
              ))}
            </div>
          </Card>
        ))}
      </div>

      <Modal
        open={Boolean(pending)}
        onClose={() => setPending(null)}
        title="Переместить дубликаты в корзину?"
        description={
          pending ? (
            <>
              {formatCount(pending.paths.length)} файлов на{" "}
              <b className="text-white">{formatBytes(pending.bytes)}</b>. Они будут в корзине, пока вы
              её не очистите.
            </>
          ) : null
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setPending(null)}>
              Отмена
            </Button>
            <Button variant="danger" onClick={confirmDelete} loading={deleteMutation.isPending}>
              <TrashIcon className="size-4" />
              В корзину
            </Button>
          </>
        }
      />
    </div>
  );
}
