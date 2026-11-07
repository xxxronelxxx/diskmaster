import { useState } from "react";
import type { ReactNode } from "react";
import { useMutation, useQuery } from "@tanstack/react-query";
import { open, save } from "@tauri-apps/plugin-dialog";
import type { AppSettings } from "@/shared/types";
import { api } from "@/client/api";
import { queryClient } from "@/client/rpc-client";
import { useI18n } from "@/client/i18n-provider";
import { AlertIcon, CheckIcon, DownloadIcon, FolderIcon, RefreshIcon, SettingsIcon, TrashIcon } from "./icons";
import { Badge, Button, Card, SectionTitle, Switch } from "./ui";
import { t, translateError } from "@/client/i18n";

const DEFAULT_SETTINGS: AppSettings = {
  notifyLowDisk: true,
  lowDiskPercent: 10,
  lowDiskCooldownMin: 60,
  quickCleanNotify: true,
  smartNotify: true,
  minimizeToTray: true,
  alwaysAdmin: false,
  protectPaths: [],
  hotkeyShow: "CmdOrCtrl+Alt+D",
  hotkeyWidget: "CmdOrCtrl+Alt+W",
};

function SettingRow({
  title,
  description,
  children,
}: {
  title: string;
  description?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-4 py-3 first:pt-0 last:pb-0">
      <div className="min-w-0">
        <p className="text-sm text-slate-100">{title}</p>
        {description ? <p className="mt-0.5 text-[11px] leading-relaxed text-slate-500">{description}</p> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

export function SettingsTab() {
  const { locale, changeLocale } = useI18n();
  const stateQuery = useQuery(queryClient.system.settingsState.queryOptions());
  const state = stateQuery.data;
  const [draft, setDraft] = useState<AppSettings | null>(null);
  const [draftSource, setDraftSource] = useState<AppSettings | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notifyResult, setNotifyResult] = useState<string | null>(null);

  // Adopt the persisted settings on first load; later edits stay local until
  // the user saves or imports.
  if (state?.settings && state.settings !== draftSource) {
    setDraftSource(state.settings);
    if (!draft) setDraft(state.settings);
  }

  const saveMutation = useMutation(queryClient.system.settingsSave.mutationOptions());
  const autostartMutation = useMutation(queryClient.system.autostartSet.mutationOptions());
  const notifyMutation = useMutation(queryClient.system.notificationTest.mutationOptions());
  const importMutation = useMutation({
    mutationFn: (path: string) => api.settingsImport(path),
  });

  function patch(next: Partial<AppSettings>) {
    setDraft((current) => (current ? { ...current, ...next } : current));
    setNotice(null);
  }

  const dirty = Boolean(draft && state && JSON.stringify(draft) !== JSON.stringify(state.settings));
  const settings = draft ?? state?.settings ?? DEFAULT_SETTINGS;

  async function saveSettings() {
    if (!draft) return;
    setError(null);
    try {
      const saved = await saveMutation.mutateAsync(draft);
      if (saved.startupError) {
        setError(translateError(saved.startupError));
      } else if (saved.hotkeyErrors.length > 0) {
        setError(t("Не удалось зарегистрировать: {errors}", { errors: saved.hotkeyErrors.join("; ") }));
      } else {
        setNotice(t("Настройки сохранены"));
      }
      await stateQuery.refetch();
    } catch (cause) {
      setError(translateError(cause));
    }
  }

  async function toggleAutostart(enabled: boolean) {
    setError(null);
    try {
      const result = await autostartMutation.mutateAsync({ enabled });
      if (!result.ok) setError(result.error ?? t("Не удалось изменить автозапуск"));
      await stateQuery.refetch();
    } catch (cause) {
      setError(translateError(cause));
    }
  }

  async function testNotification() {
    setNotifyResult(null);
    try {
      const result = await notifyMutation.mutateAsync(undefined);
      setNotifyResult(result.ok ? t("Уведомление отправлено") : (result.error ?? t("Не удалось отправить уведомление")));
    } catch (cause) {
      setNotifyResult(translateError(cause));
    }
  }

  async function addProtectedFolder() {
    const selected = await open({ directory: true, multiple: false, title: t("Защищённая папка") });
    if (typeof selected !== "string") return;
    const exists = settings.protectPaths.some(
      (value) => value.toLowerCase() === selected.toLowerCase(),
    );
    if (exists) return;
    patch({ protectPaths: [...settings.protectPaths, selected] });
  }

  function removeProtectedFolder(path: string) {
    patch({ protectPaths: settings.protectPaths.filter((value) => value !== path) });
  }

  async function exportSettings() {
    setError(null);
    try {
      const target = await save({
        title: t("Экспорт настроек"),
        defaultPath: "diskmaster-settings.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!target) return;
      await api.settingsExport(target);
      setNotice(t("Настройки сохранены в {path}", { path: target }));
    } catch (cause) {
      setError(translateError(cause));
    }
  }

  async function importSettings() {
    setError(null);
    try {
      const selected = await open({
        multiple: false,
        directory: false,
        title: t("Импорт настроек"),
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (typeof selected !== "string") return;
      const imported = await importMutation.mutateAsync(selected);
      setDraft(imported.settings);
      if (imported.startupError) {
        setError(translateError(imported.startupError));
      } else if (imported.hotkeyErrors.length > 0) {
        setError(t("Не удалось зарегистрировать: {errors}", { errors: imported.hotkeyErrors.join("; ") }));
      } else {
        setNotice(t("Настройки импортированы"));
      }
      await stateQuery.refetch();
    } catch (cause) {
      setError(translateError(cause));
    }
  }

  return (
    <div className="space-y-5">
      <SectionTitle
        title={t("Настройки")}
        subtitle={t("Уведомления, горячие клавиши, защищённые папки и резервная копия настроек")}
        action={
          <div className="flex gap-2">
            <Button
              variant="ghost"
              onClick={() => setDraft({ ...DEFAULT_SETTINGS, protectPaths: settings.protectPaths })}
              disabled={!draft}
            >
              {t("Сбросить")}
            </Button>
            <Button variant="primary" onClick={() => void saveSettings()} loading={saveMutation.isPending} disabled={!dirty}>
              {t("Сохранить")}
            </Button>
          </div>
        }
      />

      {error ? (
        <Card className="border-rose-400/25 bg-rose-500/[0.07] p-3.5">
          <p className="flex items-center gap-2 text-xs text-rose-100">
            <AlertIcon className="size-3.5" /> {error}
          </p>
        </Card>
      ) : null}

      {notice ? (
        <Card className="border-emerald-400/25 bg-emerald-500/[0.07] p-3.5">
          <p className="flex items-center gap-2 text-xs text-emerald-100">
            <CheckIcon className="size-3.5" /> {notice}
          </p>
        </Card>
      ) : null}

      <Card className="flex flex-wrap items-center justify-between gap-3 p-4">
        <div>
          <p className="text-sm font-medium text-white">{t("Язык интерфейса")}</p>
          <p className="mt-0.5 text-[11px] text-slate-500">
            {t("Переключается сразу, без перезапуска. Настройка хранится на этом компьютере.")}
          </p>
        </div>
        <div className="flex gap-2">
          <Button
            size="sm"
            variant={locale === "ru" ? "primary" : "secondary"}
            onClick={() => changeLocale("ru")}
          >
            {t("Русский")}
          </Button>
          <Button
            size="sm"
            variant={locale === "en" ? "primary" : "secondary"}
            onClick={() => changeLocale("en")}
          >
            English
          </Button>
        </div>
      </Card>

      {state?.portable ? (
        <Card className="border-amber-400/25 bg-amber-400/[0.06] p-3.5">
          <p className="text-xs text-amber-100">
            {t("Портативная версия: автозапуск недоступен, настройки лежат рядом с программой. Системные уведомления могут не показываться без ярлыка в меню «Пуск» — в установленной версии они работают.")}
          </p>
        </Card>
      ) : null}

      <div className="grid gap-4 xl:grid-cols-2">
        <Card className="p-4">
          <div className="mb-2 flex items-center gap-2">
            <AlertIcon className="size-4 text-amber-300" />
            <p className="text-sm font-medium text-white">{t("Уведомления")}</p>
          </div>

          <SettingRow
            title={t("Мало свободного места")}
            description={t("Показывать системное уведомление, когда на диске остаётся меньше порога.")}
          >
            <Switch
              label={t("Уведомлять о низком свободном месте")}
              checked={settings.notifyLowDisk}
              onChange={(value) => patch({ notifyLowDisk: value })}
            />
          </SettingRow>

          <div className="my-2 h-px bg-white/[0.06]" />

          <div className="grid gap-3 sm:grid-cols-2">
            <label className="text-[11px] text-slate-400">
              {t("Порог свободного места, %")}
              <input
                type="number"
                min={1}
                max={50}
                value={settings.lowDiskPercent}
                disabled={!settings.notifyLowDisk}
                onChange={(event) => patch({ lowDiskPercent: Number(event.target.value) || 1 })}
                className="mt-1 w-full rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 text-sm text-slate-100 outline-none focus:border-emerald-400/40 disabled:opacity-40"
              />
            </label>
            <label className="text-[11px] text-slate-400">
              {t("Повтор не чаще, минут")}
              <input
                type="number"
                min={5}
                max={1440}
                value={settings.lowDiskCooldownMin}
                disabled={!settings.notifyLowDisk}
                onChange={(event) => patch({ lowDiskCooldownMin: Number(event.target.value) || 5 })}
                className="mt-1 w-full rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 text-sm text-slate-100 outline-none focus:border-emerald-400/40 disabled:opacity-40"
              />
            </label>
          </div>

          <div className="mt-3 flex items-center justify-between gap-3">
            <p className="text-[11px] text-slate-500">
              {t("Проверка выполняется каждые 3 минуты в фоне.")}
            </p>
            <Button
              size="sm"
              variant="secondary"
              loading={notifyMutation.isPending}
              onClick={() => void testNotification()}
            >
              {t("Проверить уведомление")}
            </Button>
          </div>
          {notifyResult ? (
            <p className="mt-2 text-[11px] text-slate-400">{notifyResult}</p>
          ) : null}

          <div className="my-2 h-px bg-white/[0.06]" />

          <SettingRow
            title={t("Проблемы SMART")}
            description={t("Уведомлять, если диск сообщает о критическом предупреждении или возможном отказе.")}
          >
            <Switch
              label={t("Уведомлять о проблемах SMART")}
              checked={settings.smartNotify}
              onChange={(value) => patch({ smartNotify: value })}
            />
          </SettingRow>

          <div className="my-2 h-px bg-white/[0.06]" />

          <SettingRow
            title={t("Итог быстрой очистки")}
            description={t("Уведомление после очистки из меню в трее.")}
          >
            <Switch
              label={t("Уведомлять об итоге быстрой очистки")}
              checked={settings.quickCleanNotify}
              onChange={(value) => patch({ quickCleanNotify: value })}
            />
          </SettingRow>
        </Card>

        <Card className="p-4">
          <div className="mb-2 flex items-center gap-2">
            <FolderIcon className="size-4 text-emerald-300" />
            <p className="text-sm font-medium text-white">{t("Защищённые папки")}</p>
            <Badge variant="neutral">{settings.protectPaths.length}</Badge>
          </div>
          <p className="mb-3 text-[11px] leading-relaxed text-slate-500">
            {t("Удаление и очистка никогда не трогают файлы внутри этих папок. Проверка работает для больших файлов, дубликатов, анализа диска и категорий очистки.")}
          </p>

          {settings.protectPaths.length === 0 ? (
            <p className="rounded-lg border border-dashed border-white/10 px-3 py-4 text-center text-[11px] text-slate-500">
              {t("Пока ничего не защищено.")}
            </p>
          ) : (
            <div className="space-y-1.5">
              {settings.protectPaths.map((path) => (
                <div
                  key={path}
                  className="flex items-center gap-2 rounded-lg border border-white/[0.07] bg-white/[0.02] px-2.5 py-2"
                >
                  <FolderIcon className="size-3.5 shrink-0 text-slate-500" />
                  <span className="min-w-0 flex-1 truncate text-[11px] text-slate-300" title={path}>
                    {path}
                  </span>
                  <button
                    onClick={() => removeProtectedFolder(path)}
                    className="grid size-6 shrink-0 place-items-center rounded text-slate-500 hover:bg-rose-500/15 hover:text-rose-300"
                    title={t("Убрать из защищённых")}
                  >
                    <TrashIcon className="size-3.5" />
                  </button>
                </div>
              ))}
            </div>
          )}

          <Button className="mt-3" size="sm" variant="secondary" onClick={() => void addProtectedFolder()}>
            <FolderIcon className="size-3.5" />
            {t("Добавить папку")}
          </Button>
          <p className="mt-2 text-[10px] text-slate-600">
            {t("Изменения применяются после нажатия «Сохранить».")}
          </p>
        </Card>

        <Card className="p-4">
          <div className="mb-2 flex items-center gap-2">
            <SettingsIcon className="size-4 text-sky-300" />
            <p className="text-sm font-medium text-white">{t("Горячие клавиши")}</p>
          </div>
          <p className="mb-3 text-[11px] leading-relaxed text-slate-500">
            {t("Работают глобально. Формат:")} <code className="text-slate-300">CmdOrCtrl+Alt+D</code>{t(". Пустое поле отключает клавишу. Если сочетание занято, будет показана ошибка.")}
          </p>

          <label className="block text-[11px] text-slate-400">
            {t("Открыть Диск-Мастер")}
            <input
              value={settings.hotkeyShow}
              onChange={(event) => patch({ hotkeyShow: event.target.value })}
              placeholder="CmdOrCtrl+Alt+D"
              className="mt-1 w-full rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 font-mono text-sm text-slate-100 outline-none focus:border-sky-400/40"
            />
          </label>
          <label className="mt-3 block text-[11px] text-slate-400">
            {t("Показать / скрыть виджет")}
            <input
              value={settings.hotkeyWidget}
              onChange={(event) => patch({ hotkeyWidget: event.target.value })}
              placeholder="CmdOrCtrl+Alt+W"
              className="mt-1 w-full rounded-lg border border-white/10 bg-white/[0.04] px-2.5 py-1.5 font-mono text-sm text-slate-100 outline-none focus:border-sky-400/40"
            />
          </label>
        </Card>

        <Card className="p-4">
          <div className="mb-2 flex items-center gap-2">
            <RefreshIcon className="size-4 text-violet-300" />
            <p className="text-sm font-medium text-white">{t("Поведение и система")}</p>
          </div>

          <SettingRow
            title={t("Запускать при входе в Windows")}
            description={t("В обычном режиме — ярлык в автозапуске; в режиме администратора — задача Планировщика с повышенными правами.")}
          >
            <Switch
              label={t("Запускать при входе в Windows")}
              checked={Boolean(state?.autostart)}
              disabled={Boolean(state?.portable) || autostartMutation.isPending}
              onChange={(value) => void toggleAutostart(value)}
            />
          </SettingRow>

          <div className="my-2 h-px bg-white/[0.06]" />

          <SettingRow
            title={t("Всегда запускать с правами администратора")}
            description={t("Ручной запуск попросит UAC; автозапуск при входе пойдёт через Планировщик с повышенными правами — без запроса. Применяется после «Сохранить».")}
          >
            <Switch
              label={t("Всегда запускать с правами администратора")}
              checked={settings.alwaysAdmin}
              disabled={Boolean(state?.portable)}
              onChange={(value) => patch({ alwaysAdmin: value })}
            />
          </SettingRow>

          <div className="my-2 h-px bg-white/[0.06]" />

          <SettingRow
            title={t("Закрывать в трей")}
            description={t("Кнопка закрытия прячет окно, приложение продолжает работать.")}
          >
            <Switch
              label={t("Закрывать в трей")}
              checked={settings.minimizeToTray}
              onChange={(value) => patch({ minimizeToTray: value })}
            />
          </SettingRow>

          <div className="my-2 h-px bg-white/[0.06]" />

          <div className="flex flex-wrap gap-2 pt-1">
            <Button size="sm" variant="secondary" onClick={() => void exportSettings()}>
              <DownloadIcon className="size-3.5" />
              {t("Экспорт настроек")}
            </Button>
            <Button
              size="sm"
              variant="secondary"
              onClick={() => void importSettings()}
              loading={importMutation.isPending}
            >
              <DownloadIcon className="size-3.5 rotate-180" />
              {t("Импорт настроек")}
            </Button>
          </div>
          <p className="mt-2 text-[10px] text-slate-600">
            {t("Экспорт содержит только настройки, без истории и данных дисков.")}
          </p>
        </Card>
      </div>
    </div>
  );
}
