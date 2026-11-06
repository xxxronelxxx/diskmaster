import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { SensorInfo, SensorReading, SensorsState } from "@/shared/types";
import { queryClient } from "@/client/rpc-client";
import { Badge, Button, Card, Modal } from "./ui";
import { AlertIcon, CheckIcon, CpuIcon, GaugeIcon, ShieldIcon } from "./icons";
import { t, translateError } from "@/client/i18n";

const HARDWARE_INDEX: Record<string, number> = { Cpu: 0, Gpu: 1, Storage: 2, Motherboard: 3, Memory: 4, Controller: 5 };

function readout(sensor: SensorReading): string {
  if (sensor.kind === "fan") return `${Math.round(sensor.value)} ${t("об/мин")}`;
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
      return t("Диск");
    case "Motherboard":
      return t("Плата");
    case "Memory":
      return t("Память");
    default:
      return type;
  }
}

function shortHardware(value: string): string {
  return value.replace(/\s+with\s+.*$/i, "");
}

export function SensorsPanel({ wmiSensors }: { wmiSensors: SensorInfo[] }) {
  const stateQuery = useQuery({
    ...queryClient.system.sensorsState.queryOptions(),
    refetchInterval: (query) => {
      const data = query.state.data as SensorsState | undefined;
      return data?.running ? 1000 : 2000;
    },
  });

  const state = stateQuery.data as SensorsState | undefined;
  const [installOpen, setInstallOpen] = useState(false);
  const [slowStart, setSlowStart] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const startMutation = useMutation(queryClient.system.sensorsStart.mutationOptions());
  const installMutation = useMutation(queryClient.system.sensorsInstall.mutationOptions());

  useEffect(() => {
    if (state?.running || !state?.pawnioInstalled || state?.error) return;
    const timer = window.setTimeout(() => setSlowStart(true), 10000);
    return () => window.clearTimeout(timer);
  }, [state?.running, state?.pawnioInstalled, state?.error]);

  const groups = useMemo(() => {
    const map = new Map<string, { hardware: string; type: string; sensors: SensorReading[] }>();
    for (const sensor of state?.sensors ?? []) {
      const key = `${sensor.hardwareType}|${sensor.hardware}`;
      const group = map.get(key) ?? {
        hardware: shortHardware(sensor.hardware),
        type: sensor.hardwareType,
        sensors: [],
      };
      group.sensors.push(sensor);
      map.set(key, group);
    }
    for (const group of map.values()) {
      group.sensors.sort((left, right) => {
        const rank = (name: string) => (/^Датчик/i.test(name) ? 1 : 0);
        return rank(left.name) - rank(right.name) || left.name.localeCompare(right.name, "ru");
      });
    }
    return [...map.values()].sort(
      (left, right) =>
        (HARDWARE_INDEX[left.type] ?? 99) - (HARDWARE_INDEX[right.type] ?? 99),
    );
  }, [state?.sensors]);

  const hasFans = (state?.sensors ?? []).some((sensor) => sensor.kind !== "temperature");
  const visibleError = error ?? state?.error ?? null;

  async function start() {
    setError(null);
    setNotice(null);
    setSlowStart(false);
    try {
      await startMutation.mutateAsync(undefined);
      await stateQuery.refetch();
    } catch (cause) {
      setError(translateError(cause));
    }
  }

  async function install() {
    setInstallOpen(false);
    setError(null);
    setNotice(null);
    setSlowStart(false);
    try {
      const result = await installMutation.mutateAsync(undefined);
      setNotice(
        result.needsReboot
          ? t("PawnIO установлен. Может потребоваться перезагрузка Windows.")
          : t("Датчики включены."),
      );
      await stateQuery.refetch();
    } catch (cause) {
      setError(translateError(cause));
    }
  }

  return (
    <Card className="p-4">
      <div className="mb-3 flex flex-wrap items-center gap-2">
        <GaugeIcon className="size-4 text-emerald-300" />
        <p className="text-sm font-medium text-white">{t("Температуры и кулеры")}</p>
      </div>

      {!state ? (
        <p className="text-xs text-slate-500">{t("Проверяем датчики…")}</p>
      ) : !state.available ? (
        <p className="flex items-center gap-2 rounded-lg border border-amber-400/25 bg-amber-400/[0.08] p-2.5 text-[11px] text-amber-100">
          <AlertIcon className="size-3.5" />
          {t("Модуль датчиков не найден. Пересоберите приложение (pnpm build:portable).")}
        </p>
      ) : !state.pawnioInstalled && !state.running ? (
        <div className="space-y-3">
          <p className="text-[11px] leading-relaxed text-slate-400">
            {t("Чтобы показывать температуру процессора и кулеры, нужен компонент PawnIO — подписанный драйвер из состава приложения. Установка требует прав администратора и выполняется только по кнопке.")}
          </p>
          <Button
            size="sm"
            variant="primary"
            onClick={() => setInstallOpen(true)}
            loading={installMutation.isPending}
          >
            <ShieldIcon className="size-3.5" />
            {t("Установить датчики")}
          </Button>
        </div>
      ) : !state.running ? (
        <div className="space-y-3">
          {visibleError ? (
            <div className="space-y-2">
              <p className="text-[11px] text-rose-300">{visibleError}</p>
              <Button size="sm" variant="secondary" onClick={() => void start()} loading={startMutation.isPending}>
                {t("Повторить")}
              </Button>
            </div>
          ) : slowStart ? (
            <div className="space-y-2">
              <p className="text-[11px] text-slate-400">{t("Датчики выключены.")}</p>
              <Button size="sm" variant="secondary" onClick={() => void start()} loading={startMutation.isPending}>
                {t("Запустить")}
              </Button>
            </div>
          ) : (
            <p className="flex items-center gap-2 text-[11px] text-slate-400">
              <span className="size-1.5 animate-pulse rounded-full bg-emerald-400" />
              {t("Запускаем датчики…")}
            </p>
          )}

          {wmiSensors.length > 0 ? (
            <div className="rounded-lg border border-white/[0.07] bg-white/[0.02] p-2.5">
              <p className="mb-1 text-[10px] uppercase tracking-wide text-slate-500">
                {t("Датчики LibreHardwareMonitor (WMI)")}
              </p>
              <div className="space-y-1">
                {wmiSensors.slice(0, 8).map((sensor, index) => (
                  <div key={`${sensor.identifier ?? sensor.name}-${index}`} className="flex justify-between gap-2 text-[11px]">
                    <span className="truncate text-slate-300">{sensor.name}</span>
                    <span className="shrink-0 tabular-nums text-slate-400">
                      {sensor.sensorType === "Fan"
                        ? `${Math.round(sensor.value)} ${t("об/мин")}`
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
              {t("Датчики запущены, но это оборудование не отдаёт температуры.")}
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
                        <span className="truncate text-slate-400">{t(sensor.name)}</span>
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
            <p className="text-[10px] text-slate-500">{t("Вентиляторы не найдены на этом устройстве.")}</p>
          ) : null}
        </div>
      )}

      {notice ? (
        <p className="mt-2 flex items-center gap-1.5 text-[11px] text-emerald-300">
          <CheckIcon className="size-3.5" /> {notice}
        </p>
      ) : null}

      <Modal
        open={installOpen}
        onClose={() => setInstallOpen(false)}
        title={t("Установить датчики?")}
        description={
          <>
            {t("Будет установлен подписанный драйвер")} <b className="text-white">PawnIO 2.2.0</b> {t("из состава приложения. Понадобятся права администратора; возможно, потребуется перезагрузка.")}
            <br />
            <br />
            {t("Драйвер даёт доступ к регистрам процессора и контроллеру ноутбука. Некоторые античиты могут реагировать на такие драйверы.")}
          </>
        }
        footer={
          <>
            <Button variant="ghost" onClick={() => setInstallOpen(false)}>
              {t("Отмена")}
            </Button>
            <Button variant="primary" onClick={() => void install()} loading={installMutation.isPending}>
              {t("Установить")}
            </Button>
          </>
        }
      />
    </Card>
  );
}
