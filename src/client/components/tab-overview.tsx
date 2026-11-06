import type { SystemInfo } from "@/shared/types";
import { formatBytes, formatUptime, usedPercent } from "@/client/lib/format";
import { Button, Card, Progress, SectionTitle } from "./ui";
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

function DriveCard({ drive }: { drive: SystemInfo["drives"][number] }) {
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
  scanRunning,
}: {
  info: SystemInfo | null | undefined;
  loading: boolean;
  error: boolean;
  onScan: () => void;
  onGoPrograms: () => void;
  onGoUpdates: () => void;
  scanRunning: boolean;
}) {
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
          <DriveCard key={drive.letter} drive={drive} />
        ))}
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
