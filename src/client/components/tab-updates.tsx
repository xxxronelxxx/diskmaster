import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { queryClient } from "@/client/rpc-client";
import { formatCount } from "@/client/lib/format";
import { Badge, Button, Card, Modal, Progress, SectionTitle } from "./ui";
import {
  AlertIcon,
  ArrowRightIcon,
  CheckIcon,
  DownloadIcon,
  RefreshIcon,
  SpinnerIcon,
} from "./icons";
import { t } from "@/client/i18n";

type RunResult = { id: string; name: string; ok: boolean; output: string };

export function UpdatesTab() {
  const [running, setRunning] = useState<string | null>(null);
  const [confirmAll, setConfirmAll] = useState(false);
  const [progress, setProgress] = useState<{ done: number; total: number } | null>(null);
  const [results, setResults] = useState<RunResult[]>([]);
  const [upgraded, setUpgraded] = useState<Set<string>>(new Set());

  const updatesQuery = useQuery({
    ...queryClient.system.updates.queryOptions({ input: {} }),
    staleTime: 120_000,
  });

  const upgradeMutation = useMutation(queryClient.system.upgrade.mutationOptions());

  const listing = updatesQuery.data;
  const updates = (listing?.updates ?? []).filter((update) => !upgraded.has(update.id));
  const busy = Boolean(progress);

  async function runUpgrade(ids: string[]) {
    if (ids.length === 0) return;
    setConfirmAll(false);
    setProgress({ done: 0, total: ids.length });
    setResults([]);

    for (const id of ids) {
      setRunning(id);
      const name = listing?.updates.find((update) => update.id === id)?.name ?? id;
      try {
        const response = await upgradeMutation.mutateAsync({ id });
        setResults((current) => [...current, { id, name, ok: response.ok, output: response.output }]);
        if (response.ok) {
          setUpgraded((current) => new Set(current).add(id));
        }
      } catch (error) {
        setResults((current) => [
          ...current,
          { id, name, ok: false, output: String(error) },
        ]);
      }
      setProgress((current) => ({ done: (current?.done ?? 0) + 1, total: ids.length }));
    }

    setRunning(null);
    setProgress(null);
    await updatesQuery.refetch();
  }

  const failed = results.filter((result) => !result.ok).length;
  const succeeded = results.filter((result) => result.ok).length;

  return (
    <div className="space-y-5">
      <SectionTitle
        title={t("Обновления программ")}
        subtitle={
          listing?.available
            ? t("{count} пакетов можно обновить через winget", { count: formatCount(updates.length) })
            : t("Обновление установленных приложений через диспетчер пакетов Windows")
        }
        action={
          <div className="flex gap-2">
            <Button
              variant="secondary"
              onClick={() => void updatesQuery.refetch()}
              loading={updatesQuery.isFetching && !busy}
            >
              <RefreshIcon className="size-4" />
              {t("Проверить")}
            </Button>
            <Button
              variant="primary"
              disabled={updates.length === 0 || busy || !listing?.available}
              onClick={() => setConfirmAll(true)}
            >
              <DownloadIcon className="size-4" />
              {t("Обновить всё")}
            </Button>
          </div>
        }
      />

      {progress ? (
        <Card className="p-4">
          <div className="flex items-center justify-between text-sm">
            <span className="flex items-center gap-2 text-slate-100">
              <SpinnerIcon className="size-4 text-sky-300" />
              {t("Обновление")} {progress.done + 1} {t("из")} {progress.total}…
            </span>
            <span className="tabular-nums text-slate-400">
              {Math.round((progress.done / progress.total) * 100)}%
            </span>
          </div>
          <Progress
            className="mt-3"
            value={(progress.done / progress.total) * 100}
            tone="sky"
          />
          <p className="mt-2 truncate text-[11px] text-slate-500">
            {running ? listing?.updates.find((update) => update.id === running)?.name ?? running : ""}
          </p>
        </Card>
      ) : null}

      {results.length > 0 && !progress ? (
        <Card className="p-4">
          <p className="text-sm font-medium text-slate-100">
            {t("Готово: обновлено")} {succeeded}
            {failed > 0 ? t(", с ошибкой {count}", { count: failed }) : ""}
          </p>
          <div className="mt-2 space-y-1.5">
            {results.map((result) => (
              <div key={result.id} className="text-[11px]">
                <span className={result.ok ? "text-emerald-300" : "text-rose-300"}>
                  {result.ok ? "✓" : "✕"} {result.name}
                </span>
                {!result.ok && result.output ? (
                  <details className="mt-0.5">
                    <summary className="cursor-pointer text-slate-500">{t("журнал")}</summary>
                    <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap rounded-lg bg-black/40 p-2 font-mono text-[10px] text-slate-400">
                      {result.output}
                    </pre>
                  </details>
                ) : null}
              </div>
            ))}
          </div>
        </Card>
      ) : null}

      {updatesQuery.isLoading ? (
        <Card className="p-6">
          <div className="flex items-center gap-2 text-sm text-slate-300">
            <SpinnerIcon className="size-4 text-sky-300" />
            {t("Опрашиваем winget — это может занять до минуты…")}
          </div>
          <div className="mt-3 space-y-2">
            {[0, 1, 2].map((index) => (
              <div key={index} className="h-10 animate-pulse rounded-xl bg-white/[0.04]" />
            ))}
          </div>
        </Card>
      ) : null}

      {listing && !listing.available ? (
        <Card className="border-amber-400/25 bg-amber-400/[0.06] p-4">
          <p className="flex items-center gap-2 text-sm font-medium text-amber-100">
            <AlertIcon className="size-4" /> {t("winget недоступен")}
          </p>
          <p className="mt-1 text-xs leading-relaxed text-amber-200/80">
            {listing.error ??
              t("Установите «Установщик приложений» (App Installer) из Microsoft Store, чтобы обновлять программы.")}
          </p>
          <p className="mt-2 text-[11px] text-amber-200/60">
            {t("Проверка выполняется командой winget upgrade. Если приложение установлено, но winget не найден — возможно, оно отключено в политиках системы.")}
          </p>
        </Card>
      ) : null}

      {listing?.available && updates.length === 0 && !updatesQuery.isLoading ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.06] p-6 text-center">
          <CheckIcon className="mx-auto size-7 text-emerald-300" />
          <p className="mt-2 text-sm text-emerald-100">{t("Все программы обновлены")}</p>
          <p className="mt-1 text-xs text-emerald-200/60">
            {t("Пакетов с доступными обновлениями не найдено.")}
          </p>
        </Card>
      ) : null}

      {updates.length > 0 ? (
        <Card className="overflow-hidden">
          <div className="divide-y divide-white/[0.06]">
            {updates.map((update) => (
              <div key={update.id} className="flex items-center gap-3 px-4 py-3">
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm text-slate-100" title={update.name}>
                    {update.name}
                  </p>
                  <p className="truncate font-mono text-[11px] text-slate-500">{update.id}</p>
                </div>
                <div className="hidden items-center gap-1.5 text-xs tabular-nums text-slate-400 sm:flex">
                  <span>{update.current}</span>
                  <ArrowRightIcon className="size-3 text-slate-600" />
                  <span className="text-emerald-300">{update.available}</span>
                </div>
                <Badge variant="neutral" className="hidden sm:inline-flex">
                  {update.source}
                </Badge>
                <Button
                  size="sm"
                  variant="secondary"
                  disabled={busy}
                  loading={running === update.id}
                  onClick={() => void runUpgrade([update.id])}
                >
                  {t("Обновить")}
                </Button>
              </div>
            ))}
          </div>
        </Card>
      ) : null}

      <Modal
        open={confirmAll}
        onClose={() => setConfirmAll(false)}
        title={t("Обновить все программы?")}
        description={
          <>
            {t("Будет последовательно обновлено")} <b className="text-white">{updates.length}</b> {t("пакетов через winget в тихом режиме. Это может занять 10–30 минут, в зависимости от размера загрузок.")}
          </>
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setConfirmAll(false)}>
              {t("Отмена")}
            </Button>
            <Button variant="primary" onClick={() => void runUpgrade(updates.map((update) => update.id))}>
              <DownloadIcon className="size-4" />
              {t("Обновить всё")}
            </Button>
          </>
        }
      />
    </div>
  );
}
