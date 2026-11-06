import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { api } from "@/client/api";
import { queryClient } from "@/client/rpc-client";
import type { MaintenanceStatus } from "@/shared/types";
import { formatBytes, formatDate } from "@/client/lib/format";
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

export function MaintenanceTab() {
  const statusQuery = useQuery(queryClient.system.maintenanceStatus.queryOptions());
  const status = statusQuery.data as MaintenanceStatus | undefined;
  const [pending, setPending] = useState<Pending | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [categories, setCategories] = useState<string[]>(["win-temp"]);
  const [loadedPoints, setLoadedPoints] = useState<MaintenanceStatus["restorePoints"] | null>(null);

  const operation = useMutation({
    mutationFn: (input: { op: string; payload?: unknown }) => api.systemOperation(input),
  });

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
              <p className="text-xs text-slate-500">{t("Нет данных SMART/WMI.")}</p>
            ) : (
              <div className="space-y-2">
                {status.disks.map((disk) => {
                  const health = healthLabel(disk.health);
                  return (
                    <div key={disk.deviceId} className="rounded-xl border border-white/[0.07] bg-white/[0.02] p-2.5">
                      <div className="flex items-center justify-between gap-2">
                        <p className="truncate text-xs font-medium text-slate-100">{disk.name}</p>
                        <Badge variant={health.variant}>{health.text}</Badge>
                      </div>
                      <p className="mt-1 text-[11px] text-slate-500">
                        {mediaLabel(disk.mediaType)} · {formatBytes(disk.size, 0)}
                        {disk.temperature !== null ? t(" · {value} °C", { value: disk.temperature }) : ""}
                        {disk.wear !== null ? t(" · износ {value}%", { value: disk.wear }) : ""}
                        {disk.powerOnHours !== null ? t(" · {value} ч", { value: disk.powerOnHours }) : ""}
                      </p>
                    </div>
                  );
                })}
              </div>
            )}
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
