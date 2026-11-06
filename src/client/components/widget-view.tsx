import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

type ResizeDirection =
  | "East"
  | "North"
  | "NorthEast"
  | "NorthWest"
  | "South"
  | "SouthEast"
  | "SouthWest"
  | "West";
import type { DriveInfo, MetricsSample, ProcessInfo, WidgetSettings } from "@/shared/types";
import { formatBytes } from "@/client/lib/format";

type WidgetData = {
  settings: WidgetSettings;
  latest: MetricsSample | null;
  drives: DriveInfo[];
  top: ProcessInfo | null;
};

type Menu = { x: number; y: number } | null;

function percent(value: number) {
  return Math.max(0, Math.min(100, Math.round(value)));
}

function MetricRow({
  label,
  value,
  percentValue,
  accent,
  bars,
}: {
  label: string;
  value: string;
  percentValue?: number;
  accent: string;
  bars: boolean;
}) {
  return (
    <div className="flex items-center gap-2 py-[2px]">
      <span className="w-12 shrink-0 text-[0.72em] uppercase tracking-wide text-white/45">{label}</span>
      {bars && percentValue !== undefined ? (
        <span className="h-[4px] min-w-0 flex-1 overflow-hidden rounded-full bg-white/10">
          <span
            className="block h-full rounded-full transition-[width] duration-500"
            style={{ width: `${percent(percentValue)}%`, background: accent }}
          />
        </span>
      ) : (
        <span className="min-w-0 flex-1" />
      )}
      <span className="shrink-0 tabular-nums text-white/90">{value}</span>
    </div>
  );
}

export function WidgetView() {
  const [data, setData] = useState<WidgetData | null>(null);
  const [menu, setMenu] = useState<Menu>(null);

  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | null = null;

    const tick = async () => {
      let interval = 1500;
      try {
        const next = await invoke<WidgetData>("widget_data");
        if (!cancelled) setData(next);
        interval = next.settings.intervalMs;
      } catch {
        /* app is closing */
      }
      if (!cancelled) {
        timer = setTimeout(tick, Math.max(500, interval));
      }
    };

    void tick();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, []);

  useEffect(() => {
    const hide = () => setMenu(null);
    window.addEventListener("click", hide);
    return () => window.removeEventListener("click", hide);
  }, []);

  if (!data) return null;

  const { settings, latest, drives, top } = data;
  const totalFree = drives.reduce((sum, drive) => sum + drive.free, 0);
  const totalSpace = drives.reduce((sum, drive) => sum + drive.total, 0);

  const ramPercent =
    latest && latest.ramTotal > 0 ? (latest.ramUsed / latest.ramTotal) * 100 : 0;
  const diskRate = latest ? latest.diskRead + latest.diskWrite : 0;
  const netRate = latest ? latest.netIn + latest.netOut : 0;

  async function action(command: string) {
    setMenu(null);
    await invoke(command).catch(() => undefined);
  }

  function handleResize(direction: ResizeDirection) {
    void getCurrentWindow().startResizeDragging(direction);
  }

  return (
    <div
      className="relative h-screen w-screen select-none overflow-hidden text-white"
      style={{
        background: `rgba(7, 11, 18, ${settings.opacity})`,
        border: `1px solid ${settings.accent}44`,
        borderRadius: settings.radius,
        fontSize: `${14 * settings.fontScale}px`,
        boxShadow: "0 12px 40px -18px rgba(0,0,0,0.85)",
      }}
      onContextMenu={(event) => {
        event.preventDefault();
        setMenu({ x: event.clientX, y: event.clientY });
      }}
    >
      {/* drag strip */}
      <div data-tauri-drag-region className="h-5 w-full cursor-move" />

      <div className="px-3 pb-3">
        {settings.showTitle ? (
          <div className="mb-1 flex items-center gap-1.5" data-tauri-drag-region>
            <span
              className="size-1.5 rounded-full"
              style={{ background: settings.accent }}
            />
            <span className="text-[0.7em] font-semibold tracking-[0.16em] text-white/60">
              {settings.title}
            </span>
          </div>
        ) : null}

        {latest ? (
          <div className="space-y-0">
            {settings.showCpu ? (
              <MetricRow
                label="CPU"
                value={`${percent(latest.cpu)}%`}
                percentValue={latest.cpu}
                accent={settings.accent}
                bars={settings.bars}
              />
            ) : null}
            {settings.showRam ? (
              <MetricRow
                label="ОЗУ"
                value={formatBytes(latest.ramUsed, 1)}
                percentValue={ramPercent}
                accent={settings.accent}
                bars={settings.bars}
              />
            ) : null}
            {settings.showDisk ? (
              <MetricRow
                label="Диск"
                value={`${formatBytes(diskRate, 0)}/с`}
                percentValue={Math.min(100, (diskRate / (50 * 1024 * 1024)) * 100)}
                accent={settings.accent}
                bars={settings.bars}
              />
            ) : null}
            {settings.showNet ? (
              <MetricRow
                label="Сеть"
                value={`${formatBytes(netRate, 0)}/с`}
                percentValue={Math.min(100, (netRate / (10 * 1024 * 1024)) * 100)}
                accent={settings.accent}
                bars={settings.bars}
              />
            ) : null}
            {settings.showDrives ? (
              <MetricRow
                label="Свободно"
                value={formatBytes(totalFree, 0)}
                percentValue={totalSpace > 0 ? (totalFree / totalSpace) * 100 : 0}
                accent={settings.accent}
                bars={settings.bars}
              />
            ) : null}
            {settings.showTopProcess && top ? (
              <div className="mt-1 truncate text-[0.7em] text-white/45">
                {top.name} · {formatBytes(top.memory, 0)}
              </div>
            ) : null}
          </div>
        ) : (
          <div className="py-2 text-[0.75em] text-white/40">Собираем данные…</div>
        )}
      </div>

      {/* resize handles */}
      <div className="absolute inset-y-0 left-0 w-1.5 cursor-w-resize" onMouseDown={() => handleResize("West")} />
      <div className="absolute inset-y-0 right-0 w-1.5 cursor-e-resize" onMouseDown={() => handleResize("East")} />
      <div className="absolute inset-x-0 top-0 h-1.5 cursor-n-resize" onMouseDown={() => handleResize("North")} />
      <div className="absolute inset-x-0 bottom-0 h-1.5 cursor-s-resize" onMouseDown={() => handleResize("South")} />
      <div className="absolute left-0 top-0 size-2 cursor-nw-resize" onMouseDown={() => handleResize("NorthWest")} />
      <div className="absolute right-0 top-0 size-2 cursor-ne-resize" onMouseDown={() => handleResize("NorthEast")} />
      <div className="absolute bottom-0 left-0 size-2 cursor-sw-resize" onMouseDown={() => handleResize("SouthWest")} />
      <div className="absolute bottom-0 right-0 size-2 cursor-se-resize" onMouseDown={() => handleResize("SouthEast")} />

      {menu ? (
        <div
          className="absolute z-10 w-48 overflow-hidden rounded-lg border border-white/10 bg-[#101725] py-1 text-[12px] shadow-xl"
          style={{
            left: Math.max(4, Math.min(menu.x, window.innerWidth - 200)),
            top: Math.max(4, Math.min(menu.y, window.innerHeight - 120)),
          }}
        >
          <button className="block w-full px-3 py-1.5 text-left text-slate-200 hover:bg-white/10" onClick={() => void action("widget_hide")}>
            Скрыть виджет
          </button>
          <button className="block w-full px-3 py-1.5 text-left text-slate-200 hover:bg-white/10" onClick={() => void action("show_main")}>
            Открыть Диск-Мастер
          </button>
          <button className="block w-full px-3 py-1.5 text-left text-rose-300 hover:bg-rose-500/15" onClick={() => void action("widget_stop")}>
            Закрыть виджет
          </button>
        </div>
      ) : null}
    </div>
  );
}
