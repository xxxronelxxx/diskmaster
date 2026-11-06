import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { MetricsState, ProcessInfo } from "@/shared/types";
import { queryClient } from "@/client/rpc-client";
import { formatBytes, formatCount } from "@/client/lib/format";
import { Badge, Button, Card, Modal, Progress, SectionTitle } from "./ui";
import { Sparkline, BarMeter } from "./sparkline";
import { AlertIcon, CpuIcon, MemoryIcon, PauseIcon, PlayIcon, RefreshIcon, SettingsIcon, SpinnerIcon, TrashIcon } from "./icons";

type MetricsPayload = MetricsState & { processes: ProcessInfo[] };

const PRIORITIES = [
  ["normal", "Обычный"],
  ["below", "Ниже обычного"],
  ["above", "Выше обычного"],
  ["high", "Высокий"],
  ["idle", "Простой"],
] as const;

function percentTone(value: number): "emerald" | "amber" | "rose" {
  if (value >= 85) return "rose";
  if (value >= 60) return "amber";
  return "emerald";
}

function MetricCard({
  label,
  value,
  hint,
  values,
  max,
  tone,
  footer,
}: {
  label: string;
  value: string;
  hint?: string;
  values: number[];
  max?: number;
  tone: string;
  footer?: React.ReactNode;
}) {
  return (
    <Card className="p-4">
      <div className="flex items-baseline justify-between gap-2">
        <p className="text-[11px] uppercase tracking-wide text-slate-500">{label}</p>
        {hint ? <span className="text-[10px] text-slate-600">{hint}</span> : null}
      </div>
      <p className="mt-1 text-xl font-semibold tabular-nums tracking-tight text-white">{value}</p>
      <div className="mt-2">
        <Sparkline values={values} max={max} tone={tone} height={46} />
      </div>
      {footer ? <div className="mt-2">{footer}</div> : null}
    </Card>
  );
}

export function MonitorTab() {
  const [showProcesses, setShowProcesses] = useState(true);
  const [pendingKill, setPendingKill] = useState<ProcessInfo | null>(null);
  const [managing, setManaging] = useState<ProcessInfo | null>(null);
  const [suspendState, setSuspendState] = useState<{ pid: number; suspended: boolean } | null>(null);

  const metricsQuery = useQuery({
    ...queryClient.system.metrics.queryOptions({ input: { processes: true } }),
    refetchInterval: 1500,
    refetchIntervalInBackground: false,
    staleTime: 0,
    gcTime: 30_000,
  });

  const killMutation = useMutation(queryClient.system.killProcess.mutationOptions());
  const priorityMutation = useMutation(queryClient.system.processPriority.mutationOptions());
  const suspendMutation = useMutation(queryClient.system.processSuspend.mutationOptions());

  const data = metricsQuery.data as MetricsPayload | undefined;
  const history = data?.history ?? [];
  const latest = data?.latest;
  const managed = managing
    ? (data?.processes ?? []).find((process) => process.pid === managing.pid) ?? managing
    : null;
  const suspended =
    suspendState && managing && suspendState.pid === managing.pid
      ? suspendState.suspended
      : Boolean(managed?.suspended);

  const cpuSeries = history.map((sample) => sample.cpu);
  const ramSeries = history.map((sample) =>
    sample.ramTotal > 0 ? (sample.ramUsed / sample.ramTotal) * 100 : 0,
  );
  const diskSeries = history.map((sample) => sample.diskRead + sample.diskWrite);
  const netSeries = history.map((sample) => sample.netIn + sample.netOut);

  const ramPercent = latest && latest.ramTotal > 0 ? (latest.ramUsed / latest.ramTotal) * 100 : 0;
  const diskPeak = Math.max(1024 * 1024, ...diskSeries);
  const netPeak = Math.max(1024 * 256, ...netSeries);

  function openManage(process: ProcessInfo) {
    priorityMutation.reset();
    suspendMutation.reset();
    setSuspendState({ pid: process.pid, suspended: process.suspended });
    setManaging(process);
  }

  async function applyPriority(level: string) {
    if (!managing) return;
    await priorityMutation.mutateAsync({ pid: managing.pid, level });
    void metricsQuery.refetch();
  }

  async function toggleSuspend() {
    if (!managed) return;
    const next = !suspended;
    const result = await suspendMutation.mutateAsync({ pid: managed.pid, suspend: next });
    if (result.ok) setSuspendState({ pid: managed.pid, suspended: next });
    void metricsQuery.refetch();
  }

  async function confirmKill() {
    if (!pendingKill) return;
    await killMutation.mutateAsync({ pid: pendingKill.pid });
    setPendingKill(null);
    void metricsQuery.refetch();
  }

  return (
    <div className="space-y-5">
      <SectionTitle
        title="Мониторинг"
        subtitle={
          latest
            ? `Обновление каждую секунду · ${history.length} точек истории · адаптер ${latest.netAdapter ?? "—"}`
            : "Собираем данные о нагрузке…"
        }
        action={
          <div className="flex gap-2">
            <Button
              variant="secondary"
              size="md"
              onClick={() => void metricsQuery.refetch()}
              loading={metricsQuery.isFetching && !latest}
            >
              <RefreshIcon className="size-4" />
              Обновить
            </Button>
          </div>
        }
      />

      {data?.error ? (
        <Card className="border-amber-400/25 bg-amber-400/[0.06] p-3.5">
          <p className="flex items-center gap-2 text-xs text-amber-200">
            <AlertIcon className="size-3.5" /> {data.error}
          </p>
        </Card>
      ) : null}

      {!latest ? (
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          {[0, 1, 2, 3].map((index) => (
            <div key={index} className="h-36 animate-pulse rounded-2xl bg-white/[0.04]" />
          ))}
        </div>
      ) : (
        <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
          <MetricCard
            label="Процессор"
            value={`${latest.cpu.toFixed(0)}%`}
            hint={`${latest.cores.length} ядер`}
            values={cpuSeries}
            max={100}
            tone={percentTone(latest.cpu)}
            footer={<BarMeter value={latest.cpu} tone={percentTone(latest.cpu)} />}
          />
          <MetricCard
            label="Память"
            value={`${formatBytes(latest.ramUsed, 1)}`}
            hint={`из ${formatBytes(latest.ramTotal, 0)}`}
            values={ramSeries}
            max={100}
            tone={percentTone(ramPercent)}
            footer={
              <div className="flex items-center justify-between text-[11px] text-slate-400">
                <span>{ramPercent.toFixed(0)}% занято</span>
                <span className="text-slate-500">
                  свободно {formatBytes(latest.ramTotal - latest.ramUsed, 1)}
                </span>
              </div>
            }
          />
          <MetricCard
            label="Диск"
            value={`${formatBytes(latest.diskRead + latest.diskWrite, 1)}/с`}
            hint="чтение + запись"
            values={diskSeries}
            max={diskPeak}
            tone="sky"
            footer={
              <div className="flex justify-between text-[11px] text-slate-400">
                <span>чтение {formatBytes(latest.diskRead, 1)}/с</span>
                <span>запись {formatBytes(latest.diskWrite, 1)}/с</span>
              </div>
            }
          />
          <MetricCard
            label="Сеть"
            value={`${formatBytes(latest.netIn + latest.netOut, 1)}/с`}
            hint={latest.netAdapter ? latest.netAdapter.slice(0, 22) : "адаптер не найден"}
            values={netSeries}
            max={netPeak}
            tone="violet"
            footer={
              <div className="flex justify-between text-[11px] text-slate-400">
                <span>↓ {formatBytes(latest.netIn, 1)}/с</span>
                <span>↑ {formatBytes(latest.netOut, 1)}/с</span>
              </div>
            }
          />
        </div>
      )}

      {latest && latest.cores.length > 0 ? (
        <Card className="p-4">
          <div className="mb-3 flex items-center gap-2">
            <CpuIcon className="size-4 text-sky-300" />
            <h3 className="text-sm font-medium text-white">Ядра процессора</h3>
            <span className="ml-auto text-[11px] text-slate-500">мгновенная загрузка</span>
          </div>
          <div className="grid grid-cols-4 gap-2 sm:grid-cols-8 xl:grid-cols-16">
            {latest.cores.map((core, index) => (
              <div key={index} className="rounded-lg border border-white/[0.06] bg-white/[0.02] p-1.5">
                <div className="flex h-12 items-end overflow-hidden rounded bg-white/[0.05]">
                  <div
                    className="w-full rounded-t bg-gradient-to-t from-emerald-500/60 to-emerald-300 transition-[height] duration-300"
                    style={{ height: `${Math.max(2, core)}%` }}
                  />
                </div>
                <p className="mt-1 text-center text-[10px] tabular-nums text-slate-400">{core}%</p>
              </div>
            ))}
          </div>
        </Card>
      ) : null}

      <Card className="overflow-hidden">
        <div className="flex items-center justify-between border-b border-white/[0.08] px-4 py-3">
          <div className="flex items-center gap-2">
            <MemoryIcon className="size-4 text-violet-300" />
            <h3 className="text-sm font-medium text-white">Процессы по памяти</h3>
            <Badge variant="neutral">топ 12</Badge>
          </div>
          <button
            onClick={() => setShowProcesses((current) => !current)}
            className="text-[11px] text-slate-400 hover:text-white"
          >
            {showProcesses ? "скрыть" : "показать"}
          </button>
        </div>

        {showProcesses ? (
          metricsQuery.isLoading ? (
            <div className="flex items-center gap-2 p-4 text-xs text-slate-400">
              <SpinnerIcon className="size-3.5" /> Читаем список процессов…
            </div>
          ) : (
            <div className="divide-y divide-white/[0.06]">
              {(data?.processes ?? []).map((process) => {
                const share = latest?.ramTotal ? (process.memory / latest.ramTotal) * 100 : 0;
                return (
                  <div key={process.pid} className="flex items-center gap-3 px-4 py-2.5">
                    <div className="min-w-0 flex-1">
                      <p className="flex items-center gap-2 truncate text-sm text-slate-100">
                        {process.name}
                        {process.suspended ? <Badge variant="warn">пауза</Badge> : null}
                      </p>
                      <div className="mt-1 flex items-center gap-2">
                        <div className="w-24">
                          <Progress value={share * 3} tone="violet" className="h-1" />
                        </div>
                        <span className="font-mono text-[10px] text-slate-600">pid {process.pid}</span>
                      </div>
                    </div>
                    <div className="shrink-0 text-right">
                      <p className="text-sm font-semibold tabular-nums text-white">
                        {formatBytes(process.memory, 0)}
                      </p>
                      <p className="text-[10px] text-slate-500">{process.cpuPercent.toFixed(1).replace(".", ",")}% CPU</p>
                    </div>
                    <button
                      onClick={() => openManage(process)}
                      aria-label={`Управление процессом ${process.name}`}
                      className="grid size-7 shrink-0 place-items-center rounded-lg text-slate-500 transition-colors hover:bg-white/10 hover:text-slate-200"
                      title="Приоритет и приостановка"
                    >
                      <SettingsIcon className="size-3.5" />
                    </button>
                    <button
                      onClick={() => setPendingKill(process)}
                      aria-label={`Завершить процесс ${process.name}`}
                      className="grid size-7 shrink-0 place-items-center rounded-lg text-slate-500 transition-colors hover:bg-rose-500/15 hover:text-rose-300"
                      title="Завершить процесс"
                    >
                      <TrashIcon className="size-3.5" />
                    </button>
                  </div>
                );
              })}
            </div>
          )
        ) : null}
      </Card>

      <Modal
        open={Boolean(managing)}
        onClose={() => setManaging(null)}
        title="Управление процессом"
        description={
          managed ? (
            <>
              <b className="text-white">{managed.name}</b> · pid {managed.pid}
              {managed.path ? <span className="mt-1 block truncate font-mono text-[10px] text-slate-500">{managed.path}</span> : null}
            </>
          ) : null
        }
        footer={
          <Button variant="ghost" onClick={() => setManaging(null)}>
            Закрыть
          </Button>
        }
      >
        {managed ? (
          <div className="space-y-4">
            <div className="flex items-center justify-between gap-3 rounded-xl border border-white/[0.08] bg-white/[0.02] p-3">
              <div>
                <p className="text-xs text-slate-200">
                  {suspended ? "Процесс приостановлен" : "Процесс работает"}
                </p>
                <p className="mt-0.5 text-[10px] text-slate-500">
                  Приостановка замораживает все потоки и освобождает процессор. Процесс останется
                  замороженным, пока вы не нажмёте «Продолжить», даже после закрытия программы.
                </p>
              </div>
              <Button
                size="sm"
                variant={suspended ? "success" : "secondary"}
                loading={suspendMutation.isPending}
                onClick={() => void toggleSuspend()}
              >
                {suspended ? <PlayIcon className="size-3.5" /> : <PauseIcon className="size-3.5" />}
                {suspended ? "Продолжить" : "Приостановить"}
              </Button>
            </div>

            <div>
              <p className="mb-2 text-[11px] uppercase tracking-wide text-slate-500">Приоритет</p>
              <div className="grid grid-cols-2 gap-2">
                {PRIORITIES.map(([level, label]) => (
                  <Button
                    key={level}
                    size="sm"
                    variant="secondary"
                    loading={priorityMutation.isPending}
                    onClick={() => void applyPriority(level)}
                  >
                    {label}
                  </Button>
                ))}
              </div>
              <p className="mt-2 text-[10px] text-slate-600">
                Высокий приоритет может замедлить систему. Системные процессы защищены.
              </p>
            </div>

            {priorityMutation.data && !priorityMutation.data.ok ? (
              <p className="rounded-lg border border-rose-400/30 bg-rose-500/10 p-2.5 text-[11px] text-rose-200">
                {priorityMutation.data.error}
              </p>
            ) : null}
            {suspendMutation.data && !suspendMutation.data.ok ? (
              <p className="rounded-lg border border-rose-400/30 bg-rose-500/10 p-2.5 text-[11px] text-rose-200">
                {suspendMutation.data.error}
              </p>
            ) : null}
            {priorityMutation.data?.ok ? (
              <p className="text-[11px] text-emerald-300">Приоритет изменён.</p>
            ) : null}
            {suspendMutation.data?.ok ? (
              <p className="text-[11px] text-emerald-300">
                {suspended ? "Процесс приостановлен." : "Процесс продолжает работу."}
              </p>
            ) : null}
          </div>
        ) : null}
      </Modal>

      <Modal
        open={Boolean(pendingKill)}
        onClose={() => setPendingKill(null)}
        title="Завершить процесс?"
        description={
          pendingKill ? (
            <>
              Процесс <b className="text-white">{pendingKill.name}</b> (pid {pendingKill.pid}) будет
              остановлен принудительно. Несохранённые данные в программе могут быть потеряны.
            </>
          ) : null
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setPendingKill(null)}>
              Отмена
            </Button>
            <Button variant="danger" onClick={confirmKill} loading={killMutation.isPending}>
              <TrashIcon className="size-4" />
              Завершить
            </Button>
          </>
        }
      >
        {killMutation.data && !killMutation.data.ok ? (
          <p className="rounded-lg border border-rose-400/30 bg-rose-500/10 p-2.5 text-[11px] text-rose-200">
            {killMutation.data.error}
          </p>
        ) : null}
      </Modal>
    </div>
  );
}
