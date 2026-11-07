import { useMemo, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "@/client/api";
import { queryClient } from "@/client/rpc-client";
import type {
  DriverBackupScan,
  DriverExportResult,
  DriverOpResult,
  DriverPackage,
} from "@/shared/types";
import { formatBytes, formatDate } from "@/client/lib/format";
import { AlertIcon, DriverIcon, FolderIcon, RefreshIcon } from "./icons";
import { Badge, Button, Card, EmptyState, Modal, SectionTitle } from "./ui";
import { t, translateError } from "@/client/i18n";

const EMPTY_DRIVERS: DriverPackage[] = [];

function ResultList({ items }: { items: DriverOpResult[] }) {
  return (
    <div className="space-y-1">
      {items.map((item) => (
        <div key={item.name} className="text-[11px]">
          <p className={item.ok ? "text-emerald-300" : "text-rose-300"}>
            {item.ok ? "✓" : "✕"} {item.name}
            {item.reboot ? ` · ${t("требуется перезагрузка")}` : ""}
            {item.code !== null && !item.ok ? ` · ${t("код {code}", { code: item.code })}` : ""}
          </p>
          {item.output ? (
            <details className="mt-0.5">
              <summary className="cursor-pointer text-slate-500">{t("вывод")}</summary>
              <pre className="mt-1 max-h-32 overflow-auto whitespace-pre-wrap rounded-lg bg-black/40 p-2 font-mono text-[10px] text-slate-400">
                {item.output}
              </pre>
            </details>
          ) : null}
        </div>
      ))}
    </div>
  );
}

export function DriversTab() {
  const listQuery = useQuery(queryClient.system.driversList.queryOptions());
  const drivers = listQuery.data ?? EMPTY_DRIVERS;
  const [search, setSearch] = useState("");
  const [classFilter, setClassFilter] = useState("all");
  const [sort, setSort] = useState<"class" | "provider" | "name" | "version" | "date">("class");
  const [sortAsc, setSortAsc] = useState(true);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [busy, setBusy] = useState<"export" | "scan" | "restore" | null>(null);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [exportResult, setExportResult] = useState<DriverExportResult | null>(null);
  const [restoreResults, setRestoreResults] = useState<DriverOpResult[] | null>(null);
  const [backup, setBackup] = useState<DriverBackupScan | null>(null);
  const [backupSelected, setBackupSelected] = useState<Set<string>>(new Set());
  const [restoreFiles, setRestoreFiles] = useState<string[] | null>(null);
  const [restorePoint, setRestorePoint] = useState(true);
  const [now] = useState(() => Date.now());
  const [lastBackup, setLastBackup] = useState<number>(() =>
    Number(localStorage.getItem("dm.drivers.lastBackup") ?? 0),
  );
  const backupStale = !lastBackup || now - lastBackup > 90 * 24 * 60 * 60 * 1000;

  const classes = useMemo(
    () =>
      Array.from(new Set(drivers.map((driver) => driver.className).filter(Boolean))).sort((left, right) =>
        left.localeCompare(right),
      ),
    [drivers],
  );

  const filtered = useMemo(() => {
    const query = search.trim().toLowerCase();
    const list = drivers.filter((driver) => {
      if (classFilter !== "all" && driver.className !== classFilter) return false;
      if (!query) return true;
      return [
        driver.publishedName,
        driver.originalName,
        driver.provider,
        driver.className,
        driver.signer,
        driver.version,
      ]
        .join(" ")
        .toLowerCase()
        .includes(query);
    });
    const direction = sortAsc ? 1 : -1;
    const dateValue = (value: string) => {
      const [month, day, year] = value.split("/").map(Number);
      return year ? new Date(year, month - 1, day).getTime() : 0;
    };
    return list.sort((left, right) => {
      if (sort === "provider") {
        return left.provider.localeCompare(right.provider, "ru") * direction;
      }
      if (sort === "name") {
        return left.originalName.localeCompare(right.originalName, "ru") * direction;
      }
      if (sort === "version") {
        return left.version.localeCompare(right.version, "ru") * direction;
      }
      if (sort === "date") {
        return (dateValue(left.date) - dateValue(right.date)) * direction;
      }
      return left.className.localeCompare(right.className, "ru") * direction;
    });
  }, [drivers, search, classFilter, sort, sortAsc]);

  const allSelected =
    filtered.length > 0 && filtered.every((driver) => selected.has(driver.publishedName));

  function toggle(name: string) {
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  function toggleAll() {
    setSelected(allSelected ? new Set() : new Set(filtered.map((driver) => driver.publishedName)));
  }

  function toggleBackup(name: string) {
    setBackupSelected((current) => {
      const next = new Set(current);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  }

  async function exportDrivers(all: boolean) {
    if (!all && selected.size === 0) {
      setError(t("Выберите хотя бы один драйвер."));
      return;
    }
    const folder = await open({
      directory: true,
      multiple: false,
      title: t("Куда сохранить резервную копию драйверов?"),
    });
    if (!folder || Array.isArray(folder)) return;

    setBusy("export");
    setError(null);
    setResult(null);
    setExportResult(null);
    try {
      const response = await api.driversExport({
        parent: folder,
        infs: all ? [] : Array.from(selected),
      });
      setExportResult(response);
      const stamp = Date.now();
      setLastBackup(stamp);
      localStorage.setItem("dm.drivers.lastBackup", String(stamp));
      const failed = response.items.filter((item) => !item.ok).length;
      setResult(
        failed === 0
          ? t("Резервная копия создана: {path}", { path: response.path })
          : t("Экспорт завершён с ошибками: {failed}. Папка: {path}", {
              failed,
              path: response.path,
            }),
      );
    } catch (cause) {
      setError(translateError(cause));
    } finally {
      setBusy(null);
    }
  }

  async function pickBackup() {
    const folder = await open({
      directory: true,
      multiple: false,
      title: t("Выберите папку с резервной копией драйверов"),
    });
    if (!folder || Array.isArray(folder)) return;

    setBusy("scan");
    setError(null);
    setResult(null);
    setRestoreResults(null);
    try {
      const scan = await api.driversScanBackup(folder);
      setBackup(scan);
      setBackupSelected(new Set(scan.files.map((file) => file.relativePath)));
      if (scan.files.length === 0) {
        setError(t("В папке не найдены INF-файлы."));
      }
    } catch (cause) {
      setBackup(null);
      setError(translateError(cause));
    } finally {
      setBusy(null);
    }
  }

  function requestRestore(files: string[]) {
    if (files.length === 0) {
      setError(t("Выберите хотя бы один INF-файл."));
      return;
    }
    setError(null);
    setRestoreFiles(files);
  }

  async function runRestore() {
    if (!backup || !restoreFiles) return;
    setBusy("restore");
    setError(null);
    try {
      if (restorePoint) {
        const response = await api.systemOperation({ op: "restore_point_create" });
        if (!response.ok) {
          setError(response.message || t("Не удалось создать точку восстановления."));
          setRestoreFiles(null);
          return;
        }
      }
      const items = await api.driversRestore({ root: backup.root, infs: restoreFiles });
      setRestoreResults(items);
      const failed = items.filter((item) => !item.ok).length;
      const reboot = items.some((item) => item.reboot);
      setResult(
        failed === 0
          ? reboot
            ? t("Драйверы восстановлены. Требуется перезагрузка Windows.")
            : t("Драйверы восстановлены.")
          : t("Восстановление завершено с ошибками: {failed}", { failed }),
      );
      setRestoreFiles(null);
    } catch (cause) {
      setError(translateError(cause));
      setRestoreFiles(null);
    } finally {
      setBusy(null);
    }
  }

  const selectedCount = selected.size;
  const backupFiles = backup?.files ?? [];
  const backupSelectedCount = backupSelected.size;

  return (
    <div className="space-y-4">
      <SectionTitle
        title={t("Драйверы")}
        subtitle={t("Сторонние драйверы Windows: резервная копия и восстановление. Ничего не встраивается в программу — файлы сохраняются в выбранную папку.")}
      />

      <Card className="border-amber-400/20 bg-amber-400/[0.05] p-3.5">
        <p className="flex items-start gap-2 text-[11px] leading-relaxed text-amber-100">
          <AlertIcon className="mt-0.5 size-3.5 shrink-0" />
          {t("Восстановление драйверов может повлиять на устройства и загрузку. Перед восстановлением создайте точку восстановления. Не восстанавливайте драйверы без необходимости.")}
        </p>
      </Card>

      {backupStale ? (
        <Card className="border-sky-400/20 bg-sky-400/[0.05] p-3.5">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <p className="text-[11px] text-sky-100">
              {lastBackup
                ? t("Резервная копия драйверов старше 90 дней. Последняя: {date}", {
                    date: formatDate(lastBackup),
                  })
                : t("Резервная копия драйверов ещё не делалась.")}
            </p>
            <Button size="sm" variant="secondary" loading={busy === "export"} onClick={() => void exportDrivers(true)}>
              {t("Сделать бэкап")}
            </Button>
          </div>
        </Card>
      ) : null}

      {error ? (
        <Card className="border-rose-400/25 bg-rose-500/[0.07] p-3.5">
          <p className="text-xs text-rose-100">{error}</p>
        </Card>
      ) : null}

      {result ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-3.5">
          <p className="break-all text-xs text-emerald-100">{result}</p>
        </Card>
      ) : null}

      <Card className="p-4">
        <div className="flex flex-wrap items-center gap-2">
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <input
              value={search}
              onChange={(event) => setSearch(event.target.value)}
              placeholder={t("Поиск по имени, поставщику или классу")}
              className="min-w-0 flex-1 rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-100 outline-none placeholder:text-slate-600 focus:border-emerald-400/40"
            />
            <select
              value={classFilter}
              onChange={(event) => setClassFilter(event.target.value)}
              className="max-w-44 rounded-lg border border-white/10 bg-[var(--app-menu)] px-2 py-1.5 text-xs text-slate-200 outline-none focus:border-emerald-400/40"
            >
              <option value="all">{t("Все классы")}</option>
              {classes.map((className) => (
                <option key={className} value={className}>
                  {className}
                </option>
              ))}
            </select>
            <select
              value={sort}
              onChange={(event) => setSort(event.target.value as typeof sort)}
              className="rounded-lg border border-white/10 bg-[var(--app-menu)] px-2 py-1.5 text-xs text-slate-200 outline-none focus:border-emerald-400/40"
            >
              <option value="class">{t("Класс")}</option>
              <option value="provider">{t("Поставщик")}</option>
              <option value="name">{t("Имя")}</option>
              <option value="version">{t("Версия")}</option>
              <option value="date">{t("Дата")}</option>
            </select>
            <button
              onClick={() => setSortAsc((current) => !current)}
              className="rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 text-xs text-slate-300"
              title={t("Направление сортировки")}
            >
              {sortAsc ? "↑" : "↓"}
            </button>
          </div>
          <Badge variant="neutral">{filtered.length}</Badge>
          <Button
            size="sm"
            variant="secondary"
            loading={listQuery.isFetching}
            onClick={() => void listQuery.refetch()}
          >
            <RefreshIcon className="size-3.5" />
            {t("Обновить")}
          </Button>
        </div>

        {listQuery.isLoading ? (
          <p className="mt-4 text-xs text-slate-500">{t("Читаю список драйверов…")}</p>
        ) : listQuery.isError ? (
          <p className="mt-4 text-xs text-rose-200">{translateError(listQuery.error)}</p>
        ) : filtered.length === 0 ? (
          <EmptyState
            icon={<DriverIcon className="size-6" />}
            title={t("Драйверы не найдены")}
            description={t("Измените запрос или обновите список.")}
          />
        ) : (
          <div className="mt-3 max-h-[52vh] overflow-auto rounded-xl border border-white/[0.06]">
            <table className="w-full min-w-[760px] border-collapse text-left text-[11px]">
              <thead className="sticky top-0 z-10 bg-[var(--app-menu)] text-slate-500">
                <tr>
                  <th className="w-8 px-2 py-2">
                    <input
                      type="checkbox"
                      className="size-3.5 accent-emerald-500"
                      checked={allSelected}
                      onChange={toggleAll}
                      aria-label={t("Выбрать все")}
                    />
                  </th>
                  <th className="px-2 py-2 font-normal">{t("Опубликованное имя")}</th>
                  <th className="px-2 py-2 font-normal">{t("Оригинальный INF")}</th>
                  <th className="px-2 py-2 font-normal">{t("Поставщик")}</th>
                  <th className="px-2 py-2 font-normal">{t("Класс")}</th>
                  <th className="px-2 py-2 font-normal">{t("Версия")}</th>
                  <th className="px-2 py-2 font-normal">{t("Подписант")}</th>
                </tr>
              </thead>
              <tbody>
                {filtered.map((driver: DriverPackage) => (
                  <tr
                    key={driver.publishedName}
                    className="border-t border-white/[0.05] align-top hover:bg-white/[0.02]"
                  >
                    <td className="px-2 py-1.5">
                      <input
                        type="checkbox"
                        className="size-3.5 accent-emerald-500"
                        checked={selected.has(driver.publishedName)}
                        onChange={() => toggle(driver.publishedName)}
                        aria-label={driver.publishedName}
                      />
                    </td>
                    <td className="whitespace-nowrap px-2 py-1.5 font-mono text-slate-300">
                      {driver.publishedName}
                    </td>
                    <td className="max-w-44 truncate px-2 py-1.5 text-slate-400" title={driver.originalName}>
                      {driver.originalName}
                    </td>
                    <td className="max-w-40 truncate px-2 py-1.5 text-slate-300" title={driver.provider}>
                      {driver.provider || "—"}
                    </td>
                    <td className="max-w-40 truncate px-2 py-1.5 text-slate-400" title={driver.className}>
                      {driver.className || "—"}
                    </td>
                    <td className="whitespace-nowrap px-2 py-1.5 text-slate-400">
                      {driver.version || "—"}
                      {driver.date ? <span className="block text-[10px] text-slate-600">{driver.date}</span> : null}
                    </td>
                    <td className="max-w-44 truncate px-2 py-1.5 text-slate-500" title={driver.signer}>
                      {driver.signer || "—"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        <div className="mt-3 flex flex-wrap gap-2">
          <Button size="sm" loading={busy === "export"} onClick={() => void exportDrivers(true)}>
            {t("Экспортировать все")}
          </Button>
          <Button
            size="sm"
            variant="secondary"
            disabled={selectedCount === 0}
            loading={busy === "export"}
            onClick={() => void exportDrivers(false)}
          >
            {t("Экспортировать выбранные ({count})", { count: selectedCount })}
          </Button>
          <Button size="sm" variant="secondary" loading={busy === "scan"} onClick={() => void pickBackup()}>
            {t("Восстановить из папки")}
          </Button>
        </div>
      </Card>

      {exportResult ? (
        <Card className="p-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <p className="text-sm font-medium text-white">{t("Резервная копия")}</p>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void api.reveal({ path: exportResult.path })}
            >
              <FolderIcon className="size-3.5" />
              {t("Открыть папку")}
            </Button>
          </div>
          <p className="mt-1 break-all text-[11px] text-slate-400">{exportResult.path}</p>
          <div className="mt-2">
            <ResultList items={exportResult.items} />
          </div>
        </Card>
      ) : null}

      {backup ? (
        <Card className="p-4">
          <div className="flex flex-wrap items-center justify-between gap-2">
            <div>
              <p className="text-sm font-medium text-white">{t("Восстановление из резервной копии")}</p>
              <p className="mt-0.5 break-all text-[11px] text-slate-500">{backup.root}</p>
            </div>
            <Badge variant={backup.files.length ? "neutral" : "warn"}>
              {t("{count} файлов", { count: backup.files.length })}
            </Badge>
          </div>
          {backup.manifest ? (
            <p className="mt-1 text-[11px] text-slate-500">
              {t("Создана {date}, версия {version}", {
                date: formatDate(backup.manifest.createdAt),
                version: backup.manifest.appVersion,
              })}
            </p>
          ) : null}

          {backupFiles.length ? (
            <>
              <div className="mt-3 max-h-64 space-y-1 overflow-auto rounded-xl border border-white/[0.06] p-2">
                <label className="flex items-center gap-2 border-b border-white/[0.06] pb-1 text-[11px] text-slate-400">
                  <input
                    type="checkbox"
                    className="size-3.5 accent-emerald-500"
                    checked={backupSelectedCount === backupFiles.length}
                    onChange={() =>
                      setBackupSelected(
                        backupSelectedCount === backupFiles.length
                          ? new Set()
                          : new Set(backupFiles.map((file) => file.relativePath)),
                      )
                    }
                  />
                  {t("Выбрать все")}
                </label>
                {backupFiles.map((file) => (
                  <label
                    key={file.relativePath}
                    className="flex items-center gap-2 text-[11px] text-slate-300"
                  >
                    <input
                      type="checkbox"
                      className="size-3.5 accent-emerald-500"
                      checked={backupSelected.has(file.relativePath)}
                      onChange={() => toggleBackup(file.relativePath)}
                    />
                    <span className="min-w-0 flex-1 truncate font-mono" title={file.relativePath}>
                      {file.relativePath}
                    </span>
                    <span className="shrink-0 text-[10px] text-slate-600">{formatBytes(file.size, 1)}</span>
                  </label>
                ))}
              </div>
              <div className="mt-3 flex flex-wrap gap-2">
                <Button
                  size="sm"
                  variant="danger"
                  loading={busy === "restore"}
                  onClick={() => requestRestore(backupFiles.map((file) => file.relativePath))}
                >
                  {t("Восстановить все")}
                </Button>
                <Button
                  size="sm"
                  variant="danger"
                  disabled={backupSelectedCount === 0}
                  loading={busy === "restore"}
                  onClick={() => requestRestore(Array.from(backupSelected))}
                >
                  {t("Восстановить выбранные ({count})", { count: backupSelectedCount })}
                </Button>
              </div>
            </>
          ) : (
            <p className="mt-2 text-[11px] text-rose-200">{t("В папке не найдены INF-файлы.")}</p>
          )}

          {restoreResults ? (
            <div className="mt-3">
              <p className="mb-1 text-[11px] text-slate-400">{t("Результат восстановления")}</p>
              <ResultList items={restoreResults} />
            </div>
          ) : null}
        </Card>
      ) : null}

      <Modal
        open={restoreFiles !== null}
        onClose={() => {
          if (busy !== "restore") setRestoreFiles(null);
        }}
        title={t("Восстановить драйверы?")}
        description={t("Будет установлено файлов: {count}. Это может повлиять на устройства и загрузку Windows.", {
          count: restoreFiles?.length ?? 0,
        })}
        footer={
          <>
            <Button
              size="sm"
              variant="secondary"
              disabled={busy === "restore"}
              onClick={() => setRestoreFiles(null)}
            >
              {t("Отмена")}
            </Button>
            <Button
              size="sm"
              variant="danger"
              loading={busy === "restore"}
              onClick={() => void runRestore()}
            >
              {t("Восстановить")}
            </Button>
          </>
        }
      >
        <label className="flex items-start gap-2 text-[11px] text-slate-300">
          <input
            type="checkbox"
            className="mt-0.5 size-3.5 accent-emerald-500"
            checked={restorePoint}
            onChange={(event) => setRestorePoint(event.target.checked)}
          />
          <span>{t("Создать точку восстановления Windows перед установкой")}</span>
        </label>
        <p className="mt-2 text-[11px] text-slate-500">
          {t("Если Windows отключил защиту системы, создание точки может занять время или завершиться ошибкой.")}
        </p>
      </Modal>
    </div>
  );
}
