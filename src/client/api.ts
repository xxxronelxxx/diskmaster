import { invoke } from "@tauri-apps/api/core";
import type {
  CleanResult,
  DeleteResult,
  DuplicateProgress,
  DuplicateScan,
  FolderAnalysis,
  HistorySummary,
  InstalledProgram,
  MetricsState,
  ProcessInfo,
  RecycleResult,
  ScanState,
  ScheduleConfig,
  ScheduleRun,
  ScheduleStatus,
  StartupItem,
  SystemInfo,
  UninstallResult,
  WidgetSettings,
  WidgetStatus,
  WingetListing,
} from "@/shared/types";

/**
 * Thin typed bridge over Tauri IPC. The backend lives in `src-tauri` and every
 * command mirrors the data contract in `src/shared/types.ts`.
 */

type QueryOptions<T> = { queryKey: readonly unknown[]; queryFn: () => Promise<T> };
type MutationOptions<I, O> = { mutationKey: readonly unknown[]; mutationFn: (input: I) => Promise<O> };

/** Drops empty inputs so Rust commands can declare `Option<T>` parameters. */
function payload(input: unknown): { input: unknown } | undefined {
  if (input === undefined || input === null) return undefined;
  if (typeof input === "object" && Object.keys(input as object).length === 0) return undefined;
  return { input };
}

function query<T>(cmd: string, opts?: { input?: unknown }): QueryOptions<T> {
  return {
    queryKey: opts?.input === undefined ? [cmd] : [cmd, opts.input],
    queryFn: () => invoke<T>(cmd, payload(opts?.input)),
  };
}

function mutation<I, O>(cmd: string): () => MutationOptions<I, O> {
  return () => ({
    mutationKey: [cmd],
    mutationFn: (input: I) => invoke<O>(cmd, payload(input)),
  });
}

export const api = {
  overview: () => invoke<SystemInfo>("overview"),
  metrics: (input?: { processes?: boolean }) => invoke<MetricsState & { processes: ProcessInfo[] }>("metrics", payload(input)),
  killProcess: (input: { pid: number }) => invoke<{ ok: boolean; error?: string }>("kill_process", { input }),

  startScan: (input?: { thresholdMb?: number }) => invoke<ScanState>("start_scan", payload(input)),
  scanStatus: (input: { jobId: string }) => invoke<ScanState | null>("scan_status", { input }),
  cleanJunk: (input: { categories: string[] }) => invoke<CleanResult>("clean_junk", { input }),
  deleteLargeFiles: (input: { paths: string[] }) => invoke<RecycleResult>("delete_large_files", { input }),
  reveal: (input: { path: string }) => invoke<{ ok: boolean; error?: string }>("reveal", { input }),

  folderRoots: () => invoke<{ label: string; path: string }[]>("folder_roots"),
  folderAnalyze: (input?: { path?: string }) => invoke<FolderAnalysis>("folder_analyze", payload(input)),
  folderDelete: (input: { paths: string[]; permanent?: boolean }) => invoke<DeleteResult>("folder_delete", { input }),

  duplicatesScan: (input?: { root?: string; minSizeKb?: number }) => invoke<DuplicateScan>("duplicates_scan", payload(input)),
  duplicatesLast: () => invoke<DuplicateScan | null>("duplicates_last"),
  duplicatesStatus: () => invoke<DuplicateProgress | null>("duplicates_status"),
  duplicatesCancel: () => invoke<void>("duplicates_cancel"),

  programs: (input?: { refresh?: boolean }) => invoke<InstalledProgram[]>("programs", payload(input)),
  uninstall: (input: { key: string; quiet?: boolean }) => invoke<UninstallResult>("uninstall", { input }),

  updates: (input?: { refresh?: boolean }) => invoke<WingetListing>("updates", payload(input)),
  upgrade: (input: { id: string }) => invoke<{ ok: boolean; output: string }>("upgrade", { input }),
  upgradeAll: () => invoke<{ ok: boolean; output: string }>("upgrade_all"),

  startupList: () => invoke<StartupItem[]>("startup_list"),
  startupToggle: (input: { id: string; enabled: boolean }) =>
    invoke<{ ok: boolean; error?: string }>("startup_toggle", { input }),
  startupDelete: (input: { id: string }) => invoke<{ ok: boolean; error?: string }>("startup_delete", { input }),

  scheduleGet: () => invoke<ScheduleStatus>("schedule_get"),
  scheduleSave: (input: Omit<ScheduleConfig, "lastRunAt" | "lastFreedBytes">) =>
    invoke<{ config: ScheduleConfig; registered: boolean; error?: string }>("schedule_save", { input }),
  scheduleHistory: () => invoke<ScheduleRun[]>("schedule_history"),
  scheduleRunNow: () => invoke<{ freedBytes: number; files: number; categories: string[]; errors: string[] }>("schedule_run_now"),

  historyGet: () => invoke<HistorySummary>("history_get"),
  historyCapture: () => invoke<HistorySummary & { snapshot: unknown }>("history_capture"),

  widgetInfo: () => invoke<WidgetStatus>("widget_info"),
  widgetStart: (input?: Partial<WidgetSettings>) => invoke<{ ok: boolean; error?: string }>("widget_start", payload(input)),
  widgetStop: () => invoke<{ ok: boolean }>("widget_stop"),
  widgetSave: (input: Partial<WidgetSettings>) => invoke<WidgetSettings>("widget_save", { input }),
  widgetAutostart: (input: { enabled: boolean }) => invoke<{ ok: boolean }>("widget_autostart", { input }),
};

/**
 * Compatibility facade with the same shape the UI used with oRPC, so the tab
 * components stay untouched.
 */
export const queryClient = {
  system: {
    overview: { queryOptions: () => query<SystemInfo>("overview") },
    metrics: {
      queryOptions: (opts?: { input?: { processes?: boolean } }) => query<MetricsState & { processes: ProcessInfo[] }>("metrics", opts),
      mutationOptions: mutation<never, never>("metrics"),
    },
    killProcess: { mutationOptions: mutation<{ pid: number }, { ok: boolean; error?: string }>("kill_process") },

    startScan: { mutationOptions: mutation<{ thresholdMb?: number } | undefined, ScanState>("start_scan") },
    scanStatus: {
      queryOptions: (opts?: { input?: { jobId: string } }) => query<ScanState | null>("scan_status", opts),
      mutationOptions: mutation<never, never>("scan_status"),
    },
    cleanJunk: { mutationOptions: mutation<{ categories: string[] }, CleanResult>("clean_junk") },
    deleteLargeFiles: { mutationOptions: mutation<{ paths: string[] }, RecycleResult>("delete_large_files") },
    reveal: (input: { path: string }) => api.reveal(input),

    folderRoots: { queryOptions: () => query<{ label: string; path: string }[]>("folder_roots") },
    folderAnalyze: {
      queryOptions: (opts?: { input?: { path?: string } }) => query<FolderAnalysis>("folder_analyze", opts),
      mutationOptions: mutation<{ path?: string } | undefined, FolderAnalysis>("folder_analyze"),
    },
    folderDelete: { mutationOptions: mutation<{ paths: string[]; permanent?: boolean }, DeleteResult>("folder_delete") },

    duplicatesScan: { mutationOptions: mutation<{ root?: string; minSizeKb?: number } | undefined, DuplicateScan>("duplicates_scan") },
    duplicatesLast: { queryOptions: () => query<DuplicateScan | null>("duplicates_last") },
    duplicatesStatus: { queryOptions: () => query<DuplicateProgress | null>("duplicates_status") },
    duplicatesCancel: { mutationOptions: mutation<Record<string, never> | undefined, void>("duplicates_cancel") },

    programs: { queryOptions: (opts?: { input?: { refresh?: boolean } }) => query<InstalledProgram[]>("programs", opts) },
    uninstall: { mutationOptions: mutation<{ key: string; quiet?: boolean }, UninstallResult>("uninstall") },

    updates: { queryOptions: (opts?: { input?: { refresh?: boolean } }) => query<WingetListing>("updates", opts) },
    upgrade: { mutationOptions: mutation<{ id: string }, { ok: boolean; output: string }>("upgrade") },
    upgradeAll: { mutationOptions: mutation<never, { ok: boolean; output: string }>("upgrade_all") },

    startupList: { queryOptions: () => query<StartupItem[]>("startup_list") },
    startupToggle: { mutationOptions: mutation<{ id: string; enabled: boolean }, { ok: boolean; error?: string }>("startup_toggle") },
    startupDelete: { mutationOptions: mutation<{ id: string }, { ok: boolean; error?: string }>("startup_delete") },

    scheduleGet: { queryOptions: () => query<ScheduleStatus>("schedule_get") },
    scheduleSave: {
      mutationOptions: mutation<
        Omit<ScheduleConfig, "lastRunAt" | "lastFreedBytes">,
        { config: ScheduleConfig; registered: boolean; error?: string }
      >("schedule_save"),
    },
    scheduleHistory: { queryOptions: () => query<ScheduleRun[]>("schedule_history") },
    scheduleRunNow: { mutationOptions: mutation<Record<string, never> | undefined, { freedBytes: number; files: number; categories: string[]; errors: string[] }>("schedule_run_now") },

    historyGet: { queryOptions: () => query<HistorySummary>("history_get") },
    historyCapture: { mutationOptions: mutation<Record<string, never> | undefined, HistorySummary & { snapshot: unknown }>("history_capture") },

    widgetInfo: { queryOptions: () => query<WidgetStatus>("widget_info") },
    widgetStart: { mutationOptions: mutation<Partial<WidgetSettings> | undefined, { ok: boolean; error?: string }>("widget_start") },
    widgetStop: { mutationOptions: mutation<Record<string, never> | undefined, { ok: boolean }>("widget_stop") },
    widgetSave: { mutationOptions: mutation<Partial<WidgetSettings>, WidgetSettings>("widget_save") },
    widgetAutostart: { mutationOptions: mutation<{ enabled: boolean }, { ok: boolean }>("widget_autostart") },
  },
};
