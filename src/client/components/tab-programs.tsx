import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { InstalledProgram } from "@/shared/types";
import { queryClient, rpcClient } from "@/client/rpc-client";
import { formatBytes, formatCount, formatDate } from "@/client/lib/format";
import { Badge, Button, Card, Modal, SectionTitle } from "./ui";
import {
  AlertIcon,
  CheckIcon,
  FolderIcon,
  PackageIcon,
  RefreshIcon,
  SearchIcon,
  ShieldIcon,
  SpinnerIcon,
  TrashIcon,
} from "./icons";
import { t } from "@/client/i18n";

type SortKey = "size" | "name" | "date";

const EMPTY_PROGRAMS: InstalledProgram[] = [];

function installTimestamp(value: string | null): number {
  if (!value || !/^\d{8}$/.test(value)) return 0;
  return Number(`${value.slice(0, 4)}${value.slice(4, 6)}${value.slice(6, 8)}`);
}

export function ProgramsTab({ admin }: { admin: boolean }) {
  const [search, setSearch] = useState("");
  const [sort, setSort] = useState<SortKey>("size");
  const [pending, setPending] = useState<InstalledProgram | null>(null);
  const [quiet, setQuiet] = useState(false);
  const [launched, setLaunched] = useState<string | null>(null);

  const programsQuery = useQuery({
    ...queryClient.system.programs.queryOptions({ input: {} }),
    staleTime: 20_000,
  });

  const uninstallMutation = useMutation(queryClient.system.uninstall.mutationOptions());

  const programs = programsQuery.data ?? EMPTY_PROGRAMS;

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    const list = programs.filter((program) => {
      if (!needle) return true;
      return (
        program.name.toLowerCase().includes(needle) ||
        (program.publisher ?? "").toLowerCase().includes(needle)
      );
    });

    return [...list].sort((a, b) => {
      if (sort === "size") return b.sizeBytes - a.sizeBytes;
      if (sort === "date") return installTimestamp(b.installDate) - installTimestamp(a.installDate);
      return a.name.localeCompare(b.name, "ru");
    });
  }, [programs, search, sort]);

  const totalSize = programs.reduce((sum, program) => sum + program.sizeBytes, 0);

  async function handleUninstall() {
    if (!pending) return;
    const response = await uninstallMutation.mutateAsync({ key: pending.key, quiet });
    if (response.launched) {
      setLaunched(pending.name);
      setPending(null);
      setTimeout(() => void programsQuery.refetch(), 6000);
      setTimeout(() => void programsQuery.refetch(), 20000);
    }
  }

  return (
    <div className="space-y-5">
      <SectionTitle
        title={t("Установленные программы")}
        subtitle={t("{count} программ · {size} по данным реестра", {
          count: formatCount(programs.length),
          size: formatBytes(totalSize),
        })}
        action={
          <Button
            variant="secondary"
            onClick={() => void programsQuery.refetch()}
            loading={programsQuery.isFetching}
          >
            <RefreshIcon className="size-4" />
            {t("Обновить список")}
          </Button>
        }
      />

      {!admin ? (
        <div className="flex items-start gap-2.5 rounded-xl border border-amber-400/25 bg-amber-400/[0.07] px-3.5 py-2.5 text-[11px] leading-relaxed text-amber-200/90">
          <ShieldIcon className="mt-0.5 size-4 shrink-0" />
          {t("Программа запущена без прав администратора. Системные приложения не удалятся — для них потребуются права администратора.")}
        </div>
      ) : null}

      {launched ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-4">
          <div className="flex items-start gap-3">
            <CheckIcon className="mt-0.5 size-4 shrink-0 text-emerald-300" />
            <div className="min-w-0 text-xs leading-relaxed text-emerald-100">
              <p className="font-medium">{t("Запущено удаление:")} {launched}</p>
              <p className="mt-0.5 text-emerald-200/70">
                {t("Подтвердите действия в окне установщика. Список обновится автоматически, когда удаление завершится.")}
              </p>
            </div>
            <button
              className="ml-auto text-xs text-emerald-200/60 hover:text-emerald-100"
              onClick={() => setLaunched(null)}
            >
              {t("скрыть")}
            </button>
          </div>
        </Card>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-0 flex-1">
          <SearchIcon className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-slate-500" />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder={t("Поиск по названию или издателю")}
            className="w-full rounded-xl border border-white/10 bg-white/[0.04] py-2 pl-9 pr-3 text-sm text-slate-100 outline-none placeholder:text-slate-500 focus:border-emerald-400/40 focus:bg-white/[0.06]"
          />
        </div>
        <div className="flex overflow-hidden rounded-xl border border-white/10">
          {(
            [
              ["size", t("По размеру")],
              ["name", t("По имени")],
              ["date", t("По дате")],
            ] as [SortKey, string][]
          ).map(([key, label]) => (
            <button
              key={key}
              onClick={() => setSort(key)}
              className={`px-2.5 py-2 text-xs transition-colors ${
                sort === key
                  ? "bg-white/[0.09] text-white"
                  : "bg-white/[0.03] text-slate-400 hover:bg-white/[0.06]"
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {programsQuery.isLoading ? (
        <div className="space-y-2">
          {[0, 1, 2, 3, 4].map((index) => (
            <div key={index} className="h-14 animate-pulse rounded-xl bg-white/[0.04]" />
          ))}
        </div>
      ) : null}

      {programsQuery.isError ? (
        <Card className="border-rose-400/30 bg-rose-500/[0.07] p-4 text-xs text-rose-200">
          <span className="flex items-center gap-2">
            <AlertIcon className="size-4" /> {t("Не удалось прочитать список программ из реестра.")}
          </span>
        </Card>
      ) : null}

      {!programsQuery.isLoading && filtered.length === 0 ? (
        <Card className="p-8 text-center">
          <PackageIcon className="mx-auto size-8 text-slate-500" />
          <p className="mt-2 text-sm text-slate-200">{t("Ничего не найдено")}</p>
          <p className="mt-1 text-xs text-slate-500">
            {programs.length === 0 ? t("Список программ пуст") : t("Измените запрос поиска")}
          </p>
        </Card>
      ) : null}

      {filtered.length > 0 ? (
        <Card className="overflow-hidden">
          <div className="grid grid-cols-[1fr_auto_auto] items-center gap-3 border-b border-white/[0.08] px-4 py-2.5 text-[10px] uppercase tracking-wide text-slate-500 sm:grid-cols-[1fr_110px_96px_130px]">
            <span>{t("Программа")}</span>
            <span className="hidden sm:block">{t("Версия")}</span>
            <span className="text-right">{t("Размер")}</span>
            <span className="text-right">{t("Действие")}</span>
          </div>
          <div className="divide-y divide-white/[0.06]">
            {filtered.map((program) => (
              <div
                key={program.key}
                className="grid grid-cols-[1fr_auto_auto] items-center gap-3 px-4 py-2.5 sm:grid-cols-[1fr_110px_96px_130px]"
              >
                <div className="min-w-0">
                  <p className="truncate text-sm text-slate-100" title={program.name}>
                    {program.name}
                  </p>
                  <p className="truncate text-[11px] text-slate-500">
                    {program.publisher || t("Издатель неизвестен")}
                    {program.installDate ? t(" · {date}", { date: formatDate(program.installDate) }) : ""}
                  </p>
                  {program.location ? (
                    <button
                      onClick={() => void rpcClient.system.reveal({ path: program.location! })}
                      className="mt-0.5 flex max-w-full items-center gap-1 truncate font-mono text-[10px] text-slate-600 transition-colors hover:text-emerald-300"
                      title={t("Открыть папку установки: {path}", { path: program.location })}
                    >
                      <FolderIcon className="size-3 shrink-0" />
                      <span className="truncate">{program.location}</span>
                    </button>
                  ) : (
                    <p className="mt-0.5 text-[10px] text-slate-500">{t("путь не указан в реестре")}</p>
                  )}
                </div>
                <span className="hidden truncate text-xs text-slate-400 sm:block" title={program.version ?? ""}>
                  {program.version || "—"}
                </span>
                <span className="text-right text-xs tabular-nums text-slate-300">
                  {program.sizeBytes > 0 ? formatBytes(program.sizeBytes, 0) : "—"}
                </span>
                <div className="flex justify-end">
                  {program.uninstallString ? (
                    <Button
                      size="sm"
                      variant="danger"
                      onClick={() => {
                        uninstallMutation.reset();
                        setPending(program);
                        setQuiet(Boolean(program.quietUninstallString));
                      }}
                    >
                      <TrashIcon className="size-3.5" />
                      {t("Удалить")}
                    </Button>
                  ) : (
                    <Badge variant="neutral">{t("нет удаления")}</Badge>
                  )}
                </div>
              </div>
            ))}
          </div>
        </Card>
      ) : null}

      <Modal
        open={Boolean(pending)}
        onClose={() => setPending(null)}
        title={t("Удалить программу?")}
        description={
          pending ? (
            <>
              <b className="text-white">{pending.name}</b>
              {pending.publisher ? ` · ${pending.publisher}` : ""}
              {pending.version ? ` · ${pending.version}` : ""}
            </>
          ) : null
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setPending(null)}>
              {t("Отмена")}
            </Button>
            <Button variant="danger" onClick={handleUninstall} loading={uninstallMutation.isPending}>
              <TrashIcon className="size-4" />
              {t("Удалить")}
            </Button>
          </>
        }
      >
        <div className="space-y-3">
          <label
            className={`flex items-start gap-2.5 rounded-xl border border-white/10 bg-white/[0.03] p-3 text-xs ${
              pending?.quietUninstallString ? "cursor-pointer" : "opacity-60"
            }`}
          >
            <input
              type="checkbox"
              className="mt-0.5 size-4 accent-emerald-500"
              checked={quiet}
              disabled={!pending?.quietUninstallString}
              onChange={(event) => setQuiet(event.target.checked)}
            />
            <span>
              <span className="text-slate-200">{t("Тихое удаление")}</span>
              <span className="mt-0.5 block text-[11px] text-slate-500">
                {pending?.quietUninstallString
                  ? t("Без окон мастера, с параметрами издателя. Недоступно, если у программы нет тихого режима.")
                  : t("У этой программы нет команды тихого удаления.")}
              </span>
            </span>
          </label>

          {uninstallMutation.data && !uninstallMutation.data.launched ? (
            <p className="rounded-lg border border-rose-400/30 bg-rose-500/10 p-2.5 text-[11px] text-rose-200">
              {uninstallMutation.data.error}
            </p>
          ) : null}

          <p className="flex items-start gap-2 text-[11px] leading-relaxed text-slate-500">
            <AlertIcon className="mt-0.5 size-3.5 shrink-0" />
            {t("Откроется установщик программы. Не закрывайте его окно, пока удаление не завершится.")}
          </p>

          {uninstallMutation.isPending ? (
            <p className="flex items-center gap-2 text-[11px] text-slate-400">
              <SpinnerIcon className="size-3.5" /> {t("Запускаем установщик…")}
            </p>
          ) : null}
        </div>
      </Modal>
    </div>
  );
}
