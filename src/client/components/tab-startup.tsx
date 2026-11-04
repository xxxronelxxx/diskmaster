import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { StartupItem } from "@/shared/types";
import { queryClient, rpcClient } from "@/client/rpc-client";
import { formatCount } from "@/client/lib/format";
import { Badge, Button, Card, EmptyState, Modal, SectionTitle } from "./ui";
import { AlertIcon, CheckIcon, PackageIcon, RefreshIcon, SearchIcon, TrashIcon } from "./icons";

type Filter = "all" | "user" | "system" | "disabled";

const EMPTY: StartupItem[] = [];

export function StartupTab() {
  const [search, setSearch] = useState("");
  const [filter, setFilter] = useState<Filter>("user");
  const [pendingDelete, setPendingDelete] = useState<StartupItem | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const listQuery = useQuery({
    ...queryClient.system.startupList.queryOptions(),
    staleTime: 20_000,
  });

  const toggleMutation = useMutation(queryClient.system.startupToggle.mutationOptions());
  const deleteMutation = useMutation(queryClient.system.startupDelete.mutationOptions());

  const items = listQuery.data ?? EMPTY;

  const filtered = useMemo(() => {
    const needle = search.trim().toLowerCase();
    return items.filter((item) => {
      if (filter === "user" && item.system) return false;
      if (filter === "system" && !item.system) return false;
      if (filter === "disabled" && item.enabled) return false;
      if (!needle) return true;
      return (
        item.name.toLowerCase().includes(needle) ||
        item.command.toLowerCase().includes(needle) ||
        (item.publisher ?? "").toLowerCase().includes(needle)
      );
    });
  }, [items, search, filter]);

  const enabledCount = items.filter((item) => item.enabled && !item.system).length;

  async function toggle(item: StartupItem) {
    const response = await toggleMutation.mutateAsync({ id: item.id, enabled: !item.enabled });
    setMessage(
      response.ok
        ? { ok: true, text: `${item.name}: ${item.enabled ? "отключено" : "включено"}` }
        : { ok: false, text: response.error ?? "Не удалось изменить элемент" },
    );
    await listQuery.refetch();
  }

  async function confirmDelete() {
    if (!pendingDelete) return;
    const response = await deleteMutation.mutateAsync({ id: pendingDelete.id });
    setMessage(
      response.ok
        ? { ok: true, text: `${pendingDelete.name} удалён из автозагрузки` }
        : { ok: false, text: response.error ?? "Не удалось удалить запись" },
    );
    setPendingDelete(null);
    await listQuery.refetch();
  }

  return (
    <div className="space-y-5">
      <SectionTitle
        title="Автозагрузка"
        subtitle={`${formatCount(enabledCount)} программ запускается при входе · реестр, папка Startup и планировщик`}
        action={
          <Button variant="secondary" onClick={() => void listQuery.refetch()} loading={listQuery.isFetching}>
            <RefreshIcon className="size-4" />
            Обновить
          </Button>
        }
      />

      {message ? (
        <Card
          className={`p-3.5 ${
            message.ok
              ? "border-emerald-400/25 bg-emerald-500/[0.07]"
              : "border-amber-400/25 bg-amber-400/[0.06]"
          }`}
        >
          <p
            className={`flex items-center gap-2 text-xs ${
              message.ok ? "text-emerald-100" : "text-amber-200"
            }`}
          >
            {message.ok ? <CheckIcon className="size-3.5" /> : <AlertIcon className="size-3.5" />}
            {message.text}
          </p>
        </Card>
      ) : null}

      <div className="flex flex-wrap items-center gap-2">
        <div className="relative min-w-0 flex-1">
          <SearchIcon className="pointer-events-none absolute left-3 top-1/2 size-4 -translate-y-1/2 text-slate-500" />
          <input
            value={search}
            onChange={(event) => setSearch(event.target.value)}
            placeholder="Поиск по имени, команде или издателю"
            className="w-full rounded-xl border border-white/10 bg-white/[0.04] py-2 pl-9 pr-3 text-sm text-slate-100 outline-none placeholder:text-slate-500 focus:border-emerald-400/40"
          />
        </div>
        <div className="flex overflow-hidden rounded-xl border border-white/10">
          {(
            [
              ["user", "Программы"],
              ["system", "Системные"],
              ["disabled", "Отключённые"],
              ["all", "Все"],
            ] as [Filter, string][]
          ).map(([key, label]) => (
            <button
              key={key}
              onClick={() => setFilter(key)}
              className={`px-2.5 py-2 text-[11px] transition-colors ${
                filter === key
                  ? "bg-white/[0.09] text-white"
                  : "bg-white/[0.03] text-slate-400 hover:bg-white/[0.06]"
              }`}
            >
              {label}
            </button>
          ))}
        </div>
      </div>

      {listQuery.isLoading ? (
        <div className="space-y-2">
          {[0, 1, 2, 3].map((index) => (
            <div key={index} className="h-14 animate-pulse rounded-xl bg-white/[0.04]" />
          ))}
        </div>
      ) : null}

      {!listQuery.isLoading && filtered.length === 0 ? (
        <EmptyState
          icon={<PackageIcon className="size-6" />}
          title="Записей не найдено"
          description="В этой категории нет элементов автозагрузки."
        />
      ) : null}

      <div className="space-y-2">
        {filtered.map((item) => (
          <Card
            key={item.id}
            className={`p-3 transition-colors ${
              item.enabled ? "" : "opacity-65"
            } ${!item.exists && item.executable ? "border-amber-400/25" : ""}`}
          >
            <div className="flex items-start gap-3">
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-1.5">
                  <p className="truncate text-[13px] font-medium text-slate-100">{item.name}</p>
                  <Badge variant={item.enabled ? "success" : "neutral"}>
                    {item.enabled ? "включено" : "отключено"}
                  </Badge>
                  <Badge variant="neutral">
                    {item.source === "registry"
                      ? "реестр"
                      : item.source === "startup-folder"
                        ? "папка"
                        : "планировщик"}
                  </Badge>
                  {!item.exists && item.executable ? (
                    <Badge variant="warn">
                      <AlertIcon className="size-3" /> файл не найден
                    </Badge>
                  ) : null}
                </div>
                <p className="mt-1 truncate font-mono text-[10px] text-slate-500" title={item.command}>
                  {item.command || "—"}
                </p>
                <p className="mt-0.5 text-[10px] text-slate-600">
                  {item.location}
                  {item.publisher ? ` · ${item.publisher}` : ""}
                </p>
              </div>

              <div className="flex shrink-0 items-center gap-1.5">
                <button
                  role="switch"
                  aria-checked={item.enabled}
                  onClick={() => void toggle(item)}
                  disabled={toggleMutation.isPending}
                  className={`relative h-6 w-11 shrink-0 rounded-full transition-colors disabled:opacity-40 ${
                    item.enabled ? "bg-emerald-500" : "bg-white/15"
                  }`}
                  title={item.enabled ? "Отключить" : "Включить"}
                >
                  <span
                    className={`absolute top-0.5 size-5 rounded-full bg-white transition-[left] ${
                      item.enabled ? "left-[22px]" : "left-0.5"
                    }`}
                  />
                </button>
                {item.executable ? (
                  <button
                    onClick={() => void rpcClient.system.reveal({ path: item.executable! })}
                    className="rounded-lg px-1.5 py-1 text-[10px] text-slate-500 hover:bg-white/10 hover:text-slate-200"
                    title="Показать в проводнике"
                  >
                    файл
                  </button>
                ) : null}
                <button
                  onClick={() => setPendingDelete(item)}
                  className="grid size-7 place-items-center rounded-lg text-slate-600 transition-colors hover:bg-rose-500/15 hover:text-rose-300"
                  title="Удалить из автозагрузки"
                >
                  <TrashIcon className="size-3.5" />
                </button>
              </div>
            </div>
          </Card>
        ))}
      </div>

      <Modal
        open={Boolean(pendingDelete)}
        onClose={() => setPendingDelete(null)}
        title="Удалить из автозагрузки?"
        description={
          pendingDelete ? (
            <>
              <b className="text-white">{pendingDelete.name}</b> больше не будет запускаться при
              входе в систему. Сама программа останется установленной — её можно запускать вручную.
            </>
          ) : null
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setPendingDelete(null)}>
              Отмена
            </Button>
            <Button variant="danger" onClick={confirmDelete} loading={deleteMutation.isPending}>
              <TrashIcon className="size-4" />
              Удалить запись
            </Button>
          </>
        }
      />
    </div>
  );
}
