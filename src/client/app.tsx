import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { useMutation, useQuery } from "@tanstack/react-query";
import type { ScanState } from "@/shared/types";
import { api } from "./api";
import { queryClient } from "./rpc-client";
import { formatBytes } from "./lib/format";
import { applyTheme, initTheme, type Theme } from "./lib/theme";
import {
  FilesIcon,
  GaugeIcon,
  MoonIcon,
  PackageIcon,
  RefreshIcon,
  SettingsIcon,
  ShieldIcon,
  SparkleIcon,
  SunIcon,
} from "./components/icons";
import { Badge } from "./components/ui";
import { OverviewTab } from "./components/tab-overview";
import { MonitorTab } from "./components/tab-monitor";
import { CleanupTab } from "./components/tab-cleanup";
import { LargeFilesTab } from "./components/tab-large-files";
import { FoldersTab } from "./components/tab-folders";
import { DuplicatesTab } from "./components/tab-duplicates";
import { ProgramsTab } from "./components/tab-programs";
import { UpdatesTab } from "./components/tab-updates";
import { StartupTab } from "./components/tab-startup";
import { ScheduleTab } from "./components/tab-schedule";
import { HistoryTab } from "./components/tab-history";
import { MaintenanceTab } from "./components/tab-maintenance";
import { SettingsTab } from "./components/tab-settings";
import { WidgetTab } from "./components/tab-widget";
import { UpdateBanner } from "./components/update-banner";
import { AboutModal } from "./components/about-modal";
import { useUpdater } from "./lib/updater";

type TabId =
  | "overview"
  | "monitor"
  | "cleanup"
  | "files"
  | "folders"
  | "duplicates"
  | "programs"
  | "updates"
  | "startup"
  | "schedule"
  | "history"
  | "maintenance"
  | "widget"
  | "settings";

type TabDef = {
  id: TabId;
  label: string;
  icon: typeof GaugeIcon;
  group: string;
};

const TABS: TabDef[] = [
  { id: "overview", label: "Обзор", icon: GaugeIcon, group: "Система" },
  { id: "monitor", label: "Мониторинг", icon: GaugeIcon, group: "Система" },
  { id: "history", label: "История места", icon: RefreshIcon, group: "Система" },
  { id: "maintenance", label: "Обслуживание", icon: ShieldIcon, group: "Система" },
  { id: "cleanup", label: "Очистка", icon: SparkleIcon, group: "Освободить место" },
  { id: "folders", label: "Анализ диска", icon: FilesIcon, group: "Освободить место" },
  { id: "files", label: "Большие файлы", icon: FilesIcon, group: "Освободить место" },
  { id: "duplicates", label: "Дубликаты", icon: FilesIcon, group: "Освободить место" },
  { id: "programs", label: "Программы", icon: PackageIcon, group: "Программы" },
  { id: "updates", label: "Обновления", icon: RefreshIcon, group: "Программы" },
  { id: "startup", label: "Автозагрузка", icon: SparkleIcon, group: "Программы" },
  { id: "schedule", label: "Расписание", icon: RefreshIcon, group: "Автоматизация" },
  { id: "widget", label: "Виджет", icon: GaugeIcon, group: "Автоматизация" },
  { id: "settings", label: "Настройки", icon: SettingsIcon, group: "Автоматизация" },
];

const GROUPS = ["Система", "Освободить место", "Программы", "Автоматизация"];

const CATEGORY_FALLBACK = [
  { id: "user-temp", name: "Временные файлы" },
  { id: "browser-cache", name: "Кэш браузеров" },
  { id: "win-temp", name: "Temp Windows" },
  { id: "win-update", name: "Кэш обновлений" },
  { id: "delivery-opt", name: "Оптимизация доставки" },
  { id: "prefetch", name: "Prefetch" },
  { id: "crash-dumps", name: "Дампы сбоев" },
  { id: "thumbnails", name: "Кэш эскизов" },
  { id: "inet-cache", name: "INetCache" },
  { id: "dx-cache", name: "Кэш DirectX" },
  { id: "win-logs", name: "Логи Windows" },
  { id: "recycle-bin", name: "Корзина" },
];

function App() {
  const [tab, setTab] = useState<TabId>("overview");
  const [jobId, setJobId] = useState<string | null>(null);
  const [theme, setTheme] = useState<Theme>(() => initTheme());
  const refreshedFor = useRef<string | null>(null);
  const [aboutOpen, setAboutOpen] = useState(false);
  const updater = useUpdater();

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen("open-settings", () => setTab("settings")).then((fn) => {
      unlisten = fn;
    });
    void api.initialTab().then((value) => {
      if (value && TABS.some((item) => item.id === value)) setTab(value as TabId);
    });
    return () => unlisten?.();
  }, []);

  function toggleTheme() {
    const next: Theme = theme === "dark" ? "light" : "dark";
    applyTheme(next);
    setTheme(next);
  }

  const overviewQuery = useQuery(queryClient.system.overview.queryOptions());

  const scanQuery = useQuery({
    ...queryClient.system.scanStatus.queryOptions({ input: { jobId: jobId ?? "" } }),
    refetchInterval: (query) => {
      const data = query.state.data as ScanState | null | undefined;
      if (!data || data.finishedAt) return false;
      return 800;
    },
  });

  const startScanMutation = useMutation(queryClient.system.startScan.mutationOptions());

  const scanJob = (scanQuery.data as ScanState | null | undefined) ?? null;
  const scanRunning = Boolean(scanJob && !scanJob.finishedAt) || startScanMutation.isPending;

  function startScan(thresholdMb?: number) {
    startScanMutation.mutate(
      { thresholdMb },
      {
        onSuccess: (job) => setJobId(job.jobId),
      },
    );
  }

  useEffect(() => {
    if (!scanJob?.finishedAt) return;
    if (refreshedFor.current === scanJob.jobId) return;
    refreshedFor.current = scanJob.jobId;
    void overviewQuery.refetch();
  }, [scanJob?.finishedAt, scanJob?.jobId, overviewQuery]);

  const info = overviewQuery.data;

  const categories =
    scanJob?.categories?.length
      ? scanJob.categories.map((category) => ({ id: category.id, name: category.name }))
      : CATEGORY_FALLBACK;

  const activeTab = TABS.find((item) => item.id === tab);

  function go(next: TabId) {
    setTab(next);
  }

  return (
    <div className="min-h-screen bg-[var(--app-bg)] text-slate-200">
      <div className="pointer-events-none fixed inset-x-0 top-0 h-72 bg-[radial-gradient(60%_100%_at_50%_0%,var(--app-glow),transparent_70%)]" />

      <div className="relative mx-auto flex min-h-screen w-full max-w-[1500px]">
        <aside className="sticky top-0 hidden h-screen w-60 shrink-0 flex-col border-r border-white/[0.07] px-3 py-5 lg:flex">
          <div className="flex items-center gap-2.5 px-2">
            <div className="grid size-9 place-items-center rounded-xl bg-gradient-to-br from-emerald-400 to-teal-500 text-emerald-950 shadow-[0_8px_24px_-10px_rgba(16,185,129,0.9)]">
              <GaugeIcon className="size-5" />
            </div>
            <div>
              <p className="text-sm font-semibold tracking-tight text-white">Диск-Мастер</p>
              <p className="text-[10px] text-slate-500">Windows · очистка и программы</p>
            </div>
          </div>

          <nav className="mt-5 flex-1 space-y-3 overflow-y-auto">
            {GROUPS.map((group) => (
              <div key={group}>
                <p className="px-2.5 pb-1 text-[9px] font-semibold uppercase tracking-[0.12em] text-slate-600">
                  {group}
                </p>
                <div className="flex flex-col gap-0.5">
                  {TABS.filter((item) => item.group === group).map((item) => {
                    const Icon = item.icon;
                    const active = tab === item.id;
                    return (
                      <button
                        key={item.id}
                        onClick={() => go(item.id)}
                        className={`flex items-center gap-2.5 rounded-xl px-2.5 py-1.5 text-left text-[13px] transition-colors ${
                          active
                            ? "bg-emerald-500/[0.12] text-emerald-200"
                            : "text-slate-400 hover:bg-white/[0.05] hover:text-slate-100"
                        }`}
                      >
                        <Icon className="size-4" />
                        {item.label}
                        {item.id === "cleanup" && scanJob && !scanJob.finishedAt ? (
                          <span className="ml-auto size-1.5 animate-pulse rounded-full bg-sky-400" />
                        ) : null}
                      </button>
                    );
                  })}
                </div>
              </div>
            ))}
          </nav>

          <div className="mt-3 space-y-2 px-1">
            {info ? (
              <div className="rounded-xl border border-white/[0.07] bg-white/[0.02] p-3">
                <div className="flex items-center gap-1.5 text-[10px] uppercase tracking-wide text-slate-500">
                  <ShieldIcon className="size-3" />
                  {info.admin ? "Администратор" : "Обычные права"}
                </div>
                <p className="mt-1 truncate font-mono text-[11px] text-slate-400">{info.machine}</p>
                <p className="mt-1 text-[11px] text-slate-500">
                  свободно{" "}
                  {formatBytes(info.drives.reduce((sum, drive) => sum + drive.free, 0))}
                </p>
              </div>
            ) : (
              <div className="h-20 animate-pulse rounded-xl bg-white/[0.03]" />
            )}

            <button
              onClick={() => setAboutOpen(true)}
              className="w-full rounded-xl border border-white/[0.07] bg-white/[0.02] px-3 py-2 text-left text-[11px] text-slate-400 transition-colors hover:bg-white/[0.05] hover:text-slate-200"
            >
              О программе · v{updater.version || "…"}
            </button>
          </div>
        </aside>

        <main className="min-w-0 flex-1">
          <header className="sticky top-0 z-20 border-b border-white/[0.07] bg-[var(--app-shell)] backdrop-blur">
            <div className="flex items-center gap-3 px-4 py-3 md:px-8">
              <div className="flex items-center gap-2 lg:hidden">
                <div className="grid size-8 place-items-center rounded-lg bg-gradient-to-br from-emerald-400 to-teal-500 text-emerald-950">
                  <GaugeIcon className="size-4" />
                </div>
                <span className="text-sm font-semibold text-white">Диск-Мастер</span>
              </div>

              <div className="hidden items-center gap-2 lg:flex">
                <h1 className="text-sm font-medium text-slate-300">{activeTab?.label}</h1>
                {info ? (
                  <Badge variant={info.admin ? "success" : "neutral"}>
                    {info.admin ? "администратор" : "без прав администратора"}
                  </Badge>
                ) : null}
              </div>

              <div className="ml-auto flex items-center gap-2">
                {updater.notice ? (
                  <span className="hidden max-w-[280px] truncate text-[11px] text-slate-400 sm:block">
                    {updater.notice}
                  </span>
                ) : null}
                <button
                  onClick={() => setAboutOpen(true)}
                  className="hidden items-center rounded-lg border border-white/10 bg-white/[0.03] px-2 py-1 text-[10px] font-medium tabular-nums text-slate-500 transition-colors hover:bg-white/[0.07] hover:text-slate-200 sm:flex"
                  title="О программе"
                >
                  v{updater.version || "…"}
                </button>
                <button
                  onClick={() => void updater.checkNow()}
                  disabled={updater.status === "checking" || updater.status === "downloading"}
                  className="grid size-8 place-items-center rounded-lg border border-white/10 bg-white/[0.04] text-slate-300 transition-colors hover:bg-white/10 hover:text-white disabled:opacity-40"
                  title={updater.status === "portable" ? "Открыть страницу релизов" : "Проверить обновления"}
                  aria-label="Проверить обновления"
                >
                  <RefreshIcon className={`size-4 ${updater.status === "checking" ? "animate-spin" : ""}`} />
                </button>
                {scanJob ? (
                  <span className="hidden text-[11px] text-slate-500 sm:block">
                    {scanJob.finishedAt
                      ? `Скан завершён · ${formatBytes(scanJob.totalJunkBytes)} мусора`
                      : `Сканирование ${scanJob.percent}%`}
                  </span>
                ) : null}
                <button
                  onClick={toggleTheme}
                  className="grid size-8 place-items-center rounded-lg border border-white/10 bg-white/[0.04] text-slate-300 transition-colors hover:bg-white/10 hover:text-white"
                  title={theme === "dark" ? "Светлая тема" : "Тёмная тема"}
                  aria-label={theme === "dark" ? "Включить светлую тему" : "Включить тёмную тему"}
                >
                  {theme === "dark" ? <SunIcon className="size-4" /> : <MoonIcon className="size-4" />}
                </button>
                <button
                  onClick={() => void overviewQuery.refetch()}
                  className="grid size-8 place-items-center rounded-lg border border-white/10 bg-white/[0.04] text-slate-300 transition-colors hover:bg-white/10 hover:text-white"
                  title="Обновить данные о дисках"
                >
                  <RefreshIcon className="size-4" />
                </button>
              </div>
            </div>

            <div className="flex gap-1 overflow-x-auto px-3 pb-2 lg:hidden">
              {TABS.map((item) => {
                const Icon = item.icon;
                const active = tab === item.id;
                return (
                  <button
                    key={item.id}
                    onClick={() => go(item.id)}
                    className={`flex shrink-0 items-center gap-1.5 rounded-lg px-2.5 py-1.5 text-xs transition-colors ${
                      active
                        ? "bg-emerald-500/15 text-emerald-200"
                        : "text-slate-400 hover:bg-white/[0.05]"
                    }`}
                  >
                    <Icon className="size-3.5" />
                    {item.label}
                  </button>
                );
              })}
            </div>
          </header>

          <div className="px-4 pb-12 pt-5 md:px-8">
            <UpdateBanner updater={updater} />

            {tab === "overview" ? (
              <OverviewTab
                info={info}
                loading={overviewQuery.isLoading}
                error={overviewQuery.isError}
                scanRunning={scanRunning}
                onScan={() => {
                  go("cleanup");
                  if (!scanJob || scanJob.finishedAt) startScan();
                }}
                onGoPrograms={() => go("programs")}
                onGoUpdates={() => go("updates")}
              />
            ) : null}

            {tab === "monitor" ? <MonitorTab /> : null}
            {tab === "history" ? <HistoryTab /> : null}
            {tab === "maintenance" ? <MaintenanceTab /> : null}

            {tab === "cleanup" ? (
              <CleanupTab
                job={scanJob}
                starting={startScanMutation.isPending}
                onStart={() => startScan()}
                onCleaned={() => void overviewQuery.refetch()}
                admin={Boolean(info?.admin)}
              />
            ) : null}

            {tab === "folders" ? <FoldersTab /> : null}

            {tab === "files" ? (
              <LargeFilesTab
                job={scanJob}
                starting={startScanMutation.isPending}
                onStart={(thresholdMb) => startScan(thresholdMb)}
                onDeleted={() => void overviewQuery.refetch()}
              />
            ) : null}

            {tab === "duplicates" ? <DuplicatesTab /> : null}
            {tab === "programs" ? <ProgramsTab admin={Boolean(info?.admin)} /> : null}
            {tab === "updates" ? <UpdatesTab /> : null}
            {tab === "startup" ? <StartupTab /> : null}
            {tab === "schedule" ? <ScheduleTab categories={categories} /> : null}
            {tab === "widget" ? <WidgetTab /> : null}
            {tab === "settings" ? <SettingsTab /> : null}
          </div>
        </main>
      </div>

      <AboutModal
        open={aboutOpen}
        onClose={() => setAboutOpen(false)}
        info={info}
        updater={updater}
      />
    </div>
  );
}

export default App;
