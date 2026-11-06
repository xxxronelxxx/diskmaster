import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api } from "@/client/api";
import { queryClient } from "@/client/rpc-client";
import type { MaintenanceStatus, SmartDisk, SmartWarning } from "@/shared/types";
import { formatBytes, formatCount, formatDate } from "@/client/lib/format";
import { Badge, Button, Card, EmptyState, Modal, SectionTitle } from "./ui";
import { SensorsPanel } from "./sensors-panel";
import { AlertIcon, RefreshIcon, ShieldIcon, TrashIcon } from "./icons";
import { t, translateError } from "@/client/i18n";

type Pending = {
  op: string;
  payload?: unknown;
  title: string;
  description: string;
  danger?: boolean;
};

const ADMIN_CATEGORIES = [
  ["win-temp", "Temp Windows"],
  ["win-update", "Кэш обновлений"],
  ["delivery-opt", "Оптимизация доставки"],
  ["prefetch", "Prefetch"],
  ["win-logs", "Логи Windows"],
] as const;

function healthLabel(value: number | null): { text: string; variant: "success" | "warn" | "danger" | "neutral" } {
  if (value === 0) return { text: t("Здоров"), variant: "success" };
  if (value === 1) return { text: t("Предупреждение"), variant: "warn" };
  if (value === 2) return { text: t("Неисправен"), variant: "danger" };
  return { text: t("Неизвестно"), variant: "neutral" };
}

function mediaLabel(value: number | null): string {
  if (value === 3) return "HDD";
  if (value === 4) return "SSD";
  if (value === 5) return "SCM";
  return "—";
}

const SMART_WARNING_LABELS: Record<SmartWarning, string> = {
  availableSpare: "резерв ниже порога",
  temperature: "температура вне нормы",
  reliability: "надёжность снижена",
  readOnly: "диск только для чтения",
  volatileMemory: "сбой резервного питания",
  persistentMemory: "энергонезависимая память только для чтения",
};

function SmartWarningChips({ warnings, critical }: { warnings: SmartWarning[]; critical: number }) {
  if (!warnings.length) {
    return critical > 0 ? (
      <p className="mt-1 text-[10px] text-rose-300">
        {t("SMART: критическое предупреждение (0x{code})", { code: critical.toString(16) })}
      </p>
    ) : null;
  }
  return (
    <div className="mt-1 flex flex-wrap gap-1">
      {warnings.map((warning) => (
        <span
          key={warning}
          className="rounded-full border border-rose-400/30 bg-rose-500/10 px-1.5 py-0.5 text-[10px] text-rose-200"
        >
          {t(SMART_WARNING_LABELS[warning])}
        </span>
      ))}
    </div>
  );
}

function SmartDetails({
  source,
  availableSpare,
  spareThreshold,
  unsafeShutdowns,
  mediaErrors,
  errorLogEntries,
  dataRead,
  dataWritten,
}: {
  source: string;
  availableSpare: number | null;
  spareThreshold: number | null;
  unsafeShutdowns: number | null;
  mediaErrors: number | null;
  errorLogEntries: number | null;
  dataRead: number | null;
  dataWritten: number | null;
}) {
  const rows: Array<[string, string]> = [
    [t("источник"), source],
    [t("резерв"), availableSpare === null ? "—" : `${availableSpare}%`],
    [t("порог резерва"), spareThreshold === null ? "—" : `${spareThreshold}%`],
    [t("некорректных выключений"), unsafeShutdowns === null ? "—" : formatCount(unsafeShutdowns)],
    [t("ошибок носителя"), mediaErrors === null ? "—" : formatCount(mediaErrors)],
    [t("записей в журнале ошибок"), errorLogEntries === null ? "—" : formatCount(errorLogEntries)],
    [t("записано"), dataWritten === null ? "—" : formatBytes(dataWritten, 1)],
    [t("прочитано"), dataRead === null ? "—" : formatBytes(dataRead, 1)],
  ];
  return (
    <details className="mt-1.5">
      <summary className="cursor-pointer text-[10px] text-slate-500">{t("подробнее")}</summary>
      <dl className="mt-1 grid grid-cols-2 gap-x-3 gap-y-0.5 text-[10px]">
        {rows.map(([label, value]) => (
          <div key={label} className="flex items-baseline justify-between gap-2">
            <dt className="text-slate-500">{label}</dt>
            <dd className="truncate text-slate-300">{value}</dd>
          </div>
        ))}
      </dl>
    </details>
  );
}

export function MaintenanceTab() {
  const statusQuery = useQuery(queryClient.system.maintenanceStatus.queryOptions());
  const status = statusQuery.data as MaintenanceStatus | undefined;
  const [pending, setPending] = useState<Pending | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [categories, setCategories] = useState<string[]>(["win-temp"]);
  const [smartDisks, setSmartDisks] = useState<SmartDisk[] | null>(null);
  const [loadedPoints, setLoadedPoints] = useState<MaintenanceStatus["restorePoints"] | null>(null);

  const operation = useMutation({
    mutationFn: (input: { op: string; payload?: unknown }) => api.systemOperation(input),
  });
  const smartMutation = useMutation({
    mutationFn: () => api.smartRead(),
  });

  async function readSmart() {
    setError(null);
    try {
      setSmartDisks(await smartMutation.mutateAsync());
    } catch (cause) {
      setError(translateError(cause));
    }
  }

  async function runPending() {
    if (!pending) return;
    setError(null);
    try {
      const response = await operation.mutateAsync({ op: pending.op, payload: pending.payload });
      setResult(translateError(response.message));
      setPending(null);
      if (pending.op === "restore_point_delete" && loadedPoints) {
        const sequence = (pending.payload as { sequence?: number } | undefined)?.sequence;
        setLoadedPoints(loadedPoints.filter((point) => point.sequence !== sequence));
      }
      await statusQuery.refetch();
    } catch (error) {
      setResult(null);
      setError(translateError(error));
    }
  }

  const helperMissing = status && !status.helperAvailable;
  const smartLoaded = smartDisks ? smartDisks.length > 0 : Boolean(status?.disks.some((disk) => disk.smart));
  const points = loadedPoints ?? status?.restorePoints ?? [];
  const oldest = points.reduce(
    (best, point) => (best === null || point.sequence < best.sequence ? point : best),
    null as { sequence: number } | null,
  );

  async function loadRestorePoints() {
    setError(null);
    try {
      const response = await operation.mutateAsync({ op: "restore_points_list" });
      const data = response.data as MaintenanceStatus["restorePoints"];
      setLoadedPoints(Array.isArray(data) ? data : []);
      setResult(translateError(response.message));
    } catch (error) {
      setResult(null);
      setError(translateError(error));
    }
  }

  return (
    <div className="space-y-5">
      <SectionTitle
        title={t("Обслуживание системы")}
        subtitle={t("Windows.old, WinSxS, точки восстановления и здоровье дисков — операции с UAC только по кнопке")}
        action={
          <Button variant="secondary" onClick={() => void statusQuery.refetch()} loading={statusQuery.isFetching}>
            <RefreshIcon className="size-4" />
            {t("Обновить")}
          </Button>
        }
      />

      {helperMissing ? (
        <Card className="border-amber-400/25 bg-amber-400/[0.06] p-3.5">
          <p className="flex items-center gap-2 text-xs text-amber-200">
            <AlertIcon className="size-3.5" /> {t("Helper не найден рядом с приложением. Системные операции будут недоступны до пересборки.")}
          </p>
        </Card>
      ) : null}

      {result ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-3.5">
          <p className="text-xs text-emerald-100">{result}</p>
        </Card>
      ) : null}

      {error ? (
        <Card className="border-rose-400/25 bg-rose-500/[0.07] p-3.5">
          <p className="text-xs text-rose-100">{error}</p>
        </Card>
      ) : null}

      <div className="grid items-start gap-4 xl:grid-cols-2">
        <div className="space-y-4">
          <Card className="p-4">
            <div className="mb-3 flex items-center gap-2">
              <ShieldIcon className="size-4 text-emerald-300" />
              <p className="text-sm font-medium text-white">{t("Здоровье дисков")}</p>
            </div>
            {!status?.disks.length ? (
              smartDisks?.length ? (
                <div className="space-y-2">
                  {smartDisks.map((disk) => (
                    <div key={disk.index} className="rounded-xl border border-white/[0.07] bg-white/[0.02] p-2.5">
                      <p className="truncate text-xs font-medium text-slate-100">
                        {disk.model || t("NVMe-диск {index}", { index: disk.index })}
                      </p>
                      <p className="mt-1 text-[11px] text-slate-500">
                        {t("NVMe")} · {formatBytes(disk.dataWritten ?? 0, 0)} {t("записано")}
                        {disk.temperature !== null ? t(" · {value} °C", { value: disk.temperature }) : ""}
                        {disk.wear !== null ? t(" · износ {value}%", { value: disk.wear }) : ""}
                        {disk.powerOnHours !== null ? t(" · {value} ч", { value: disk.powerOnHours }) : ""}
                        {(disk.mediaErrors ?? 0) > 0 ? t(" · ошибок {value}", { value: disk.mediaErrors ?? 0 }) : ""}
                      </p>
                      <SmartWarningChips warnings={disk.warnings} critical={disk.criticalWarning} />
                      <SmartDetails
                        source={t("NVMe SMART")}
                        availableSpare={disk.availableSpare}
                        spareThreshold={disk.spareThreshold}
                        unsafeShutdowns={disk.unsafeShutdowns}
                        mediaErrors={disk.mediaErrors}
                        errorLogEntries={disk.errorLogEntries}
                        dataRead={disk.dataRead}
                        dataWritten={disk.dataWritten}
                      />
                    </div>
                  ))}
                </div>
              ) : (
                <p className="text-xs text-slate-500">{t("Нет данных SMART/WMI.")}</p>
              )
            ) : (
              <div className="space-y-2">
                {status.disks.map((disk) => {
                  const health = healthLabel(disk.health);
                  const matched = disk.smart
                    ? undefined
                    : smartDisks?.find(
                        (item) =>
                          item.index === Number(disk.deviceId) ||
                          (item.serial !== "" && item.serial === disk.serial),
                      );
                  const hasSmart = disk.smart || Boolean(matched);
                  const temperature = disk.temperature ?? matched?.temperature ?? null;
                  const wear = disk.wear ?? matched?.wear ?? null;
                  const hours = disk.powerOnHours ?? matched?.powerOnHours ?? null;
                  const warnings = disk.warnings.length ? disk.warnings : (matched?.warnings ?? []);
                  const critical = disk.criticalWarning ?? matched?.criticalWarning ?? 0;
                  const mediaErrors = disk.mediaErrors ?? matched?.mediaErrors ?? null;
                  const availableSpare = disk.availableSpare ?? matched?.availableSpare ?? null;
                  const spareThreshold = disk.spareThreshold ?? matched?.spareThreshold ?? null;
                  const unsafeShutdowns = disk.unsafeShutdowns ?? matched?.unsafeShutdowns ?? null;
                  const errorLogEntries = disk.errorLogEntries ?? matched?.errorLogEntries ?? null;
                  const dataRead = disk.dataRead ?? matched?.dataRead ?? null;
                  const dataWritten = disk.dataWritten ?? matched?.dataWritten ?? null;
                  const unsupportedBus =
                    !hasSmart && disk.busType !== null && disk.busType !== 17;
                  return (
                    <div key={disk.deviceId} className="rounded-xl border border-white/[0.07] bg-white/[0.02] p-2.5">
                      <div className="flex items-center justify-between gap-2">
                        <p className="truncate text-xs font-medium text-slate-100">{disk.name}</p>
                        <Badge variant={health.variant}>{health.text}</Badge>
                      </div>
                      <p className="mt-1 text-[11px] text-slate-500">
                        {mediaLabel(disk.mediaType)} · {formatBytes(disk.size, 0)}
                        {temperature !== null ? t(" · {value} °C", { value: temperature }) : ""}
                        {wear !== null ? t(" · износ {value}%", { value: wear }) : ""}
                        {hours !== null ? t(" · {value} ч", { value: hours }) : ""}
                        {mediaErrors !== null && mediaErrors > 0 ? t(" · ошибок {value}", { value: mediaErrors }) : ""}
                      </p>
                      {hasSmart ? <SmartWarningChips warnings={warnings} critical={critical} /> : null}
                      {hasSmart ? (
                        <SmartDetails
                          source={t("NVMe SMART")}
                          availableSpare={availableSpare}
                          spareThreshold={spareThreshold}
                          unsafeShutdowns={unsafeShutdowns}
                          mediaErrors={mediaErrors}
                          errorLogEntries={errorLogEntries}
                          dataRead={dataRead}
                          dataWritten={dataWritten}
                        />
                      ) : unsupportedBus ? (
                        <p className="mt-1 text-[10px] text-slate-500">
                          {t("SMART пока поддерживается только для NVMe-дисков.")}
                        </p>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            )}
            <div className="mt-2 space-y-2">
              <Button
                size="sm"
                variant="secondary"
                onClick={() => void readSmart()}
                loading={smartMutation.isPending}
              >
                {smartLoaded
                  ? status?.elevated
                    ? t("Обновить SMART")
                    : t("Обновить SMART (UAC)")
                  : status?.elevated
                    ? t("Прочитать SMART")
                    : t("Прочитать SMART (UAC)")}
              </Button>
              {smartDisks !== null && smartDisks.length === 0 ? (
                <p className="text-[11px] text-slate-500">
                  {status?.disks.some((disk) => disk.busType !== null && disk.busType !== 17)
                    ? t("SMART пока поддерживается только для NVMe-дисков.")
                    : t("SMART не прочитан: NVMe-диск не найден или Windows не отдал журнал здоровья.")}
                </p>
              ) : null}
            </div>
          </Card>

          <Card className="p-4">
            <p className="text-sm font-medium text-white">{t("Системные категории от администратора")}</p>
            <p className="mt-1 text-[11px] text-slate-500">
              {t("Очистка Temp Windows, кэша обновлений, Delivery Optimization, Prefetch и логов.")}
            </p>
            <div className="mt-2 grid grid-cols-2 gap-1">
              {ADMIN_CATEGORIES.map(([id, label]) => (
                <label key={id} className="flex items-center gap-2 text-[11px] text-slate-300">
                  <input
                    type="checkbox"
                    className="size-3.5 accent-emerald-500"
                    checked={categories.includes(id)}
                    onChange={(event) =>
                      setCategories((current) =>
                        event.target.checked ? [...current, id] : current.filter((value) => value !== id),
                      )
                    }
                  />
                  {t(label)}
                </label>
              ))}
            </div>
            <Button
              className="mt-3"
              size="sm"
              variant="danger"
              disabled={categories.length === 0}
              onClick={() =>
                setPending({
                  op: "clean_system",
                  payload: { categories },
                  title: t("Очистить системные категории?"),
                  description: t("Файлы будут удалены безвозвратно, потребуется UAC."),
                  danger: true,
                })
              }
            >
              {t("Очистить от администратора")}
            </Button>
          </Card>

          <Card className="p-4">
            <p className="text-sm font-medium text-white">{t("Точки восстановления")}</p>
            {points.length ? (
              <div className="mt-2 max-h-40 space-y-1 overflow-auto">
                {points.map((point) => (
                  <div key={point.sequence} className="flex items-center justify-between gap-2 text-[11px]">
                    <span className="truncate text-slate-300">{point.description ?? t("Точка {sequence}", { sequence: point.sequence })}</span>
                    <span className="shrink-0 text-slate-500">{point.createdAt ? formatDate(point.createdAt) : ""}</span>
                  </div>
                ))}
              </div>
            ) : (
              <p className="mt-2 text-[11px] text-slate-500">{t("Точки восстановления не найдены.")}</p>
            )}
            <div className="mt-3 flex flex-wrap gap-2">
              <Button
                size="sm"
                variant="secondary"
                onClick={() => void loadRestorePoints()}
                loading={operation.isPending}
              >
                {t("Загрузить список (UAC)")}
              </Button>
              <Button
                size="sm"
                variant="secondary"
                onClick={() =>
                  setPending({
                    op: "restore_point_create",
                    title: t("Создать точку восстановления?"),
                    description: t("Windows создаст точку восстановления системы (потребуется UAC)."),
                  })
                }
              >
                {t("Создать точку")}
              </Button>
              {oldest ? (
                <Button
                  size="sm"
                  variant="danger"
                  onClick={() =>
                    setPending({
                      op: "restore_point_delete",
                      payload: { sequence: oldest.sequence },
                      title: t("Удалить старую точку?"),
                      description: t("Будет удалена точка №{sequence}.", { sequence: oldest.sequence }),
                      danger: true,
                    })
                  }
                >
                  {t("Удалить старую")}
                </Button>
              ) : null}
            </div>
          </Card>
        </div>

        <div className="space-y-4">
          <SensorsPanel wmiSensors={status?.sensors ?? []} />

          <Card className="p-4">
            <p className="text-sm font-medium text-white">Windows.old</p>
            <p className="mt-1 text-[11px] leading-relaxed text-slate-500">
              {t("Удаляет папку предыдущей установки Windows. Освобождает обычно 5–20 ГБ, откат на старую версию после этого невозможен.")}
            </p>
            <Button
              className="mt-3"
              size="sm"
              variant="danger"
              onClick={() =>
                setPending({
                  op: "windows_old",
                  title: t("Удалить Windows.old?"),
                  description: t("Откат к предыдущей версии Windows станет невозможен."),
                  danger: true,
                })
              }
            >
              <TrashIcon className="size-4" />
              {t("Удалить Windows.old")}
            </Button>
          </Card>

          <Card className="p-4">
            <p className="text-sm font-medium text-white">WinSxS (DISM)</p>
            <p className="mt-1 text-[11px] leading-relaxed text-slate-500">
              {t("Запускает DISM StartComponentCleanup. Может занять 10–30 минут; файлы WinSxS вручную не удаляются.")}
            </p>
            <Button
              className="mt-3"
              size="sm"
              variant="secondary"
              onClick={() =>
                setPending({
                  op: "dism_cleanup",
                  title: t("Очистить WinSxS?"),
                  description: t("DISM выполнит очистку компонентов. Это может занять длительное время."),
                })
              }
            >
              {t("Запустить DISM")}
            </Button>
          </Card>
        </div>
      </div>

      {!status && !statusQuery.isLoading ? (
        <EmptyState icon={<ShieldIcon className="size-6" />} title={t("Нет данных")} description={t("Не удалось получить состояние системы.")} />
      ) : null}

      <Modal
        open={Boolean(pending)}
        onClose={() => setPending(null)}
        title={pending?.title ?? ""}
        description={pending?.description}
        footer={
          <>
            <Button variant="ghost" onClick={() => setPending(null)}>
              {t("Отмена")}
            </Button>
            <Button
              variant={pending?.danger ? "danger" : "primary"}
              onClick={() => void runPending()}
              loading={operation.isPending}
            >
              {t("Продолжить")}
            </Button>
          </>
        }
      />
    </div>
  );
}
