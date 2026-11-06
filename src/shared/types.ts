/**
 * Shared data contract between the React UI and the Rust backend.
 *
 * The Rust structs use `#[serde(rename_all = "camelCase")]`, so every field
 * here must stay in sync with `src-tauri/src/types.rs`.
 */

/* --------------------------------- system --------------------------------- */

export type DriveInfo = {
  letter: string;
  label: string;
  fileSystem: string;
  total: number;
  free: number;
  used: number;
};

export type SystemInfo = {
  machine: string;
  user: string;
  os: string;
  build: string;
  version: string;
  lastBoot: string | null;
  cpu: string;
  cores: number;
  threads: number;
  ramTotal: number;
  ramFree: number;
  admin: boolean;
  drives: DriveInfo[];
};

/* --------------------------------- metrics -------------------------------- */

export type MetricsSample = {
  t: number;
  cpu: number;
  cores: number[];
  ramUsed: number;
  ramTotal: number;
  diskRead: number;
  diskWrite: number;
  netIn: number;
  netOut: number;
  netAdapter: string | null;
};

export type MetricsState = {
  history: MetricsSample[];
  latest: MetricsSample | null;
  intervalMs: number;
  error?: string;
};

export type ProcessInfo = {
  pid: number;
  name: string;
  memory: number;
  cpuPercent: number;
  path: string | null;
  suspended: boolean;
};

/* ---------------------------------- scan ---------------------------------- */

export type Risk = "safe" | "caution";
export type CategoryKind = "contents" | "files" | "recycle-bin";
export type CategoryStatus = "pending" | "scanning" | "done" | "partial" | "error";

export type ScanCategoryResult = {
  id: string;
  name: string;
  description: string;
  paths: string[];
  kind: CategoryKind;
  risk: Risk;
  needsAdmin: boolean;
  note?: string;
  bytes: number;
  files: number;
  status: CategoryStatus;
  error?: string;
};

export type LargeFile = {
  path: string;
  name: string;
  dir: string;
  size: number;
  modified: number;
};

export type ScanPhase = "starting" | "junk" | "large-files" | "done" | "error";

export type ScanState = {
  jobId: string;
  startedAt: number;
  finishedAt: number | null;
  phase: ScanPhase;
  percent: number;
  scanPath: string;
  filesScanned: number;
  bytesScanned: number;
  categories: ScanCategoryResult[];
  largeFiles: LargeFile[];
  largeFilesTruncated: boolean;
  largeFileThreshold: number;
  totalJunkBytes: number;
  totalJunkFiles: number;
  error?: string;
};

export type CleanCategoryResult = {
  id: string;
  name: string;
  bytes: number;
  files: number;
  errors: string[];
};

export type CleanResult = {
  freedBytes: number;
  deletedFiles: number;
  categories: CleanCategoryResult[];
  errors: string[];
};

export type PreviewEntry = {
  path: string;
  size: number;
};

export type PreviewCategory = {
  id: string;
  name: string;
  totalBytes: number;
  totalFiles: number;
  entries: PreviewEntry[];
  truncated: boolean;
};

export type CleanPreview = {
  categories: PreviewCategory[];
};

export type CleanHistoryEntry = {
  at: number;
  action: string;
  trigger: string;
  categories: string[];
  bytes: number;
  files: number;
  errors: string[];
};

export type RecycleResult = {
  removed: number;
  bytes: number;
  errors: string[];
};

/* -------------------------------- folders --------------------------------- */

export type FolderEntry = {
  path: string;
  name: string;
  isDir: boolean;
  size: number;
  files: number;
  modified: number;
  children: number;
};

export type FolderAnalysis = {
  path: string;
  name: string;
  size: number;
  files: number;
  entries: FolderEntry[];
  parent: string | null;
  elapsedMs: number;
  truncated: boolean;
  error?: string;
};

export type DeleteResult = {
  ok: boolean;
  bytes: number;
  files: number;
  errors: string[];
};

/* ------------------------------- duplicates ------------------------------- */

export type DuplicateFile = {
  path: string;
  name: string;
  dir: string;
  size: number;
  modified: number;
};

export type DuplicateGroup = {
  hash: string;
  size: number;
  count: number;
  wasted: number;
  files: DuplicateFile[];
};

export type DuplicateProgress = {
  phase: string;
  scanned: number;
  hashed: number;
  total: number;
  percent: number;
  cancelled: boolean;
};

export type DuplicateScan = {
  id: string;
  root: string;
  minSize: number;
  scannedFiles: number;
  candidateFiles: number;
  groups: DuplicateGroup[];
  elapsedMs: number;
  truncated: boolean;
  error?: string;
};

/* -------------------------------- programs -------------------------------- */

export type InstalledProgram = {
  key: string;
  name: string;
  version: string | null;
  publisher: string | null;
  installDate: string | null;
  sizeBytes: number;
  location: string | null;
  uninstallString: string | null;
  quietUninstallString: string | null;
  noRemove: boolean;
  displayIcon?: string | null;
};

export type UninstallResult = {
  launched: boolean;
  command?: string;
  error?: string;
};

/* --------------------------------- winget --------------------------------- */

export type WingetUpdate = {
  name: string;
  id: string;
  current: string;
  available: string;
  source: string;
};

export type WingetListing = {
  available: boolean;
  updates: WingetUpdate[];
  error?: string;
  raw?: string;
  refreshedAt: number;
};

/* -------------------------------- startup --------------------------------- */

export type StartupItem = {
  id: string;
  name: string;
  command: string;
  location: string;
  source: "registry" | "startup-folder" | "task";
  enabled: boolean;
  publisher: string | null;
  executable: string | null;
  exists: boolean;
  system: boolean;
};

/* ------------------------------- schedule --------------------------------- */

export type ScheduleFrequency = "daily" | "weekly" | "monthly" | "logon";

export type ScheduleConfig = {
  enabled: boolean;
  frequency: ScheduleFrequency;
  time: string;
  dayOfWeek: number;
  categories: string[];
  thresholdMb: number;
  lastRunAt: number | null;
  lastFreedBytes: number | null;
};

export type ScheduleRun = {
  at: number;
  freedBytes: number;
  files: number;
  categories: string[];
  trigger: "manual" | "scheduled";
};

export type ScheduleStatus = {
  config: ScheduleConfig;
  registered: boolean;
  nextRun: string | null;
};

/* -------------------------------- history --------------------------------- */

export type DriveSnapshot = {
  t: number;
  drives: { letter: string; used: number; free: number; total: number }[];
};

export type HistorySummary = {
  points: DriveSnapshot[];
  letters: string[];
  growthPerDay: number;
  firstAt: number | null;
};

/* --------------------------------- widget --------------------------------- */

export type WidgetSettings = {
  x: number;
  y: number;
  width: number;
  height: number;
  opacity: number;
  accent: string;
  radius: number;
  fontScale: number;
  showCpu: boolean;
  showRam: boolean;
  showDisk: boolean;
  showNet: boolean;
  showDrives: boolean;
  showTopProcess: boolean;
  showTemperatures: boolean;
  bars: boolean;
  zMode: "normal" | "top" | "desktop";
  clickThrough: boolean;
  intervalMs: number;
  showTitle: boolean;
  title: string;
};

export type WidgetStatus = {
  running: boolean;
  autostart: boolean;
  settings: WidgetSettings;
  log: string | null;
};

/* ------------------------------ maintenance ------------------------------- */

export type DiskHealth = {
  deviceId: string;
  name: string;
  mediaType: number | null;
  size: number;
  health: number | null;
  operational: number[];
  serial: string | null;
  temperature: number | null;
  wear: number | null;
  powerOnHours: number | null;
  readErrors: number | null;
  writeErrors: number | null;
  smart: boolean;
  mediaErrors: number | null;
  errorLogEntries: number | null;
  criticalWarning: number | null;
  availableSpare: number | null;
  unsafeShutdowns: number | null;
};

export type SmartDisk = {
  index: number;
  model: string;
  serial: string;
  busType: number;
  criticalWarning: number;
  temperature: number | null;
  availableSpare: number | null;
  spareThreshold: number | null;
  wear: number | null;
  powerOnHours: number | null;
  unsafeShutdowns: number | null;
  mediaErrors: number | null;
  errorLogEntries: number | null;
  dataRead: number | null;
  dataWritten: number | null;
};

export type SensorInfo = {
  name: string;
  sensorType: string;
  value: number;
  identifier: string | null;
};

export type RestorePoint = {
  sequence: number;
  description: string | null;
  createdAt: string | null;
  kind: number | null;
};

export type MaintenanceStatus = {
  disks: DiskHealth[];
  sensors: SensorInfo[];
  restorePoints: RestorePoint[];
  helperAvailable: boolean;
};

/* -------------------------------- settings -------------------------------- */

export type AppSettings = {
  notifyLowDisk: boolean;
  lowDiskPercent: number;
  lowDiskCooldownMin: number;
  quickCleanNotify: boolean;
  minimizeToTray: boolean;
  alwaysAdmin: boolean;
  protectPaths: string[];
  hotkeyShow: string;
  hotkeyWidget: string;
};

export type SettingsState = {
  settings: AppSettings;
  autostart: boolean;
  portable: boolean;
  hotkeyErrors: string[];
  startupError: string | null;
};

/* --------------------------------- sensors -------------------------------- */

export type SensorReading = {
  kind: "temperature" | "fan" | "percent";
  name: string;
  hardware: string;
  hardwareType: string;
  value: number;
  identifier: string;
};

export type SensorsState = {
  available: boolean;
  running: boolean;
  admin: boolean;
  pawnioInstalled: boolean;
  pawnioVersion: string | null;
  source: string;
  updatedAt: string | null;
  ageSeconds: number | null;
  sensors: SensorReading[];
  error: string | null;
  needsReboot: boolean;
};
