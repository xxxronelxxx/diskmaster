import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { SensorInfo, SensorReading, SensorsState } from "@/shared/types";
import { queryClient } from "@/client/rpc-client";
import { Badge, Button, Card, Modal } from "./ui";
import { AlertIcon, CheckIcon, CpuIcon, GaugeIcon, RefreshIcon, ShieldIcon } from "./icons";

const HARDWARE_ORDER = ["Cpu", "Gpu", "Storage", "Motherboard", "Memory", "Controller"];

function readout(sensor: SensorReading): string {
  if (sensor.kind === "fan") return `${Math.round(sensor.value)} об/мин`;
  if (sensor.kind === "percent") return `${Math.round(sensor.value)}%`;
  return `${sensor.value.toFixed(1)} °C`;
}

function hardwareTone(type: string): "info" | "success" | "warn" | "neutral" {
  if (type === "Cpu") return "info";
  if (type === "Gpu") return "success";
  if (type === "Storage") return "warn";
  return "neutral";
}

function hardwareLabel(type: string): string {
  switch (type) {
    case "Cpu":
      return "CPU";
    case "Gpu":
      return "GPU";
    case "Storage":
      return "Диск";
    case "Motherboard":
      return "Плата";
    case "Memory":
      return "Память";
    default:
      return type;
  }
}

export function SensorsPanel({ wmiSensors }: { wmiSensors: SensorInfo[] }) {
  const stateQuery = useQuery({
    ...queryClient.system.sensorsState.queryOptions(),
    refetchInterval: (query) => {
      const data = query.state.data as SensorsState | undefined;
      return data?.running ? 5000 : false;
    },
  });

  const state = stateQuery.data as SensorsState | undefined;
  const [installOpen, setInstallOpen] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const startMutation = useMutation(queryClient.system.sensorsStart.mutationOptions());
  const stopMutation = useMutation(queryClient.system.sensorsStop.mutationOptions());
  const installMutation = useMutation(queryClient.system.sensorsInstall.mutationOptions());

  const groups = useMemo(() => {
    const map = new Map<string, { hardware: string; type: string; sensors: SensorReading[] }>();
    for (const sensor of state?.sensors ?? []) {
      const key = `${sensor.hardwareType}|${sensor.hardware}`;
      const group = map.get(key) ?? {
        hardware: sensor.hardware,
        type: sensor.hardwareType,
        sensors: [],
      };
      group.sensors.push(sensor);
      map.set(key, group);
    }
    return [...map.values()].sort(
      (left, right) =>
        (HARDWARE_ORDER.indexOf(left.type) + 1 || 99) -
        (HARDWARE_ORDER.indexOf(right.type) + 1 || 99),
    );
  }, [state?.sensors]);

  const hasFans = (state?.sensors ?? []).some((sensor) => sensor.kind !== "temperature");

  async function start() {
    setError(null);
    setNotice(null);
    try {
      await startMutation.mutateAsync(undefined);
      await stateQuery.refetch();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function install() {
    setInstallOpen(false);
    setError(null);
    setNotice(null);
    try {
      const result = await installMutation.mutateAsync(undefined);
      setNotice(
        result.needsReboot
          ? "PawnIO установлен. Может потребоваться перезагрузка Windows."
          : "Датчики включены.",
      );
      await stateQuery.refetch();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  async function stop() {
    setError(null);
    setNotice(null);
    try {
      await stopMutation.mutateAsync(undefined);
      setNotice("Датчики остановлены.");
      await stateQuery.refetch();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    }
  }

  return (
    <Card className="p-4">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <GaugeIcon className="size-4 text-emerald-300" />
        <p className="text-sm font-medium text-white">Температуры и кулеры</p>
        {state?.running ? (
          <Badge variant="success">
            {state.pawnioInstalled ? `PawnIO ${state.pawnioVersion ?? ""}`.trim() : "без драйвера"}
          </Badge>
        ) : (
          <Badge variant="neutral">выключено</Badge>
        )}
        <button
          onClick={() => void stateQuery.refetch()}
          className="ml-auto grid size-7 place-items-center rounded-lg text-slate-500 hover:bg-white/10 hover:text-slate-200"
          title="Обновить датчики"
        >
          <RefreshIcon className={`size-3.5 ${stateQuery.isFetching ? "animate-spin" : ""}`} />
        </button>
      </div>

      {!state ? (
        <p className="text-xs text-slate-500">Проверяем модуль датчиков…</p>
      ) : !state.available ? (
        <p className="flex items-center gap-2 rounded-lg border border-amber-400/25 bg-amber-400/[0.08] p-2.5 text-[11px] text-amber-100">
          <AlertIcon className="size-3.5" />
          Модуль датчиков не найден. Пересоберите приложение (pnpm build:portable).
        </p>
      ) : !state.running ? (
        <div className="space-y-3">
          <p className="text-[11px] leading-relaxed text-slate-400">
            {state.pawnioInstalled ? (
              <>
                PawnIO {state.pawnioVersion ?? ""} установлен. Включите датчики — понадобится
                подтверждение администратора.
              </>
            ) : (
              <>
                Для температуры CPU и кулеров нужен компонент PawnIO (подписанный драйвер). Он не
                установлен. Установка выполняется только по кнопке и требует прав администратора.
              </>
            )}
          </p>

          <div className="flex flex-wrap gap-2">
            {!state.pawnioInstalled ? (
              <Button size="sm" variant="primary" onClick={() => setInstallOpen(true)} loading={installMutation.isPending}>
                <ShieldIcon className="size-3.5" />
                Установить и включить
              </Button>
            ) : (
              <Button size="sm" variant="primary" onClick={() => void start()} loading={startMutation.isPending}>
                Включить датчики
              </Button>
            )}
            <Button size="sm" variant="ghost" onClick={() => void stateQuery.refetch()}>
              Проверить снова
            </Button>
          </div>

          {wmiSensors.length > 0 ? (
            <div className="rounded-lg border border-white/[0.07] bg-white/[0.02] p-2.5">
              <p className="mb-1 text-[10px] uppercase tracking-wide text-slate-500">
                Найдены датчики LibreHardwareMonitor (WMI)
              </p>
              <div className="space-y-1">
                {wmiSensors.slice(0, 8).map((sensor, index) => (
                  <div key={`${sensor.identifier ?? sensor.name}-${index}`} className="flex justify-between gap-2 text-[11px]">
                    <span className="truncate text-slate-300">{sensor.name}</span>
                    <span className="shrink-0 tabular-nums text-slate-400">
                      {sensor.sensorType === "Fan"
                        ? `${Math.round(sensor.value)} об/мин`
                        : `${sensor.value.toFixed(1)} °C`}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ) : null}
        </div>
      ) : (
        <div className="space-y-3">
          {groups.length === 0 ? (
            <p className="rounded-lg border border-white/[0.07] bg-white/[0.02] p-2.5 text-[11px] text-slate-400">
              Датчики запущены, но это оборудование не отдаёт температуры. Обычно так бывает в
              виртуальной машине или на ноутбуке с закрытым EC.
            </p>
          ) : (
            <div className="space-y-3">
              {groups.map((group) => (
                <div key={`${group.type}-${group.hardware}`} className="rounded-xl border border-white/[0.07] bg-white/[0.02] p-2.5">
                  <div className="mb-1.5 flex items-center gap-2">
                    <CpuIcon className="size-3.5 text-slate-400" />
                    <p className="truncate text-[11px] font-medium text-slate-200">{group.hardware}</p>
                    <Badge variant={hardwareTone(group.type)} className="ml-auto">
                      {hardwareLabel(group.type)}
                    </Badge>
                  </div>
                  <div className="space-y-1">
                    {group.sensors.map((sensor) => (
                      <div key={sensor.identifier} className="flex items-center justify-between gap-2 text-[11px]">
                        <span className="truncate text-slate-400">{sensor.name}</span>
                        <span className="shrink-0 font-medium tabular-nums text-slate-200">
                          {readout(sensor)}
                        </span>
                      </div>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}

          {!hasFans ? (
            <p className="text-[10px] text-slate-500">
              Датчики вентиляторов не найдены — на этом ноутбуке EC не отдаёт обороты.
            </p>
          ) : null}

          <div className="flex flex-wrap items-center justify-between gap-2 text-[10px] text-slate-500">
            <span>
              {state.ageSeconds === null ? "" : `обновлено ${state.ageSeconds} с назад`}
              {state.admin ? " · администратор" : ""}
            </span>
            <Button size="sm" variant="ghost" onClick={() => void stop()} loading={stopMutation.isPending}>
              Остановить
            </Button>
          </div>
        </div>
      )}

      {notice ? (
        <p className="mt-2 flex items-center gap-1.5 text-[11px] text-emerald-300">
          <CheckIcon className="size-3.5" /> {notice}
        </p>
      ) : null}
      {error ? (
        <p className="mt-2 flex items-center gap-1.5 text-[11px] text-rose-300">
          <AlertIcon className="size-3.5" /> {error}
        </p>
      ) : null}

      <Modal
        open={installOpen}
        onClose={() => setInstallOpen(false)}
        title="Установить компонент датчиков?"
        description={
          <>
            Будет установлен подписанный драйвер <b className="text-white">PawnIO 2.2.0</b> из состава
            приложения. Понадобятся права администратора; возможно, потребуется перезагрузка.
            <br />
            <br />
            Драйвер даёт доступ к регистрам процессора и контроллеру ноутбука. Некоторые античиты
            могут реагировать на такие драйверы. Если не хотите устанавливать его — оставьте датчики
            выключенными, остальные функции продолжат работать.
          </>
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setInstallOpen(false)}>
              Отмена
            </Button>
            <Button variant="primary" onClick={() => void install()} loading={installMutation.isPending}>
              Установить
            </Button>
          </>
        }
      />
    </Card>
  );
}