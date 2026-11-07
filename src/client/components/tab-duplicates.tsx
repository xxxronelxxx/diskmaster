import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import type { DuplicateGroup, DuplicateScan } from "@/shared/types";
import { queryClient, rpcClient } from "@/client/rpc-client";
import { formatBytes, formatCount, formatDate, shortPath } from "@/client/lib/format";
import { Badge, Button, Card, EmptyState, Modal, Progress, SectionTitle } from "./ui";
import { AlertIcon, CheckIcon, FilesIcon, RefreshIcon, TrashIcon } from "./icons";
import { t } from "@/client/i18n";

const MIN_SIZES = [16, 64, 256, 1024];

export function DuplicatesTab() {
  const [minSize, setMinSize] = useState(64);
  const [scan, setScan] = useState<DuplicateScan | null>(null);
  const [keepFirst, setKeepFirst] = useState<Set<string>>(new Set());
  const [removed, setRemoved] = useState<Set<string>>(new Set());
  const [pending, setPending] = useState<{ paths: string[]; bytes: number } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [root, setRoot] = useState<string | null>(() => localStorage.getItem("dm.dup.root"));
  const [exclude, setExclude] = useState<string[]>(() => {
    try {
      const raw = localStorage.getItem("dm.dup.exclude");
      return raw ? (JSON.parse(raw) as string[]) : [];
    } catch {
      return [];
    }
  });
  const [groupLimit, setGroupLimit] = useState(100);
  const [strategy, setStrategy] = useState<"first" | "newest" | "oldest" | "shortest">(
    () => (localStorage.getItem("dm.dup.strategy") as "first" | "newest" | "oldest" | "shortest") || "first",
  );

  const scanMutation = useMutation(queryClient.system.duplicatesScan.mutationOptions());
  const cancelMutation = useMutation(queryClient.system.duplicatesCancel.mutationOptions());
  const deleteMutation = useMutation(queryClient.system.folderDelete.mutationOptions());

  const progressQuery = useQuery({
    ...queryClient.system.duplicatesStatus.queryOptions(),
    enabled: scanMutation.isPending,
    refetchInterval: scanMutation.isPending ? 500 : false,
  });
  const progress = progressQuery.data;

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
  const renderedGroups = groups.slice(0, groupLimit);

  async function runScan() {
    setNote(null);
    setRemoved(new Set());
    setKeepFirst(new Set());
    const result = (await scanMutation.mutateAsync({
      minSizeKb: minSize,
      root: root ?? undefined,
      exclude,
    })) as DuplicateScan;
    if (result.error) setNote(result.error);
    setScan(result);
  }

  function keepIndex(group: DuplicateGroup): number {
    if (strategy === "newest") {
      return group.files.reduce((best, file, index) => (file.modified > group.files[best].modified ? index : best), 0);
    }
    if (strategy === "oldest") {
      return group.files.reduce((best, file, index) => (file.modified < group.files[best].modified ? index : best), 0);
    }
    if (strategy === "shortest") {
      return group.files.reduce((best, file, index) => (file.path.length < group.files[best].path.length ? index : best), 0);
    }
    return 0;
  }

  function filesToDelete(group: DuplicateGroup): string[] {
    const keep = keepIndex(group);
    if (keepFirst.has(group.hash)) {
      return group.files.filter((_, index) => index !== keep).map((file) => file.path);
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
      const keep = keepIndex(group);
      const targets = keepOne
        ? group.files.filter((_, index) => index !== keep)
        : group.files;
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
        ? t("В корзину отправлено {files} файлов · {bytes}", {
            files: formatCount(response.files),
            bytes: formatBytes(response.bytes),
          })
        : t("Ошибки: {errors}", { errors: response.errors.slice(0, 2).join("; ") }),
    );
    setPending(null);
  }

  return (
    <div className="space-y-5">
      <SectionTitle
        title={t("Дубликаты")}
        subtitle={t("Поиск одинаковых файлов по содержимому (BLAKE3), а не по имени")}
        action={
          scanMutation.isPending ? (
            <Button variant="danger" onClick={() => void cancelMutation.mutateAsync({})}>
              {t("Остановить")}
            </Button>
          ) : (
            <Button variant="primary" onClick={runScan}>
              <RefreshIcon className="size-4" />
              {scan ? t("Сканировать снова") : t("Найти дубликаты")}
            </Button>
          )
        }
      />

      {scanMutation.isPending && progress ? (
        <Card className="p-4">
          <div className="flex items-baseline justify-between gap-2">
            <p className="text-xs text-slate-300">
              {progress.phase === "hashing"
                ? t("Считаем хэши файлов…")
                : t("Обходим файлы и группируем по размеру…")}
            </p>
            <span className="text-[11px] tabular-nums text-slate-400">{progress.percent}%</span>
          </div>
          <Progress value={progress.percent} className="mt-2" />
          <p className="mt-2 text-[11px] text-slate-500">
            {t("просмотрено")} {formatCount(progress.scanned)} {t("файлов")}
            {progress.total > 0
              ? t(" · проверено {hashed} из {total}", {
                  hashed: formatCount(progress.hashed),
                  total: formatCount(progress.total),
                })
              : ""}
          </p>
        </Card>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <span className="text-xs text-slate-500">{t("Минимальный размер файла:")}</span>
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
            {value >= 1024 ? t("1 МБ") : `${value} КБ`}
          </button>
        ))}
      </div>

      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="text-slate-500">{t("Папка:")}</span>
        <span className="max-w-72 truncate rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1 text-slate-300" title={root ?? t("Домашняя папка")}>
          {root ? shortPath(root, 48) : t("Домашняя папка")}
        </span>
        <Button
          size="sm"
          variant="secondary"
          onClick={async () => {
            const selected = await open({ directory: true, multiple: false });
            if (typeof selected === "string") {
              setRoot(selected);
              localStorage.setItem("dm.dup.root", selected);
            }
          }}
          disabled={scanMutation.isPending}
        >
          {t("Выбрать")}
        </Button>
        {root ? (
          <Button
            size="sm"
            variant="ghost"
            onClick={() => {
              setRoot(null);
              localStorage.removeItem("dm.dup.root");
            }}
            disabled={scanMutation.isPending}
          >
            {t("Сбросить")}
          </Button>
        ) : null}
        <Button
          size="sm"
          variant="secondary"
          onClick={async () => {
            const selected = await open({ directory: true, multiple: false });
            if (typeof selected === "string" && !exclude.includes(selected)) {
              const next = [...exclude, selected];
              setExclude(next);
              localStorage.setItem("dm.dup.exclude", JSON.stringify(next));
            }
          }}
          disabled={scanMutation.isPending}
        >
          {t("+ Исключение")}
        </Button>
        {exclude.map((path) => (
          <button
            key={path}
            onClick={() => {
              const next = exclude.filter((value) => value !== path);
              setExclude(next);
              localStorage.setItem("dm.dup.exclude", JSON.stringify(next));
            }}
            className="max-w-56 truncate rounded-lg border border-rose-400/25 bg-rose-500/10 px-2 py-1 text-rose-200"
            title={t("Убрать исключение: {path}", { path })}
          >
            {shortPath(path, 32)} ×
          </button>
        ))}
      </div>

      <div className="flex flex-wrap items-center gap-2 text-xs">
        <span className="text-slate-500">{t("При удалении копий оставлять:")}</span>
        {([
          ["first", t("первый")],
          ["newest", t("самый новый")],
          ["oldest", t("самый старый")],
          ["shortest", t("короткий путь")],
        ] as const).map(([value, label]) => (
          <button
            key={value}
            onClick={() => {
              setStrategy(value);
              localStorage.setItem("dm.dup.strategy", value);
            }}
            className={`rounded-lg border px-2.5 py-1 ${strategy === value ? "border-emerald-400/40 bg-emerald-500/15 text-emerald-200" : "border-white/10 bg-white/[0.04] text-slate-300"}`}
          >
            {label}
          </button>
        ))}
      </div>

      {scanMutation.isPending ? (
        <Card className="flex items-center gap-2 p-4 text-xs text-slate-300">
          <RefreshIcon className="size-4 animate-spin text-emerald-300" />
          {t("Считаем хэши файлов — на большом профиле это занимает до минуты…")}
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
            <p className="text-[11px] uppercase tracking-wide text-violet-300/70">{t("Можно освободить")}</p>
            <p className="text-2xl font-semibold tracking-tight text-white">{formatBytes(totalWasted)}</p>
            <p className="text-xs text-slate-400">
              {groups.length} {t("групп · проверено")} {formatCount(scan.scannedFiles)} {t("файлов за")}{" "}
              {(scan.elapsedMs / 1000).toFixed(1)} {t("с")}
              {scan.truncated ? t(" · поиск ограничен по времени") : ""}
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              variant="secondary"
              size="sm"
              disabled={groups.length === 0}
              onClick={() => void markAllDuplicates(true)}
            >
              {t("Удалить копии, оставить по одной")}
            </Button>
            <Button
              variant="danger"
              size="sm"
              disabled={groups.length === 0}
              onClick={() => void markAllDuplicates(false)}
            >
              <TrashIcon className="size-3.5" />
              {t("Удалить все найденные")}
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
          title={t("Дубликатов не найдено")}
          description={t("Файлов больше {size} КБ с одинаковым содержимым нет. Можно снизить минимальный размер и проверить снова.", { size: minSize })}
        />
      ) : null}

      {!scan && !scanMutation.isPending ? (
        <EmptyState
          icon={<FilesIcon className="size-6" />}
          title={t("Ещё не искали")}
          description={t("Просканируем домашнюю папку: сначала группировка по размеру, потом сравнение хэшей — так поиск остаётся быстрым.")}
          action={
            <Button variant="primary" onClick={runScan}>
              {t("Найти дубликаты")}
            </Button>
          }
        />
      ) : null}

      <div className="space-y-2">
        {renderedGroups.map((group) => (
          <Card
            key={group.hash}
            className="overflow-hidden [contain-intrinsic-size:140px] [content-visibility:auto]"
          >
            <div className="flex flex-wrap items-center gap-2 border-b border-white/[0.07] px-4 py-2.5">
              <Badge variant="info">{formatBytes(group.size)}</Badge>
              <span className="text-xs text-slate-300">{group.files.length} {t("копии")}</span>
              <span className="text-[11px] text-slate-500">
                {t("лишних")} {formatBytes(group.wasted)}
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
                  {t("оставить первый")}
                </label>
                <Button size="sm" variant="danger" onClick={() => markDuplicatesOf(group)}>
                  <TrashIcon className="size-3.5" />
                  {t("Удалить")} {keepFirst.has(group.hash) ? group.files.length - 1 : group.files.length}
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
                    {index === 0 && keepFirst.has(group.hash) ? t("оставить") : `копия ${index + 1}`}
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
                    title={t("Показать в проводнике")}
                  >
                    {t("открыть")}
                  </button>
                </div>
              ))}
            </div>
          </Card>
        ))}
        {groups.length > renderedGroups.length ? (
          <div className="flex justify-center pt-2">
            <Button size="sm" variant="secondary" onClick={() => setGroupLimit((current) => current + 100)}>
              {t("Показать ещё 100")}
            </Button>
          </div>
        ) : null}
      </div>

      <Modal
        open={Boolean(pending)}
        onClose={() => setPending(null)}
        title={t("Переместить дубликаты в корзину?")}
        description={
          pending ? (
            <>
              {formatCount(pending.paths.length)} {t("файлов на")}{" "}
              <b className="text-white">{formatBytes(pending.bytes)}</b>{t(". Они будут в корзине, пока вы её не очистите.")}
            </>
          ) : null
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setPending(null)}>
              {t("Отмена")}
            </Button>
            <Button variant="danger" onClick={confirmDelete} loading={deleteMutation.isPending}>
              <TrashIcon className="size-4" />
              {t("В корзину")}
            </Button>
          </>
        }
      />
    </div>
  );
}
