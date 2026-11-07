import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import type { DriveSnapshot, SystemInfo } from "@/shared/types";
import { queryClient } from "@/client/rpc-client";
import { formatBytes, formatCount, formatDate, formatUptime, usedPercent } from "@/client/lib/format";
import { Badge, Button, Card, Progress, SectionTitle } from "./ui";
import {
  CpuIcon,
  GaugeIcon,
  MemoryIcon,
  ShieldIcon,
  SparkleIcon,
  WindowsIcon,
} from "./icons";
import { t } from "@/client/i18n";

function driveTone(percent: number): "emerald" | "amber" | "rose" {
  if (percent >= 90) return "rose";
  if (percent >= 75) return "amber";
  return "emerald";
}

type Forecast = { text: string; tone: "neutral" | "warn" | "danger" };

function forecastFor(letter: string, points: DriveSnapshot[], currentFree?: number): Forecast | null {
  const series = points
    .map((point) => ({ t: point.t, drive: point.drives.find((item) => item.letter === letter) }))
    .filter((item): item is { t: number; drive: NonNullable<typeof item.drive> } => Boolean(item.drive));
  if (series.length < 2) return null;
  const first = series[0];
  const last = series[series.length - 1];
  const days = (last.t - first.t) / 86_400_000;
  if (days < 0.2) return null;
  const growth = (last.drive.used - first.drive.used) / days;
  if (growth <= 0) return { text: t("заполнение не растёт"), tone: "neutral" };
  const free = currentFree ?? last.drive.free;
  if (free <= 0) return { text: t("диск заполнен"), tone: "danger" };
  const daysLeft = free / growth;
  if (daysLeft > 3650) return { text: t("больше 10 лет"), tone: "neutral" };
  const rounded = Math.max(1, Math.round(daysLeft));
  return {
    text: t("заполнится через {days} дн.", { days: rounded }),
    tone: daysLeft <= 7 ? "danger" : daysLeft <= 30 ? "warn" : "neutral",
  };
}

function DriveCard({
  drive,
  forecast,
}: {
  drive: SystemInfo["drives"][number];
  forecast?: Forecast | null;
}) {
  const percent = usedPercent(drive.used, drive.total);
  return (
    <Card className="p-4">
      <div className="flex items-center gap-3">
        <div className="grid size-10 shrink-0 place-items-center rounded-xl bg-gradient-to-br from-sky-500/25 to-indigo-500/20 text-sm font-bold text-sky-200 ring-1 ring-inset ring-white/10">
          {drive.letter}
        </div>
        <div className="min-w-0">
          <p className="truncate text-sm font-medium text-white">
            {drive.label || t("Локальный диск")}
          </p>
          <p className="text-[11px] text-slate-500">
            {drive.letter}:\ {drive.fileSystem ? `· ${drive.fileSystem}` : ""}
          </p>
        </div>
        <div className="ml-auto text-right">
          <p className="text-sm font-semibold tabular-nums text-white">{percent}%</p>
          <p className="text-[11px] text-slate-500">{t("занято")}</p>
        </div>
      </div>

      <Progress className="mt-3.5" value={percent} tone={driveTone(percent)} />

      <div className="mt-2 flex items-center justify-between text-[11px]">
        <span className="text-slate-400">
          {formatBytes(drive.used)} {t("из")} {formatBytes(drive.total)}
        </span>
        <span className={percent >= 90 ? "text-rose-300" : "text-slate-400"}>
          {t("свободно")} {formatBytes(drive.free)}
        </span>
      </div>
      {forecast ? (
        <p
          className={`mt-1 text-[10px] ${
            forecast.tone === "danger"
              ? "text-rose-300"
              : forecast.tone === "warn"
                ? "text-amber-300"
                : "text-slate-500"
          }`}
        >
          {forecast.text}
        </p>
      ) : null}
    </Card>
  );
}

function DrivesSummaryCard({ drives }: { drives: SystemInfo["drives"] }) {
  const total = drives.reduce((sum, drive) => sum + drive.total, 0);
  const used = drives.reduce((sum, drive) => sum + drive.used, 0);
  const free = drives.reduce((sum, drive) => sum + drive.free, 0);
  const percent = usedPercent(used, total);
  return (
    <Card className="flex flex-col p-4">
      <div className="flex items-center gap-2">
        <GaugeIcon className="size-4 text-slate-400" />
        <h3 className="text-sm font-medium text-white">{t("Сводка")}</h3>
      </div>
      <div className="mt-3 flex flex-1 items-end justify-between gap-3">
        <div className="min-w-0">
          <p className="text-2xl font-semibold tabular-nums text-white">{formatBytes(free, 0)}</p>
          <p className="mt-0.5 text-[11px] text-slate-500">
            {t("свободно")} {t("из")} {formatBytes(total, 0)}
          </p>
        </div>
        <span className="text-sm font-semibold tabular-nums text-slate-300">{percent}%</span>
      </div>
      <Progress className="mt-3" value={percent} tone={driveTone(percent)} />
      <p className="mt-2 text-[11px] text-slate-500">
        {t("Дисков: {count} · занято {used}", { count: drives.length, used: formatBytes(used, 0) })}
      </p>
    </Card>
  );
}

function Stat({
  icon,
  label,
  value,
}: {
  icon: React.ReactNode;
  label: string;
  value: string;
}) {
  return (
    <div className="flex items-start gap-3 rounded-xl border border-white/[0.06] bg-white/[0.02] p-3">
      <div className="mt-0.5 text-slate-400">{icon}</div>
      <div className="min-w-0">
        <p className="text-[11px] uppercase tracking-wide text-slate-500">{label}</p>
        <p className="mt-0.5 truncate text-sm text-slate-100" title={value}>
          {value}
        </p>
      </div>
    </div>
  );
}

export function OverviewTab({
  info,
  loading,
  error,
  onScan,
  onGoPrograms,
  onGoUpdates,
  onGoMaintenance,
  scanRunning,
}: {
  info: SystemInfo | null | undefined;
  loading: boolean;
  error: boolean;
  onScan: () => void;
  onGoPrograms: () => void;
  onGoUpdates: () => void;
  onGoMaintenance: () => void;
  scanRunning: boolean;
}) {
  const historyQuery = useQuery(queryClient.system.historyGet.queryOptions());
  const smartQuery = useQuery(queryClient.system.smartHistory.queryOptions());
  const cleanQuery = useQuery(queryClient.system.cleanHistory.queryOptions());
  const [now] = useState(() => Date.now());

  if (loading && !info) {
    return (
      <div className="grid gap-4 sm:grid-cols-2">
        {[0, 1].map((index) => (
          <div key={index} className="h-32 animate-pulse rounded-2xl bg-white/[0.04]" />
        ))}
      </div>
    );
  }

  if (error || !info) {
    return (
      <Card className="p-6">
        <p className="text-sm text-rose-200">{t("Не удалось получить данные о системе.")}</p>
        <p className="mt-1 text-xs text-slate-400">
          {t("Проверьте, что сервер приложения запущен на этом компьютере.")}
        </p>
      </Card>
    );
  }

  const totalSpace = info.drives.reduce((sum, drive) => sum + drive.total, 0);
  const totalFree = info.drives.reduce((sum, drive) => sum + drive.free, 0);
  const ramUsedPercent = info.ramTotal
    ? Math.round(((info.ramTotal - info.ramFree) / info.ramTotal) * 100)
    : 0;

  const points = historyQuery.data?.points ?? [];
  const smartDrives = (smartQuery.data ?? []).filter((drive) => drive.samples.length > 0);
  const cleanCutoff = now - 30 * 24 * 60 * 60 * 1000;
  const recentCleans = (cleanQuery.data ?? []).filter((entry) => entry.at >= cleanCutoff);
  const cleanedBytes = recentCleans.reduce((sum, entry) => sum + entry.bytes, 0);
  const cleanedFiles = recentCleans.reduce((sum, entry) => sum + entry.files, 0);
  const lastClean = recentCleans.reduce<number | null>(
    (latest, entry) => (latest === null || entry.at > latest ? entry.at : latest),
    null,
  );

  return (
    <div className="space-y-6">
      <SectionTitle
        title={t("Диски")}
        subtitle={t("{count} локальных диска · свободно {free} из {total}", {
          count: info.drives.length,
          free: formatBytes(totalFree),
          total: formatBytes(totalSpace),
        })}
        action={
          <Button variant="primary" onClick={onScan} disabled={scanRunning}>
            <SparkleIcon className="size-4" />
            {scanRunning ? t("Сканирование…") : t("Проверить мусор")}
          </Button>
        }
      />

      <div className="grid gap-3 sm:grid-cols-2">
        {info.drives.map((drive) => (
          <DriveCard
            key={drive.letter}
            drive={drive}
            forecast={forecastFor(drive.letter, points, drive.free)}
          />
        ))}
        {info.drives.length % 2 === 1 ? <DrivesSummaryCard drives={info.drives} /> : null}
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card className="p-4">
          <div className="mb-3 flex items-center justify-between gap-2">
            <div className="flex items-center gap-2">
              <ShieldIcon className="size-4 text-emerald-300" />
              <h3 className="text-sm font-medium text-white">{t("Здоровье дисков")}</h3>
            </div>
            <Button size="sm" variant="secondary" onClick={onGoMaintenance}>
              {t("Подробнее")}
            </Button>
          </div>
          {smartDrives.length === 0 ? (
            <p className="text-[11px] text-slate-500">
              {t("Нет данных SMART. Откройте «Обслуживание», чтобы прочитать состояние дисков.")}
            </p>
          ) : (
            <div className="space-y-2">
              {smartDrives.map((drive) => {
                const last = drive.samples[drive.samples.length - 1];
                const attention = last.criticalWarning > 0 || last.healthPassed === false;
                return (
                  <div
                    key={drive.key}
                    className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-xl border border-white/[0.06] bg-white/[0.02] px-3 py-2"
                  >
                    <span className="min-w-0 flex-1 truncate text-xs text-slate-200">
                      {drive.model || t("Диск")}
                    </span>
                    {attention ? <Badge variant="danger">{t("внимание")}</Badge> : null}
                    <span className="text-[11px] text-slate-400">
                      {last.temperature !== null ? `${last.temperature} °C` : "—"}
                      {last.wear !== null ? ` · ${t("износ")} ${last.wear}%` : ""}
                    </span>
                    <span className="text-[10px] text-slate-600">{formatDate(last.t)}</span>
                  </div>
                );
              })}
            </div>
          )}
        </Card>

        <Card className="p-4">
          <div className="mb-3 flex items-center gap-2">
            <SparkleIcon className="size-4 text-sky-300" />
            <h3 className="text-sm font-medium text-white">{t("Очистка за 30 дней")}</h3>
          </div>
          {recentCleans.length === 0 ? (
            <p className="text-[11px] text-slate-500">{t("Пока нет очисток за последние 30 дней.")}</p>
          ) : (
            <>
              <p className="text-2xl font-semibold tabular-nums text-white">{formatBytes(cleanedBytes, 0)}</p>
              <p className="mt-1 text-[11px] text-slate-400">
                {formatCount(cleanedFiles)} {t("файлов · очисток:")} {recentCleans.length}
              </p>
              {lastClean !== null ? (
                <p className="mt-1 text-[10px] text-slate-600">
                  {t("Последняя: {date}", { date: formatDate(lastClean) })}
                </p>
              ) : null}
            </>
          )}
        </Card>
      </div>

      <div className="grid gap-4 lg:grid-cols-[1.5fr_1fr]">
        <Card className="p-4">
          <div className="mb-3 flex items-center gap-2">
            <WindowsIcon className="size-4 text-sky-300" />
            <h3 className="text-sm font-medium text-white">{t("Система")}</h3>
          </div>
          <div className="grid gap-2 sm:grid-cols-2">
            <Stat icon={<WindowsIcon />} label={t("Операционная система")} value={`${info.os} · ${info.build}`} />
            <Stat
              icon={<CpuIcon />}
              label={t("Процессор")}
              value={`${info.cpu} · ${t("{count} ядер", { count: info.cores })}`}
            />
            <Stat
              icon={<MemoryIcon />}
              label={t("Оперативная память")}
              value={t("{used} из {total} ({percent}%)", {
                used: formatBytes(info.ramTotal - info.ramFree, 0),
                total: formatBytes(info.ramTotal, 0),
                percent: ramUsedPercent,
              })}
            />
            <Stat icon={<GaugeIcon />} label={t("Время работы")} value={formatUptime(info.lastBoot)} />
          </div>
          <div className="mt-3 flex flex-wrap items-center gap-2 text-[11px] text-slate-500">
            <span className="font-mono">{info.machine}</span>
            <span>·</span>
            <span>{info.user}</span>
          </div>
        </Card>

        <Card className="p-4">
          <div className="mb-3 flex items-center gap-2">
            <ShieldIcon className="size-4 text-amber-300" />
            <h3 className="text-sm font-medium text-white">{t("Права доступа")}</h3>
          </div>
          {info.admin ? (
            <p className="text-xs leading-relaxed text-emerald-200">
              {t("Программа запущена от имени администратора — доступны все разделы очистки.")}
            </p>
          ) : (
            <p className="text-xs leading-relaxed text-amber-200/90">
              {t("Нет прав администратора. Системные папки (Temp Windows, кэш обновлений, Prefetch, логи) недоступны для чтения и очистки. Запустите приложение от имени администратора, чтобы видеть их.")}
            </p>
          )}
          <div className="mt-3 flex flex-col gap-2">
            <Button size="sm" variant="secondary" onClick={onGoPrograms} className="justify-between">
              {t("Установленные программы")}
              <span className="text-slate-500">→</span>
            </Button>
            <Button size="sm" variant="secondary" onClick={onGoUpdates} className="justify-between">
              {t("Обновления winget")}
              <span className="text-slate-500">→</span>
            </Button>
          </div>
        </Card>
      </div>
    </div>
  );
}
