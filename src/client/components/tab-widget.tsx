import { useState } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { WidgetSettings } from "@/shared/types";
import { queryClient } from "@/client/rpc-client";
import { Badge, Button, Card, SectionTitle } from "./ui";
import { AlertIcon, CheckIcon, RefreshIcon, SpinnerIcon, SparkleIcon } from "./icons";

const ACCENTS = ["#38e0a0", "#38bdf8", "#a78bfa", "#fbbf24", "#fb7185", "#f472b6"];
const PRESETS = [
  { label: "Компактный", width: 260, height: 150, fontScale: 0.9, bars: false, showTitle: true },
  { label: "Стандарт", width: 320, height: 210, fontScale: 1, bars: true, showTitle: true },
  { label: "Подробный", width: 360, height: 290, fontScale: 1.05, bars: true, showTitle: true },
];

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center justify-between gap-3 border-b border-white/[0.06] py-2.5 last:border-0">
      <div className="min-w-0">
        <p className="text-[13px] text-slate-200">{label}</p>
        {hint ? <p className="mt-0.5 text-[11px] leading-relaxed text-slate-500">{hint}</p> : null}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

function Toggle({
  checked,
  onChange,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 shrink-0 rounded-full transition-colors disabled:opacity-40 ${
        checked ? "bg-emerald-500" : "bg-white/15"
      }`}
    >
      <span
        className={`absolute top-0.5 size-5 rounded-full bg-white transition-[left] ${
          checked ? "left-[22px]" : "left-0.5"
        }`}
      />
    </button>
  );
}

function Slider({
  value,
  min,
  max,
  step = 1,
  onChange,
  suffix,
}: {
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (value: number) => void;
  suffix?: string;
}) {
  return (
    <div className="flex items-center gap-2.5">
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(event) => onChange(Number(event.target.value))}
        className="h-1.5 w-32 cursor-pointer appearance-none rounded-full bg-white/15 accent-emerald-500"
      />
      <span className="w-14 text-right text-[11px] tabular-nums text-slate-400">
        {value}
        {suffix ?? ""}
      </span>
    </div>
  );
}

export function WidgetTab() {
  const [draft, setDraft] = useState<WidgetSettings | null>(null);
  const [savedFlag, setSavedFlag] = useState(false);
  const [draftSource, setDraftSource] = useState<WidgetSettings | null>(null);

  const infoQuery = useQuery({
    ...queryClient.system.widgetInfo.queryOptions(),
    refetchInterval: 4000,
  });

  const startMutation = useMutation(queryClient.system.widgetStart.mutationOptions());
  const stopMutation = useMutation(queryClient.system.widgetStop.mutationOptions());
  const saveMutation = useMutation(queryClient.system.widgetSave.mutationOptions());
  const autostartMutation = useMutation(queryClient.system.widgetAutostart.mutationOptions());

  const status = infoQuery.data;

  // Adopt the persisted settings the first time they arrive.
  if (status?.settings && status.settings !== draftSource) {
    setDraftSource(status.settings);
    if (!draft) setDraft(status.settings);
  }

  const settings = draft ?? status?.settings ?? null;

  /**
   * The widget writes its own x/y when dragged, so the form must not echo a
   * stale position back and snap it to the corner.
   */
  function withoutPosition(value: WidgetSettings): Partial<WidgetSettings> {
    const { x: _x, y: _y, ...rest } = value;
    return rest;
  }

  function patch(next: Partial<WidgetSettings>) {
    setDraft((current) => (current ? { ...current, ...next } : current));
    setSavedFlag(false);
  }

  async function apply(patchValue?: Partial<WidgetSettings>) {
    if (!settings) return;
    const merged = { ...settings, ...(patchValue ?? {}) };
    setDraft(merged);
    await saveMutation.mutateAsync(withoutPosition(merged) as never);
    setSavedFlag(true);
    if (status?.running) {
      await stopMutation.mutateAsync({});
      await startMutation.mutateAsync(withoutPosition(merged) as never);
    }
    await infoQuery.refetch();
  }

  async function toggleRunning(next: boolean) {
    if (next) {
      await startMutation.mutateAsync((settings ? withoutPosition(settings) : {}) as never);
    } else {
      await stopMutation.mutateAsync({});
    }
    await infoQuery.refetch();
  }

  /** Moves the widget back to the default corner (right top of the work area). */
  async function resetPosition() {
    if (!settings) return;
    const merged = { ...settings, x: -1, y: -1 };
    setDraft(merged);
    await saveMutation.mutateAsync(merged as never);
    await stopMutation.mutateAsync({});
    await startMutation.mutateAsync(merged as never);
    await infoQuery.refetch();
  }

  const running = Boolean(status?.running);
  const busy = startMutation.isPending || stopMutation.isPending || saveMutation.isPending;

  return (
    <div className="space-y-5">
      <SectionTitle
        title="Виджет на рабочем столе"
        subtitle="Отдельное окно поверх обоев: двигайте мышью, меняйте размер за угол, всё настраивается"
        action={
          <div className="flex items-center gap-2">
            {running ? <Badge variant="success">запущен</Badge> : <Badge variant="neutral">остановлен</Badge>}
            {running ? (
              <Button variant="secondary" onClick={resetPosition} loading={busy} title="Вернуть виджет в правый верхний угол">
                <RefreshIcon className="size-4" />
                На экран
              </Button>
            ) : null}
            <Button
              variant={running ? "secondary" : "primary"}
              onClick={() => void toggleRunning(!running)}
              loading={busy}
            >
              <SparkleIcon className="size-4" />
              {running ? "Остановить" : "Запустить виджет"}
            </Button>
          </div>
        }
      />

      {running ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-3.5">
          <div className="flex items-start gap-2.5">
            <CheckIcon className="mt-0.5 size-4 shrink-0 text-emerald-300" />
            <div className="min-w-0 text-[11px] leading-relaxed text-emerald-100/90">
              <p className="text-xs font-medium text-emerald-100">Виджет показан на рабочем столе</p>
              <p className="mt-1 text-emerald-200/70">
                Ищите панель в правом верхнем углу экрана — рядом с часами. Перетаскивайте её
                левой кнопкой мыши за любое место, размер меняйте за угол в правом нижнем углу.
                {settings?.zMode === "desktop"
                  ? " Сейчас выбран режим «Под окнами»: виджет лежит над обоями и скрывается за развёрнутыми окнами — сверните их, чтобы увидеть."
                  : ""}
              </p>
              <p className="mt-1 text-emerald-200/60">
                Не видно? Нажмите «На экран» — виджет вернётся в угол. Правый клик по виджету
                открывает меню: скрыть, свернуть в трей, закрыть.
              </p>
            </div>
          </div>
        </Card>
      ) : null}

      {startMutation.data && !startMutation.data.ok ? (
        <Card className="border-rose-400/30 bg-rose-500/[0.08] p-3.5">
          <p className="flex items-center gap-2 text-xs text-rose-200">
            <AlertIcon className="size-3.5" /> {startMutation.data.error}
          </p>
        </Card>
      ) : null}

      {!settings ? (
        <div className="flex items-center gap-2 text-xs text-slate-400">
          <SpinnerIcon className="size-3.5" /> Читаем настройки…
        </div>
      ) : (
        <div className="grid gap-4 lg:grid-cols-[1.25fr_1fr]">
          <div className="space-y-4">
            <Card className="p-4">
              <div className="mb-1 flex items-center gap-2">
                <h3 className="text-sm font-medium text-white">Внешний вид</h3>
                {savedFlag ? (
                  <span className="ml-auto flex items-center gap-1 text-[10px] text-emerald-300/80">
                    <CheckIcon className="size-3" /> сохранено
                  </span>
                ) : null}
              </div>

              <Row label="Прозрачность" hint="Насколько виджет пропускает обои и окна">
                <Slider
                  value={Math.round(settings.opacity * 100)}
                  min={20}
                  max={100}
                  onChange={(value) => patch({ opacity: value / 100 })}
                  suffix="%"
                />
              </Row>

              <Row label="Прозрачность кликов" hint="Клики проходят сквозь виджет — как у настоящего виджета">
                <Toggle
                  checked={settings.clickThrough}
                  onChange={(value) => patch({ clickThrough: value })}
                />
              </Row>

              <Row label="Масштаб текста" hint="Размер подписей и цифр">
                <Slider
                  value={Number((settings.fontScale * 100).toFixed(0))}
                  min={70}
                  max={160}
                  step={5}
                  onChange={(value) => patch({ fontScale: value / 100 })}
                  suffix="%"
                />
              </Row>

              <Row label="Скругление углов">
                <Slider
                  value={settings.radius}
                  min={0}
                  max={28}
                  onChange={(value) => patch({ radius: value })}
                  suffix=" px"
                />
              </Row>

              <Row label="Акцентный цвет">
                <div className="flex gap-1.5">
                  {ACCENTS.map((color) => (
                    <button
                      key={color}
                      onClick={() => patch({ accent: color })}
                      className={`size-6 rounded-full ring-2 transition-transform hover:scale-110 ${
                        settings.accent === color ? "ring-white/80" : "ring-transparent"
                      }`}
                      style={{ backgroundColor: color }}
                      title={color}
                    />
                  ))}
                </div>
              </Row>

              <Row label="Размер" hint={`${settings.width} × ${settings.height} px — можно менять и мышью`}>
                <Slider
                  value={settings.width}
                  min={220}
                  max={560}
                  step={10}
                  onChange={(value) => patch({ width: value })}
                  suffix=" px"
                />
              </Row>

              <Row label="Поведение окон" hint="Выберите, как виджет ведёт себя относительно других окон">
                <div className="flex overflow-hidden rounded-lg border border-white/10">
                  {(
                    [
                      ["desktop", "Под окнами"],
                      ["normal", "Обычный"],
                      ["top", "Поверх всех"],
                    ] as [WidgetSettings["zMode"], string][]
                  ).map(([mode, label]) => (
                    <button
                      key={mode}
                      onClick={() => patch({ zMode: mode })}
                      className={`px-2.5 py-1.5 text-[11px] transition-colors ${
                        settings.zMode === mode
                          ? "bg-emerald-500/20 text-emerald-200"
                          : "bg-white/[0.03] text-slate-400 hover:bg-white/[0.07]"
                      }`}
                    >
                      {label}
                    </button>
                  ))}
                </div>
              </Row>

              <Row label="Полосы индикаторов" hint="Горизонтальная шкала загрузки рядом с каждой метрикой">
                <Toggle checked={settings.bars} onChange={(value) => patch({ bars: value })} />
              </Row>
            </Card>

            <Card className="p-4">
              <h3 className="mb-1 text-sm font-medium text-white">Метрики</h3>
              {(
                [
                  ["showCpu", "Процессор"],
                  ["showRam", "Память"],
                  ["showDisk", "Диск (чтение/запись)"],
                  ["showNet", "Сеть (входящий/исходящий)"],
                  ["showDrives", "Свободное место на диске"],
                  ["showTopProcess", "Самый тяжёлый процесс"],
                  ["showTitle", "Заголовок"],
                ] as [keyof WidgetSettings, string][]
              ).map(([key, label]) => (
                <Row key={key} label={label}>
                  <Toggle
                    checked={Boolean(settings[key])}
                    onChange={(value) => patch({ [key]: value } as Partial<WidgetSettings>)}
                  />
                </Row>
              ))}

              <Row label="Интервал обновления">
                <Slider
                  value={settings.intervalMs}
                  min={500}
                  max={5000}
                  step={100}
                  onChange={(value) => patch({ intervalMs: value })}
                  suffix=" мс"
                />
              </Row>
            </Card>
          </div>

          <div className="space-y-4">
            <Card className="p-4">
              <h3 className="mb-3 text-sm font-medium text-white">Предпросмотр</h3>
              <div className="flex justify-center rounded-xl bg-[radial-gradient(circle_at_30%_20%,var(--preview-from),var(--preview-to)_70%)] p-4">
                <div
                  className="w-full max-w-[300px] overflow-hidden border p-3 shadow-2xl"
                  style={{
                    borderRadius: settings.radius,
                    borderColor: `${settings.accent}47`,
                    backgroundColor: `rgba(11,18,32,${settings.opacity})`,
                  }}
                >
                  {settings.showTitle ? (
                    <p
                      className="mb-1.5 font-semibold tracking-wide"
                      style={{ color: settings.accent, fontSize: 10 * settings.fontScale }}
                    >
                      {settings.title}
                    </p>
                  ) : null}
                  {[
                    settings.showCpu ? ["CPU", "18%", 22] : null,
                    settings.showRam ? ["ОЗУ", "14,7 ГБ", 47] : null,
                    settings.showDisk ? ["ДИСК", "R 0 Б W 2 МБ", 8] : null,
                    settings.showNet ? ["СЕТЬ", "↓ 24 КБ ↑ 3 КБ", 12] : null,
                    settings.showDrives ? ["СВОБОДНО", "8,8 ГБ", 96] : null,
                    settings.showTopProcess ? ["ТОП", "java 3,4 ГБ", 30] : null,
                  ]
                    .filter(Boolean)
                    .map((row) => {
                      const [label, value, bar] = row as [string, string, number];
                      return (
                        <div key={label} className="flex items-center gap-2 py-0.5">
                          <span
                            className="w-10 shrink-0"
                            style={{ fontSize: 8.5 * settings.fontScale, color: "#a6b3c4" }}
                          >
                            {label}
                          </span>
                          {settings.bars ? (
                            <div
                              className="h-2.5 flex-1 overflow-hidden rounded-sm"
                              style={{ backgroundColor: "rgba(255,255,255,0.08)" }}
                            >
                              <div
                                className="h-full rounded-sm"
                                style={{
                                  width: `${bar}%`,
                                  backgroundColor: settings.accent,
                                  opacity: 0.75,
                                }}
                              />
                            </div>
                          ) : null}
                          <span
                            className="shrink-0 text-right font-semibold"
                            style={{ fontSize: 9.5 * settings.fontScale, color: "#e9eff7" }}
                          >
                            {value}
                          </span>
                        </div>
                      );
                    })}
                </div>
              </div>
              <p className="mt-2 text-center text-[10px] leading-relaxed text-slate-500">
                Предпросмотр приблизительный — точный вид задаёт окно WPF на рабочем столе
              </p>
            </Card>

            <Card className="p-4">
              <h3 className="mb-2 text-sm font-medium text-white">Пресеты</h3>
              <div className="space-y-1.5">
                {PRESETS.map((preset) => (
                  <button
                    key={preset.label}
                    onClick={() =>
                      void apply({
                        width: preset.width,
                        height: preset.height,
                        fontScale: preset.fontScale,
                        bars: preset.bars,
                        showTitle: preset.showTitle,
                      })
                    }
                    className="flex w-full items-center justify-between rounded-lg border border-white/[0.07] bg-white/[0.02] px-3 py-2 text-left text-xs text-slate-300 transition-colors hover:bg-white/[0.06]"
                  >
                    <span>{preset.label}</span>
                    <span className="text-[10px] text-slate-500">
                      {preset.width}×{preset.height}
                    </span>
                  </button>
                ))}
              </div>
            </Card>

            <Card className="p-4">
              <h3 className="mb-2 text-sm font-medium text-white">Автозапуск</h3>
              <Row label="Запускать вместе с Windows" hint="Добавляет запись в HKCU\...\Run">
                <Toggle
                  checked={Boolean(status?.autostart)}
                  disabled={autostartMutation.isPending}
                  onChange={async (value) => {
                    await autostartMutation.mutateAsync({ enabled: value });
                    await infoQuery.refetch();
                  }}
                />
              </Row>
              <div className="mt-2 flex gap-2">
                <Button variant="primary" size="sm" onClick={() => void apply()} loading={busy}>
                  Применить настройки
                </Button>
                <Button variant="ghost" size="sm" onClick={() => void infoQuery.refetch()}>
                  <RefreshIcon className="size-3.5" />
                  Перечитать
                </Button>
              </div>
              {status?.log ? (
                <pre className="mt-2 max-h-24 overflow-auto whitespace-pre-wrap rounded-lg bg-black/40 p-2 font-mono text-[10px] text-slate-500">
                  {status.log}
                </pre>
              ) : null}
            </Card>
          </div>
        </div>
      )}
    </div>
  );
}
