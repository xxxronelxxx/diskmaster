import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { ScheduleConfig, ScheduleFrequency } from "@/shared/types";
import { queryClient } from "@/client/rpc-client";
import { formatBytes, formatCount, formatDate } from "@/client/lib/format";
import { Badge, Button, Card, SectionTitle } from "./ui";
import { AlertIcon, CheckIcon, RefreshIcon, SparkleIcon } from "./icons";
import { currentLocale, t } from "@/client/i18n";

const DAYS = ["Вс", "Пн", "Вт", "Ср", "Чт", "Пт", "Сб"];
const EMPTY_RUNS: never[] = [];

const CATEGORY_LABELS: Record<string, string> = {
  "user-temp": "Временные файлы",
  "browser-cache": "Кэш браузеров",
  "win-temp": "Temp Windows",
  "win-update": "Кэш обновлений",
  "delivery-opt": "Оптимизация доставки",
  prefetch: "Prefetch",
  "crash-dumps": "Дампы сбоев",
  thumbnails: "Кэш эскизов",
  "inet-cache": "INetCache",
  "dx-cache": "Кэш DirectX",
  "win-logs": "Логи Windows",
  "recycle-bin": "Корзина",
};

export function ScheduleTab({ categories }: { categories: { id: string; name: string }[] }) {
  const [draft, setDraft] = useState<ScheduleConfig | null>(null);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const [draftSource, setDraftSource] = useState<ScheduleConfig | null>(null);

  const scheduleQuery = useQuery(queryClient.system.scheduleGet.queryOptions());
  const historyQuery = useQuery(queryClient.system.scheduleHistory.queryOptions());
  const saveMutation = useMutation(queryClient.system.scheduleSave.mutationOptions());
  const runMutation = useMutation(queryClient.system.scheduleRunNow.mutationOptions());

  const status = scheduleQuery.data;

  // Follow server config until the user edits it; after a save the server
  // value becomes the new baseline, which also drops stale local edits.
  if (status?.config && status.config !== draftSource) {
    setDraftSource(status.config);
    setDraft(status.config);
  }

  const config = draft ?? status?.config ?? null;

  function patch(next: Partial<ScheduleConfig>) {
    setDraft((current) => (current ? { ...current, ...next } : current));
  }

  async function save() {
    if (!config) return;
    const response = await saveMutation.mutateAsync({
      enabled: config.enabled,
      frequency: config.frequency,
      time: config.time,
      dayOfWeek: config.dayOfWeek,
      categories: config.categories,
      thresholdMb: config.thresholdMb,
    });
    setMessage(
      response.registered
        ? { ok: true, text: t("Расписание сохранено и задание создано в планировщике Windows") }
        : response.error
          ? { ok: false, text: response.error }
          : { ok: true, text: t("Расписание сохранено, автоочистка выключена") },
    );
    await scheduleQuery.refetch();
  }

  async function runNow() {
    const result = await runMutation.mutateAsync({});
    setMessage(
      result.errors.length > 0
        ? { ok: false, text: result.errors.slice(0, 2).join("; ") }
        : {
            ok: true,
            text: t("Очищено {size} · {files} объектов", {
              size: formatBytes(result.freedBytes),
              files: formatCount(result.files),
            }),
          },
    );
    await historyQuery.refetch();
  }

  const runs = historyQuery.data ?? EMPTY_RUNS;

  return (
    <div className="space-y-5">
      <SectionTitle
        title={t("Автоочистка по расписанию")}
        subtitle={t("Windows будет сам запускать очистку выбранных категорий через Планировщик заданий")}
        action={
          <div className="flex items-center gap-2">
            {status?.registered ? (
              <Badge variant="success">{t("задание создано")}</Badge>
            ) : (
              <Badge variant="neutral">{t("задание не создано")}</Badge>
            )}
            <Button variant="primary" onClick={save} loading={saveMutation.isPending}>
              <CheckIcon className="size-4" />
              {t("Сохранить")}
            </Button>
            <Button variant="secondary" onClick={runNow} loading={runMutation.isPending}>
              <SparkleIcon className="size-4" />
              {t("Запустить сейчас")}
            </Button>
          </div>
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

      {!config ? (
        <div className="h-40 animate-pulse rounded-2xl bg-white/[0.04]" />
      ) : (
        <div className="grid gap-4 lg:grid-cols-[1.2fr_1fr]">
          <Card className="p-4">
            <div className="flex items-center justify-between border-b border-white/[0.07] pb-3">
              <div>
                <p className="text-sm font-medium text-white">{t("Включить автоочистку")}</p>
                <p className="mt-0.5 text-[11px] text-slate-500">
                  {status?.nextRun
                    ? t("Следующий запуск: {date}", {
                        date: new Date(status.nextRun).toLocaleString(
                          currentLocale() === "en" ? "en-US" : "ru-RU",
                        ),
                      })
                    : t("Задание ещё не запланировано")}
                </p>
              </div>
              <button
                role="switch"
                aria-checked={config.enabled}
                onClick={() => patch({ enabled: !config.enabled })}
                className={`relative h-7 w-12 shrink-0 rounded-full transition-colors ${
                  config.enabled ? "bg-emerald-500" : "bg-white/15"
                }`}
              >
                <span
                  className={`absolute top-0.5 size-6 rounded-full bg-white transition-[left] ${
                    config.enabled ? "left-[24px]" : "left-0.5"
                  }`}
                />
              </button>
            </div>

            <div className="mt-3 space-y-3">
              <div>
                <p className="mb-1.5 text-[11px] uppercase tracking-wide text-slate-500">{t("Частота")}</p>
                <div className="flex flex-wrap gap-1.5">
                  {(
                    [
                      ["daily", t("Каждый день")],
                      ["weekly", t("По дням недели")],
                      ["monthly", t("Раз в месяц")],
                      ["logon", t("При входе")],
                    ] as [ScheduleFrequency, string][]
                  ).map(([key, label]) => (
                    <button
                      key={key}
                      onClick={() => patch({ frequency: key })}
                      className={`rounded-lg border px-2.5 py-1.5 text-xs transition-colors ${
                        config.frequency === key
                          ? "border-emerald-400/40 bg-emerald-500/15 text-emerald-200"
                          : "border-white/10 bg-white/[0.03] text-slate-300 hover:bg-white/[0.07]"
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </div>

              {config.frequency === "weekly" ? (
                <div>
                  <p className="mb-1.5 text-[11px] uppercase tracking-wide text-slate-500">{t("День недели")}</p>
                  <div className="flex gap-1">
                    {DAYS.map((day, index) => (
                      <button
                        key={day}
                        onClick={() => patch({ dayOfWeek: index })}
                        className={`size-8 rounded-lg text-[11px] transition-colors ${
                          config.dayOfWeek === index
                            ? "bg-emerald-500/20 text-emerald-200"
                            : "bg-white/[0.04] text-slate-400 hover:bg-white/[0.08]"
                        }`}
                      >
                        {t(day)}
                      </button>
                    ))}
                  </div>
                </div>
              ) : null}

              {config.frequency !== "logon" ? (
                <div>
                  <p className="mb-1.5 text-[11px] uppercase tracking-wide text-slate-500">{t("Время запуска")}</p>
                  <input
                    type="time"
                    value={config.time}
                    onChange={(event) => patch({ time: event.target.value })}
                    className="rounded-lg border border-white/10 bg-white/[0.04] px-3 py-1.5 text-sm text-slate-100 outline-none focus:border-emerald-400/40"
                  />
                </div>
              ) : null}

              <div>
                <p className="mb-1.5 text-[11px] uppercase tracking-wide text-slate-500">
                  {t("Что очищать (")}{config.categories.length})
                </p>
                <div className="grid max-h-56 gap-1 overflow-auto pr-1 sm:grid-cols-2">
                  {categories.map((category) => {
                    const active = config.categories.includes(category.id);
                    return (
                      <label
                        key={category.id}
                        className={`flex cursor-pointer items-center gap-2 rounded-lg border px-2.5 py-1.5 text-[11px] transition-colors ${
                          active
                            ? "border-emerald-400/30 bg-emerald-500/[0.08] text-emerald-100"
                            : "border-white/[0.07] bg-white/[0.02] text-slate-400 hover:bg-white/[0.05]"
                        }`}
                      >
                        <input
                          type="checkbox"
                          className="size-3.5 accent-emerald-500"
                          checked={active}
                          onChange={(event) =>
                            patch({
                              categories: event.target.checked
                                ? [...config.categories, category.id]
                                : config.categories.filter((id) => id !== category.id),
                            })
                          }
                        />
                        <span className="truncate">{t(CATEGORY_LABELS[category.id] ?? category.name)}</span>
                      </label>
                    );
                  })}
                </div>
              </div>
            </div>
          </Card>

          <div className="space-y-4">
            <Card className="p-4">
              <div className="mb-2 flex items-center justify-between">
                <h3 className="text-sm font-medium text-white">{t("Журнал запусков")}</h3>
                <button
                  onClick={() => void historyQuery.refetch()}
                  className="text-[11px] text-slate-500 hover:text-slate-200"
                >
                  <RefreshIcon className="size-3.5" />
                </button>
              </div>
              {runs.length === 0 ? (
                <p className="text-[11px] leading-relaxed text-slate-500">
                  {t("Пока нет запусков. Первый отчёт появится после срабатывания расписания (или ручного запуска из раздела «Очистка»).")}
                </p>
              ) : (
                <div className="space-y-1.5">
                  {runs.map((run) => (
                    <div
                      key={run.at}
                      className="flex items-center justify-between rounded-lg border border-white/[0.06] bg-white/[0.02] px-2.5 py-1.5 text-[11px]"
                    >
                      <div>
                        <p className="text-slate-200">{formatDate(run.at)}</p>
                        <p className="text-[10px] text-slate-600">
                          {formatCount(run.categories.length)} {t("категорий ·")}{" "}
                          {run.trigger === "manual" ? t("вручную") : t("по расписанию")}
                        </p>
                      </div>
                      <span className="tabular-nums text-emerald-300">{formatBytes(run.freedBytes)}</span>
                    </div>
                  ))}
                </div>
              )}
            </Card>

            <Card className="p-4">
              <h3 className="flex items-center gap-2 text-sm font-medium text-white">
                <SparkleIcon className="size-4 text-emerald-300" />
                {t("Как это работает")}
              </h3>
              <ul className="mt-2 space-y-1.5 text-[11px] leading-relaxed text-slate-400">
                <li>{t("· Создаётся задание Windows «DiskMasterAutoClean» в папке \\DiskMaster\\")}</li>
                <li>{t("· Запуск пропускается, если компьютер выключен, и выполняется при включении")}</li>
                <li>{t("· Отчёт о каждом запуске пишется в журнал и виден здесь")}</li>
                <li>{t("· Для изменения задания нужны права администратора")}</li>
              </ul>
            </Card>
          </div>
        </div>
      )}
    </div>
  );
}
