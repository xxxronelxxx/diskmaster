import { useCallback, useEffect, useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { FolderAnalysis, FolderEntry } from "@/shared/types";
import { queryClient, rpcClient } from "@/client/rpc-client";
import { formatBytes, formatCount, formatDate } from "@/client/lib/format";
import { Button, Card, EmptyState, Modal, SectionTitle } from "./ui";
import { Treemap } from "./treemap";
import {
  AlertIcon,
  ChevronIcon,
  FilesIcon,
  FolderIcon,
  RefreshIcon,
  SpinnerIcon,
  TrashIcon,
} from "./icons";
import { t, translateError } from "@/client/i18n";

type SortKey = "size" | "name" | "files";

const COLORS = [
  "#34d399", "#38bdf8", "#a78bfa", "#fbbf24", "#fb7185",
  "#22d3ee", "#f472b6", "#4ade80", "#facc15", "#818cf8",
];

type ContextTarget = { x: number; y: number; entry: FolderEntry };

/* -------------------------------------------------------------------------- */
/*                              context menu                                  */
/* -------------------------------------------------------------------------- */

function ContextMenu({
  state,
  onOpen,
  onDelete,
  onReveal,
  onClose,
}: {
  state: ContextTarget;
  onOpen: () => void;
  onDelete: () => void;
  onReveal: () => void;
  onClose: () => void;
}) {
  useEffect(() => {
    const close = () => onClose();
    window.addEventListener("click", close);
    window.addEventListener("scroll", close, true);
    window.addEventListener("contextmenu", close);
    window.addEventListener("keydown", close);
    return () => {
      window.removeEventListener("click", close);
      window.removeEventListener("scroll", close, true);
      window.removeEventListener("contextmenu", close);
      window.removeEventListener("keydown", close);
    };
  }, [onClose]);

  return (
    <div
      className="fixed z-50 min-w-52 overflow-hidden rounded-xl border border-white/12 bg-[var(--app-menu)] py-1 shadow-2xl"
      style={{
        left: Math.min(state.x, window.innerWidth - 230),
        top: Math.min(state.y, window.innerHeight - 160),
      }}
      onClick={(event) => event.stopPropagation()}
    >
      <div className="border-b border-white/[0.07] px-3 py-1.5">
        <p className="truncate text-[11px] font-medium text-slate-200">{state.entry.name}</p>
        <p className="text-[10px] text-slate-500">
          {formatBytes(state.entry.size)}
          {state.entry.isDir ? t(" · {files} файлов", { files: formatCount(state.entry.files) }) : ""}
        </p>
      </div>
      {state.entry.isDir ? (
        <button
          onClick={() => {
            onOpen();
            onClose();
          }}
          className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-slate-300 transition-colors hover:bg-white/[0.07]"
        >
          {t("Показать содержимое")}
        </button>
      ) : null}
      <button
        onClick={() => {
          onReveal();
          onClose();
        }}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-slate-300 transition-colors hover:bg-white/[0.07]"
      >
        {t("Показать в проводнике")}
      </button>
      <button
        onClick={() => {
          onDelete();
          onClose();
        }}
        className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-xs text-rose-300 transition-colors hover:bg-rose-500/15"
      >
        <TrashIcon className="size-3.5" />
        {t("Удалить ·")} {formatBytes(state.entry.size)}
      </button>
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*                                tree node                                   */
/* -------------------------------------------------------------------------- */

type NodeProps = {
  entry: FolderEntry;
  depth: number;
  total: number;
  colorIndex: number;
  selected: string | null;
  onSelect: (path: string) => void;
  onDelete: (entry: FolderEntry) => void;
  onContext: (target: ContextTarget) => void;
  expandedPaths: Set<string>;
  onToggleExpand: (path: string) => void;
};

function FolderNode({
  entry,
  depth,
  total,
  colorIndex,
  selected,
  onSelect,
  onDelete,
  onContext,
  expandedPaths,
  onToggleExpand,
}: NodeProps) {
  const expanded = expandedPaths.has(entry.path);
  const share = total > 0 ? (entry.size / total) * 100 : 0;
  const isSelected = selected === entry.path;

  // Children are fetched only when a node is actually expanded.
  const childrenQuery = useQuery({
    ...queryClient.system.folderAnalyze.queryOptions({ input: { path: entry.path } }),
    enabled: expanded && entry.isDir && entry.children > 0,
    staleTime: 30_000,
  });

  const children = (childrenQuery.data as FolderAnalysis | undefined)?.entries ?? [];

  return (
    <div>
      <div
        onClick={() => onSelect(entry.path)}
        onDoubleClick={() => {
          if (entry.isDir && entry.children > 0) onToggleExpand(entry.path);
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          onSelect(entry.path);
          onContext({ x: event.clientX, y: event.clientY, entry });
        }}
        className={`group flex cursor-pointer items-center gap-2 py-1.5 pr-3 transition-colors ${
          isSelected ? "bg-emerald-500/[0.09]" : "hover:bg-white/[0.03]"
        }`}
        style={{ paddingLeft: 12 + depth * 18 }}
      >
        <button
          onClick={(event) => {
            event.stopPropagation();
            if (entry.isDir && entry.children > 0) onToggleExpand(entry.path);
          }}
          className={`grid size-4 shrink-0 place-items-center rounded transition-colors ${
            entry.isDir && entry.children > 0
              ? "text-slate-500 hover:bg-white/10 hover:text-slate-200"
              : "text-transparent"
          }`}
          tabIndex={entry.isDir && entry.children > 0 ? 0 : -1}
          aria-label={expanded ? t("Свернуть") : t("Развернуть")}
        >
          {childrenQuery.isFetching ? (
            <SpinnerIcon className="size-3" />
          ) : (
            <ChevronIcon
              className={`size-3 transition-transform ${expanded ? "rotate-90" : ""}`}
            />
          )}
        </button>

        <span className="shrink-0 text-slate-500">
          {entry.isDir ? <FolderIcon className="size-4" /> : <FilesIcon className="size-4" />}
        </span>

        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <p className="truncate text-[13px] text-slate-100" title={entry.path}>
              {entry.name}
            </p>
            {entry.isDir ? (
              <span className="shrink-0 text-[10px] text-slate-600">
                {formatCount(entry.children)} {t("объ.")}
              </span>
            ) : null}
          </div>
          <div className="mt-1 flex items-center gap-2">
            <div className="h-1.5 flex-1 overflow-hidden rounded-full bg-white/[0.06]">
              <div
                className="h-full rounded-full transition-[width] duration-300"
                style={{
                  width: `${Math.max(share > 0 ? 1.5 : 0, Math.min(100, share))}%`,
                  backgroundColor: COLORS[colorIndex % COLORS.length],
                }}
              />
            </div>
            <span className="w-10 shrink-0 text-right text-[10px] tabular-nums text-slate-600">
              {share >= 0.1 ? `${share.toFixed(share >= 10 ? 0 : 1)}%` : "<0,1%"}
            </span>
          </div>
        </div>

        <div className="shrink-0 text-right">
          <p className="text-[13px] font-semibold tabular-nums text-white">{formatBytes(entry.size)}</p>
          <p className="text-[10px] text-slate-600">
            {entry.isDir ? t("{files} файлов", { files: formatCount(entry.files) }) : formatDate(entry.modified)}
          </p>
        </div>

        <button
          onClick={(event) => {
            event.stopPropagation();
            onDelete(entry);
          }}
          className="grid size-7 shrink-0 place-items-center rounded-lg text-slate-600 opacity-0 transition-opacity hover:bg-rose-500/15 hover:text-rose-300 group-hover:opacity-100"
          title={t("Удалить")}
        >
          <TrashIcon className="size-3.5" />
        </button>
      </div>

      {expanded ? (
        childrenQuery.isLoading ? (
          <div
            className="flex items-center gap-2 py-2 text-[11px] text-slate-500"
            style={{ paddingLeft: 34 + depth * 18 }}
          >
            <SpinnerIcon className="size-3" /> {t("Считаем размеры…")}
          </div>
        ) : children.length === 0 ? (
          <div
            className="py-1.5 text-[11px] text-slate-600"
            style={{ paddingLeft: 34 + depth * 18 }}
          >
            {t("пусто")}
          </div>
        ) : (
          children.map((child, index) => (
            <FolderNode
              key={child.path}
              entry={child}
              depth={depth + 1}
              total={entry.size}
              colorIndex={index}
              selected={selected}
              onSelect={onSelect}
              onDelete={onDelete}
              onContext={onContext}
              expandedPaths={expandedPaths}
              onToggleExpand={onToggleExpand}
            />
          ))
        )
      ) : null}
    </div>
  );
}

/* -------------------------------------------------------------------------- */
/*                                  tab                                       */
/* -------------------------------------------------------------------------- */

export function FoldersTab() {
  const [currentPath, setCurrentPath] = useState<string | undefined>(undefined);
  const [sort, setSort] = useState<SortKey>("size");
  const [view, setView] = useState<"list" | "map">("list");
  const [selected, setSelected] = useState<string | null>(null);
  const [context, setContext] = useState<ContextTarget | null>(null);
  const [pending, setPending] = useState<FolderEntry[] | null>(null);
  const [permanent, setPermanent] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());

  const rootsQuery = useQuery(queryClient.system.folderRoots.queryOptions());

  const analysisQuery = useQuery({
    ...queryClient.system.folderAnalyze.queryOptions({ input: { path: currentPath } }),
    staleTime: 30_000,
  });

  const deleteMutation = useMutation(queryClient.system.folderDelete.mutationOptions());

  const analysis = analysisQuery.data as FolderAnalysis | undefined;

  // Reset the tree when the root changes; the selection follows the data.
  const rootPath = analysis?.path ?? "";
  const [treeRoot, setTreeRoot] = useState("");
  if (rootPath && treeRoot !== rootPath) {
    setTreeRoot(rootPath);
    setExpandedPaths(new Set());
  }

  const entries = useMemo(() => {
    const list = [...(analysis?.entries ?? [])];
    if (sort === "name") return list.sort((a, b) => a.name.localeCompare(b.name, "ru"));
    if (sort === "files") return list.sort((a, b) => b.files - a.files);
    return list.sort((a, b) => b.size - a.size);
  }, [analysis?.entries, sort]);

  const total = analysis?.size ?? 0;

  const toggleExpand = useCallback((path: string) => {
    setExpandedPaths((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });
  }, []);

  const requestDelete = useCallback((target: FolderEntry) => {
    setPending([target]);
    setPermanent(false);
  }, []);

  const openRoot = useCallback((path: string) => {
    setCurrentPath(path);
    setSelected(null);
    setResult(null);
  }, []);

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.key !== "Delete" || !selected) return;
      const tag = (event.target as HTMLElement | null)?.tagName;
      if (tag === "INPUT" || tag === "TEXTAREA") return;
      const match = entries.find((entry) => entry.path === selected);
      if (!match) return;
      event.preventDefault();
      requestDelete(match);
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [selected, entries, requestDelete]);

  async function confirmDelete() {
    if (!pending) return;
    const response = await deleteMutation.mutateAsync({
      paths: pending.map((entry) => entry.path),
      permanent,
    });
    const names = pending.map((entry) => entry.name).join(", ");
    setPending(null);
    setResult(
      response.errors.length > 0
        ? t("Удалено {bytes}, ошибки: {errors}", {
            bytes: formatBytes(response.bytes),
            errors: response.errors.slice(0, 2).map(translateError).join("; "),
          })
        : t("Удалено {bytes} ({files} файлов): {names}", {
            bytes: formatBytes(response.bytes),
            files: formatCount(response.files),
            names,
          }),
    );
    await analysisQuery.refetch();
  }

  const crumbs = useMemo(() => {
    const root = analysis?.path;
    if (!root) return [];
    const parts: { label: string; path: string }[] = [];
    let cursor = root;
    while (cursor) {
      parts.unshift({
        label: cursor.split(/[\\/]/).filter(Boolean).pop() ?? cursor,
        path: cursor,
      });
      const next = cursor.replace(/[\\/][^\\/]+$/, "");
      if (!next || next === cursor) break;
      cursor = next;
    }
    return parts.slice(-5);
  }, [analysis?.path]);

  return (
    <div className="space-y-4">
      <SectionTitle
        title={t("Анализ диска")}
        subtitle={t("Как ncdu: раскрывайте папки стрелкой и удаляйте выборочно — файл или отдельную подпапку")}
        action={
          <Button
            variant="secondary"
            onClick={() => void analysisQuery.refetch()}
            loading={analysisQuery.isFetching}
          >
            <RefreshIcon className="size-4" />
            {t("Пересчитать")}
          </Button>
        }
      />

      <div className="flex flex-wrap items-center gap-1.5">
        {(rootsQuery.data ?? []).map((root) => (
          <button
            key={root.path}
            onClick={() => openRoot(root.path)}
            className={`rounded-lg border px-2.5 py-1 text-[11px] transition-colors ${
              analysis?.path === root.path
                ? "border-emerald-400/40 bg-emerald-500/15 text-emerald-200"
                : "border-white/10 bg-white/[0.03] text-slate-400 hover:bg-white/[0.07]"
            }`}
          >
            {root.label}
          </button>
        ))}
      </div>

      {crumbs.length > 0 ? (
        <div className="flex flex-wrap items-center gap-1 text-[11px] text-slate-500">
          {crumbs.map((crumb, index) => (
            <span key={crumb.path} className="flex items-center gap-1">
              {index > 0 ? <ChevronIcon className="size-3 text-slate-600" /> : null}
              <button
                onClick={() => openRoot(crumb.path)}
                className="max-w-44 truncate rounded px-1 py-0.5 font-mono hover:bg-white/[0.06] hover:text-slate-200"
                title={crumb.path}
              >
                {crumb.label}
              </button>
            </span>
          ))}
        </div>
      ) : null}

      {analysis?.error ? (
        <Card className="border-amber-400/25 bg-amber-400/[0.06] p-3.5">
          <p className="flex items-center gap-2 text-xs text-amber-200">
            <AlertIcon className="size-3.5" /> {analysis.error}
          </p>
        </Card>
      ) : null}

      {result ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-3.5">
          <p className="text-xs text-emerald-100">{result}</p>
        </Card>
      ) : null}

      {analysisQuery.isLoading ? (
        <Card className="flex items-center gap-2 p-4 text-xs text-slate-400">
          <SpinnerIcon className="size-4" /> {t("Считаем размеры папок…")}
        </Card>
      ) : null}

      {analysis && entries.length > 0 ? (
        <Card className="overflow-hidden">
          <div className="flex flex-wrap items-center justify-between gap-2 border-b border-white/[0.08] px-4 py-2.5">
            <div>
              <p className="text-[13px] font-medium text-white">
                {formatBytes(total)} · {formatCount(analysis.files)} {t("файлов")}
              </p>
              <p className="text-[10px] text-slate-500">
                {formatCount(entries.length)} {t("элементов ·")} {analysis.elapsedMs} {t("мс")}
                {analysis.truncated ? t(" · расчёт ограничен по времени") : ""}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() =>
                  setExpandedPaths((current) => {
                    if (current.size > 0) return new Set();
                    return new Set(entries.filter((entry) => entry.isDir && entry.children > 0).map((entry) => entry.path));
                  })
                }
                className="rounded-lg border border-white/10 px-2 py-1 text-[11px] text-slate-400 transition-colors hover:bg-white/[0.07] hover:text-slate-200"
              >
                {expandedPaths.size > 0 ? t("Свернуть всё") : t("Раскрыть всё")}
              </button>
              <div className="flex gap-1 rounded-lg border border-white/10 p-0.5">
                {(
                  [
                    ["list", t("Список")],
                    ["map", t("Карта")],
                  ] as ["list" | "map", string][]
                ).map(([key, label]) => (
                  <button
                    key={key}
                    onClick={() => setView(key)}
                    className={`rounded px-2 py-1 text-[11px] transition-colors ${
                      view === key ? "bg-white/[0.1] text-white" : "text-slate-400 hover:text-slate-200"
                    }`}
                  >
                    {label}
                  </button>
                ))}
              </div>
              <div className="flex gap-1 rounded-lg border border-white/10 p-0.5">
                {(
                  [
                    ["size", t("Размер")],
                    ["files", t("Файлы")],
                    ["name", t("Имя")],
                  ] as [SortKey, string][]
                ).map(([key, label]) => (
                  <button
                    key={key}
                    onClick={() => setSort(key)}
                    className={`rounded px-2 py-1 text-[11px] transition-colors ${
                      sort === key ? "bg-white/[0.1] text-white" : "text-slate-400 hover:text-slate-200"
                    }`}
                  >
                    {label}
                  </button>
                ))}
              </div>
            </div>
          </div>

          {view === "map" ? (
            <Treemap entries={entries} total={total} onOpen={openRoot} />
          ) : (
            <div className="divide-y divide-white/[0.05]">
              {entries.map((entry, index) => (
                <FolderNode
                  key={entry.path}
                  entry={entry}
                  depth={0}
                  total={total}
                  colorIndex={index}
                  selected={selected}
                  onSelect={setSelected}
                  onDelete={requestDelete}
                  onContext={setContext}
                  expandedPaths={expandedPaths}
                  onToggleExpand={toggleExpand}
                />
              ))}
            </div>
          )}
        </Card>
      ) : null}

      {analysis && entries.length === 0 && !analysis.error ? (
        <EmptyState
          icon={<FolderIcon className="size-6" />}
          title={t("Папка пуста")}
          description={t("Здесь нет файлов и подпапок.")}
        />
      ) : null}

      {!analysis && !analysisQuery.isLoading ? (
        <EmptyState
          icon={<FolderIcon className="size-6" />}
          title={t("Выберите папку")}
          description={t("Начните с домашней папки или любого диска, чтобы увидеть, что занимает место.")}
        />
      ) : null}

      {context ? (
        <ContextMenu
          state={context}
          onOpen={() => {
            if (context.entry.children > 0) toggleExpand(context.entry.path);
          }}
          onReveal={() => void rpcClient.system.reveal({ path: context.entry.path })}
          onDelete={() => requestDelete(context.entry)}
          onClose={() => setContext(null)}
        />
      ) : null}

      <Modal
        open={Boolean(pending)}
        onClose={() => setPending(null)}
        title={permanent ? t("Удалить безвозвратно?") : t("Переместить в корзину?")}
        description={
          pending ? (
            <>
              <b className="text-white">{pending[0]?.name}</b>
              {pending.length > 1 ? t(" и ещё {count}", { count: pending.length - 1 }) : ""} —{" "}
              {formatBytes(pending.reduce((sum, entry) => sum + entry.size, 0))}
              {permanent
                ? t(". Файлы будут удалены окончательно, восстановить их нельзя.")
                : t(". Их можно будет восстановить из корзины.")}
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
              {permanent ? t("Удалить навсегда") : t("В корзину")}
            </Button>
          </>
        }
      >
        <label className="flex cursor-pointer items-start gap-2.5 rounded-xl border border-white/10 bg-white/[0.03] p-3">
          <input
            type="checkbox"
            className="mt-0.5 size-4 accent-rose-500"
            checked={permanent}
            onChange={(event) => setPermanent(event.target.checked)}
          />
          <span className="text-[11px] leading-relaxed text-slate-400">
            <span className="text-slate-200">{t("Удалить безвозвратно")}</span>
            <span className="mt-0.5 block text-slate-500">
              {t("Быстрее и освобождает место сразу, но восстановление невозможно. Папки в корзину перемещаются через Windows.")}
            </span>
          </span>
        </label>
      </Modal>
    </div>
  );
}
