import { useMemo, useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { DriveSnapshot } from "@/shared/types";
import { queryClient } from "@/client/rpc-client";
import { formatBytes, formatDate } from "@/client/lib/format";
import { Badge, Button, Card, EmptyState, SectionTitle } from "./ui";
import { ArrowRightIcon, GaugeIcon, RefreshIcon } from "./icons";
import { t } from "@/client/i18n";

function formatAxis(bytes: number): string {
  return formatBytes(bytes, 2);
}

function DriveChart({
  points,
  letter,
}: {
  points: DriveSnapshot[];
  letter: string;
}) {
  const series = useMemo(
    () =>
      points
        .map((point) => {
          const drive = point.drives.find((item) => item.letter === letter);
          return drive
            ? { t: point.t, used: drive.used, free: drive.free, total: drive.total }
            : null;
        })
        .filter((value): value is { t: number; used: number; free: number; total: number } => Boolean(value)),
    [points, letter],
  );

  if (series.length === 0) return null;

  const width = 620;
  const height = 160;
  const padLeft = 56;
  const padBottom = 22;
  const plotWidth = width - padLeft - 8;
  const plotHeight = height - padBottom - 10;

  const totals = series.map((item) => item.total).filter((value) => value > 0);
  const capacity = totals.length > 0 ? Math.max(...totals) : Math.max(...series.map((item) => item.used));
  const usedValues = series.map((item) => item.used);
  const minUsed = Math.min(...usedValues);
  const maxUsed = Math.max(...usedValues);
  const span = Math.max(1, maxUsed - minUsed);
  const lower = Math.max(0, minUsed - span * 0.25);
  const upper = Math.max(lower + 1, maxUsed + span * 0.1);

  const xFor = (index: number) =>
    padLeft + (series.length > 1 ? (index / (series.length - 1)) * plotWidth : plotWidth / 2);
  const yFor = (value: number) =>
    10 + plotHeight - ((value - lower) / (upper - lower)) * plotHeight;

  const line = series.map((item, index) => `${xFor(index).toFixed(1)},${yFor(item.used).toFixed(1)}`);
  const path = `M${line.join(" L")}`;
  const area = `${path} L${xFor(series.length - 1)},${10 + plotHeight} L${padLeft},${10 + plotHeight} Z`;

  const first = series[0];
  const last = series[series.length - 1];
  const delta = last.used - first.used;
  const spanMs = last.t - first.t;
  // A trend from a few minutes of data is just noise — require an hour.
  const perDay = spanMs >= 60 * 60 * 1000 ? delta / (spanMs / 86_400_000) : null;

  const ticks = [upper, (upper + lower) / 2, lower];

  return (
    <div className="rounded-xl border border-white/[0.07] bg-white/[0.02] p-3">
      <div className="mb-2 flex flex-wrap items-baseline justify-between gap-2">
        <div className="flex items-center gap-2">
          <span className="grid size-6 place-items-center rounded-md bg-sky-500/15 text-[11px] font-bold text-sky-300">
            {letter}
          </span>
          <span className="text-xs text-slate-300">
            {t("сейчас")} {formatBytes(last.used)} {t("из")} {formatBytes(capacity)}
          </span>
        </div>
        {perDay === null ? (
          <Badge variant="neutral">{t("мало данных для тренда")}</Badge>
        ) : (
          <Badge variant={perDay > 0 ? "warn" : "success"}>
            {perDay > 0 ? "+" : ""}
            {formatBytes(Math.abs(perDay), 0)}{t("/день")} {perDay > 0 ? t("растёт") : t("освобождается")}
          </Badge>
        )}
      </div>

      {perDay !== null && perDay > 0 && last.free > 0 ? (
        <p
          className={`mb-1 text-[11px] ${
            last.free / perDay <= 7
              ? "text-rose-300"
              : last.free / perDay <= 30
                ? "text-amber-300"
                : "text-slate-500"
          }`}
        >
          {last.free / perDay > 3650
            ? t("больше 10 лет")
            : t("заполнится через {days} дн.", {
                days: Math.max(1, Math.round(last.free / perDay)),
              })}
        </p>
      ) : null}

      <svg viewBox={`0 0 ${width} ${height}`} className="h-40 w-full" preserveAspectRatio="none">
        <defs>
          <linearGradient id={`drive-fill-${letter}`} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="rgba(56,189,248,0.30)" />
            <stop offset="100%" stopColor="rgba(56,189,248,0)" />
          </linearGradient>
        </defs>

        {ticks.map((value, index) => (
          <g key={index}>
            <line
              x1={padLeft}
              x2={width - 8}
              y1={yFor(value)}
              y2={yFor(value)}
              stroke="rgba(148,163,184,0.14)"
              strokeDasharray="3 4"
            />
            <text x={padLeft - 6} y={yFor(value) + 3} textAnchor="end" className="fill-slate-500" fontSize="9">
              {formatAxis(value)}
            </text>
          </g>
        ))}

        <path d={area} fill={`url(#drive-fill-${letter})`} />
        <path
          d={path}
          fill="none"
          stroke="#38bdf8"
          strokeWidth="1.8"
          strokeLinejoin="round"
          strokeLinecap="round"
          vectorEffect="non-scaling-stroke"
        />

        {series.length <= 40
          ? series.map((item, index) => (
            <circle key={item.t} cx={xFor(index)} cy={yFor(item.used)} r="1.8" fill="#7dd3fc" />
          ))
          : null}

        <text x={padLeft} y={height - 6} fontSize="9" className="fill-slate-500">
          {formatDate(first.t)}
        </text>
        <text x={width - 8} y={height - 6} textAnchor="end" fontSize="9" className="fill-slate-500">
          {formatDate(last.t)}
        </text>
      </svg>
    </div>
  );
}

export function HistoryTab() {
  const [capturing, setCapturing] = useState(false);

  const historyQuery = useQuery(queryClient.system.historyGet.queryOptions());
  const captureMutation = useMutation(queryClient.system.historyCapture.mutationOptions());
  const cleanHistoryQuery = useQuery(queryClient.system.cleanHistory.queryOptions());
  const cleanHistory = cleanHistoryQuery.data ?? [];

  const summary = historyQuery.data;

  async function capture() {
    setCapturing(true);
    await captureMutation.mutateAsync({});
    await historyQuery.refetch();
    setCapturing(false);
  }

  const points = summary?.points ?? [];
  const letters = summary?.letters ?? [];

  return (
    <div className="space-y-5">
      <SectionTitle
        title={t("История места на диске")}
        subtitle={t("Снимки свободного места каждые 30 минут — видно, как быстро диск заполняется")}
        action={
          <Button variant="secondary" onClick={capture} loading={capturing}>
            <RefreshIcon className="size-4" />
            {t("Снять сейчас")}
          </Button>
        }
      />

      {summary && summary.growthPerDay > 0 ? (
        <Card className="border-amber-400/25 bg-amber-400/[0.06] p-3.5">
          <p className="flex items-center gap-2 text-xs text-amber-200">
            <ArrowRightIcon className="size-3.5" />
            {t("В среднем занятое место растёт на")} {formatBytes(summary.growthPerDay, 0)} {t("в день — за месяц это")} {formatBytes(summary.growthPerDay * 30, 1)}
          </p>
        </Card>
      ) : null}

      {points.length === 0 ? (
        <EmptyState
          icon={<GaugeIcon className="size-6" />}
          title={t("История пока пуста")}
          description={t("Первый снимок создаётся сразу, дальше — автоматически каждые полчаса (и при каждом запуске приложения). Через несколько дней здесь появится тренд расхода места.")}
          action={
            <Button variant="primary" onClick={capture} loading={capturing}>
              {t("Сделать первый снимок")}
            </Button>
          }
        />
      ) : (
        <div className="space-y-3">
          {letters.map((letter) => (
            <DriveChart key={letter} points={points} letter={letter} />
          ))}
        </div>
      )}

      {points.length > 0 ? (
        <Card className="overflow-hidden">
          <div className="border-b border-white/[0.08] px-4 py-2.5">
            <p className="text-[13px] font-medium text-white">
              {points.length} {t("снимков · с")} {formatDate(points[0].t)}
            </p>
          </div>
          <div className="max-h-64 divide-y divide-white/[0.05] overflow-auto">
            {[...points]
              .reverse()
              .slice(0, 40)
              .map((point) => (
                <div key={point.t} className="flex items-center gap-3 px-4 py-1.5 text-[11px]">
                  <span className="w-32 shrink-0 text-slate-500">
                    {new Date(point.t).toLocaleString("ru-RU", {
                      day: "2-digit",
                      month: "2-digit",
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                  </span>
                  <div className="flex flex-wrap gap-3">
                    {point.drives.map((drive) => (
                      <span key={drive.letter} className="tabular-nums text-slate-300">
                        {drive.letter}: <span className="text-slate-400">{formatBytes(drive.used, 1)}</span>
                      </span>
                    ))}
                  </div>
                </div>
              ))}
          </div>
        </Card>
      ) : null}

      <Card className="overflow-hidden">
        <div className="flex items-center justify-between border-b border-white/[0.08] px-4 py-2.5">
          <p className="text-[13px] font-medium text-white">{t("Последние очистки")}</p>
          <span className="text-[10px] text-slate-500">{cleanHistory.length} {t("записей")}</span>
        </div>
        {cleanHistory.length === 0 ? (
          <p className="px-4 py-3 text-[11px] text-slate-500">{t("Пока нет записей.")}</p>
        ) : (
          <div className="divide-y divide-white/[0.05]">
            {cleanHistory.slice(0, 20).map((entry, index) => (
              <div key={`${entry.at}-${index}`} className="flex flex-wrap items-center gap-3 px-4 py-2 text-[11px]">
                <span className="w-32 shrink-0 text-slate-500">
                  {new Date(entry.at).toLocaleString("ru-RU", {
                    day: "2-digit",
                    month: "2-digit",
                    hour: "2-digit",
                    minute: "2-digit",
                  })}
                </span>
                <span className="rounded-md border border-white/10 bg-white/[0.04] px-1.5 py-0.5 text-slate-400">
                  {entry.action}
                </span>
                <span className="tabular-nums text-slate-200">{formatBytes(entry.bytes)}</span>
                <span className="text-slate-500">{entry.files} {t("объектов")}</span>
                <span className="text-slate-600">{entry.trigger}</span>
                {entry.errors.length > 0 ? (
                  <span className="text-amber-300/80">{entry.errors.length} {t("ошибок")}</span>
                ) : null}
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}
