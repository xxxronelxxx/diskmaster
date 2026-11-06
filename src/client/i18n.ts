export type Locale = "ru" | "en";

const STORAGE_KEY = "dm.locale";
const DEFAULT_LOCALE: Locale = "ru";

import { EN_TRANSLATIONS } from "./i18n-en";

/**
 * English translations keyed by the Russian source string. A missing entry
 * falls back to the Russian text, so new UI strings never break the build.
 */
const OVERRIDES: Record<string, string> = {
  "Автозагрузка": "Startup",
  "Автозапуск": "Autostart",
  "Анализ диска": "Disk analysis",
  "в": "in",
  "В корзину": "To Recycle Bin",
  "в корзину": "to Recycle Bin",
  "В корзину отправлено": "Moved to Recycle Bin",
  "Время работы": "Uptime",
  "Готово: обновлено": "Done: updated",
  "Датчики": "Sensors",
  "датч.": "sensors",
  "Диски": "Disks",
  "Диск": "Disk",
  "Журнал": "Log",
  "журнал": "log",
  "Завершить": "End",
  "занято": "used",
  "Запускаем датчики…": "Starting sensors…",
  "Запускаем установщик…": "Starting the installer…",
  "Запускать вместе с Windows": "Start with Windows",
  "Запустить": "Start",
  "Запустить DISM": "Run DISM",
  "Запустить сейчас": "Run now",
  "История места": "Space history",
  "Корзина": "Recycle Bin",
  "Мало свободного места": "Low disk space",
  "Мгновенная загрузка": "Instant load",
  "Можно освободить": "Can free up",
  "На экран": "Show",
  "Начать проверку": "Start scan",
  "Насколько виджет пропускает обои и окна": "How much wallpaper and windows the widget shows through",
  "Обзор": "Overview",
  "Обновить": "Refresh",
  "Обновить всё": "Update all",
  "Обслуживание": "Maintenance",
  "Обычные права": "Standard rights",
  "Обычный": "Normal",
  "Очистить": "Clean",
  "Очистить от администратора": "Clean as administrator",
  "Очистить WinSxS?": "Clean WinSxS?",
  "Очистка": "Cleanup",
  "Операционная система": "Operating system",
  "Освободить место": "Free up space",
  "Освобождено": "Freed",
  "Плата": "Motherboard",
  "Поверх всех": "Always on top",
  "Показать в проводнике": "Show in Explorer",
  "При входе": "At login",
  "Проверить": "Check",
  "Проверить мусор": "Scan for junk",
  "Пробный запуск": "Dry run",
  "Пробный запуск: будет удалено": "Dry run: will be deleted",
  "Простой": "Idle",
  "Процесс работает": "Process is running",
  "Процессы по памяти": "Processes by memory",
  "Права": "Privileges",
  "Самый тяжёлый процесс": "Top process",
  "Снять сейчас": "Take snapshot now",
  "ТЕМП": "TEMP",
  "Температуры и кулеры": "Temperatures and fans",
  "Точка": "Point",
  "Удалить старую точку?": "Delete the oldest restore point?",
  "Управление процессом": "Manage process",
  "Установить датчики": "Install sensors",
  "Уведомление после очистки из меню в трее.": "Notification after cleanup from the tray menu.",
  "файлов": "files",
  "частично": "partial",
  "Скан завершён ·": "Scan complete ·",
  "мусора": "junk",
  "Свободно": "Free",
  "СВОБОДНО": "FREE",
  "Сеть": "Network",
  "СЕТЬ": "NET",
  "ДИСК": "DISK",
  "Права доступа": "Access",
  "Системные категории от администратора": "System categories (administrator)",
  "Установлена последняя версия": "You have the latest version",
  "Установленные программы": "Installed programs",
  "Язык интерфейса": "Interface language",
  "Ядра процессора": "CPU cores",
  "Windows · очистка и программы": "Windows · cleanup and programs",
  "О программе": "About",
  "Закрыть": "Close",
  "Скопировать диагностику": "Copy diagnostics",
  "Проверить обновления": "Check for updates",
  "Скопировано": "Copied",
  "Открыть журнал": "Open log",
  "Обслуживание системы": "System maintenance",
  "Проверка выполняется каждые 3 минуты в фоне.": "Checks run every 3 minutes in the background.",
  "Обновление каждую секунду · {points} точек истории · адаптер {adapter}":
    "Updated every second · {points} history points · adapter {adapter}",
  "{count} ядер": "{count} cores",
  "из {total}": "of {total}",
  "{count} локальных диска · свободно {free} из {total}":
    "{count} local disks · {free} free of {total}",
  "{used} из {total} ({percent}%)": "{used} of {total} ({percent}%)",
  "{count} программ · {size} по данным реестра":
    "{count} programs · {size} according to the registry",
  " · {date}": " · {date}",
  "Открыть папку установки: {path}": "Open installation folder: {path}",
  "Очищено {size} · {files} объектов": "Cleaned {size} · {files} items",
  "Следующий запуск: {date}": "Next run: {date}",
  "Не удалось зарегистрировать: {errors}": "Failed to register: {errors}",
  "Настройки сохранены в {path}": "Settings saved to {path}",
  "{name} удалён из автозагрузки": "{name} removed from startup",
  "{count} программ запускается при входе · реестр, папка Startup и планировщик":
    "{count} programs start at login · registry, Startup folder and scheduler",
  "{count} пакетов можно обновить через winget":
    "{count} packages can be updated via winget",
  ", с ошибкой {count}": ", with {count} failed",
  "Скачивание обновления… {percent}%": "Downloading the update… {percent}%",
  "Управление процессом {name}": "Manage process {name}",
  "Завершить процесс {name}": "End process {name}",
  "выключены: {error}": "off: {error}",
  "{count} датч.": "{count} sensors",
  "Скан завершён · {size} мусора": "Scan complete · {size} junk",
  "Сканирование {percent}%": "Scanning {percent}%",
  "В корзину отправлено {files} файлов · {bytes}":
    "Moved to Recycle Bin: {files} files · {bytes}",
  "Ошибки: {errors}": "Errors: {errors}",
  " · проверено {hashed} из {total}": " · checked {hashed} of {total}",
  "Убрать исключение: {path}": "Remove exclusion: {path}",
  "Файлов больше {size} КБ с одинаковым содержимым нет. Можно снизить минимальный размер и проверить снова.":
    "No files larger than {size} KB with identical content. Lower the minimum size and scan again.",
  " · {files} файлов": " · {files} files",
  "{files} файлов": "{files} files",
  "Удалено {bytes}, ошибки: {errors}": "Deleted {bytes}, errors: {errors}",
  "Удалено {bytes} ({files} файлов): {names}": "Deleted {bytes} ({files} files): {names}",
  " и ещё {count}": " and {count} more",
  "Запустим поиск файлов больше {size} МБ в вашем профиле и на остальных локальных дисках. Сканирование выводит и категории мусора — оба раздела используют один проход.":
    "Scan for files larger than {size} MB in your profile and other local disks. The same pass also fills the junk categories.",
  "Файлов больше {size} МБ на дисках нет. Можно снизить порог или проверить категории мусора.":
    "No files larger than {size} MB were found. Lower the threshold or scan for junk.",
  " · {value} °C": " · {value} °C",
  " · износ {value}%": " · wear {value}%",
  " · {value} ч": " · {value} h",
  "Точка {sequence}": "Point {sequence}",
  "Будет удалена точка №{sequence}.": "Restore point #{sequence} will be deleted.",
  "Итог быстрой очистки": "Quick cleanup result",
  "Уведомления, горячие клавиши, защищённые папки и резервная копия настроек":
    "Notifications, hotkeys, protected folders and settings backup",
  "Портативная версия: автозапуск недоступен, настройки лежат рядом с программой. Системные уведомления могут не показываться без ярлыка в меню «Пуск» — в установленной версии они работают.":
    "Portable build: autostart is unavailable and settings live next to the program. System notifications may not appear without a Start menu shortcut — they work in the installed build.",
  "Температура процессора": "CPU temperature",
  "Температура накопителя": "Drive temperature",
  "Датчик 1": "Sensor 1",
  "Датчик 2": "Sensor 2",
  "Датчик 3": "Sensor 3",
  "Датчик 4": "Sensor 4",
  "Вентилятор процессора": "CPU fan",
  "Вентилятор видеокарты": "GPU fan",
  "Корпусной вентилятор": "Case fan",
  "Вентилятор": "Fan",
  "Обороты вентилятора": "Fan speed",
  "Горячая точка": "Hot spot",
  "Память видеокарты": "GPU memory",
  "Ядро видеокарты": "GPU core",
  "Кристалл": "Die",
  "Максимум по ядрам": "Core max",
  "Среднее по ядрам": "Core average",
  "ДИСК-МАСТЕР": "DISK MASTER",
  "О программе · v": "About · v",
  "без прав администратора": "standard user",
  "свободно {size}": "{size} free",
  "Проверить уведомление": "Test notification",
  "Прочитать SMART (UAC)": "Read SMART (UAC)",
  "SMART: критическое предупреждение (0x{code})": "SMART: critical warning (0x{code})",
  " · ошибок {value}": " · errors {value}",
};

const EN: Record<string, string> = { ...EN_TRANSLATIONS, ...OVERRIDES };

const ERROR_EN: Record<string, string> = {
  "Запрос прав администратора отменён": "The administrator permission request was cancelled",
  "Не удалось запустить модуль": "Could not start the module",
  "Нужны права администратора": "Administrator rights are required",
  "Автозапуск недоступен в портативной версии": "Autostart is unavailable in the portable build",
  "Модуль датчиков не найден": "The sensor module was not found",
  "PawnIO_setup.exe не найден в сборке. Пересоберите приложение.":
    "PawnIO_setup.exe is missing from the build. Rebuild the application.",
  "Датчики не запустились за 15 секунд": "The sensors did not start within 15 seconds",
  "Установка не завершилась за 3 минуты": "The installation did not finish within 3 minutes",
  "Не удалось установить PawnIO": "Could not install PawnIO",
  "Не удалось сохранить настройки": "Could not save settings",
  "Некорректный JSON": "Invalid JSON",
  "Выберите файл с расширением .json": "Choose a .json file",
  "Путь должен быть абсолютным": "The path must be absolute",
  "Процесс отсутствует в списке активных": "The process is not in the active list",
  "Процесс уже завершён": "The process has already exited",
  "Нельзя завершить собственный процесс": "You cannot end your own process",
  "Не удалось завершить процесс (нужны права)": "Could not end the process (rights required)",
  "Недопустимый идентификатор процесса": "Invalid process id",
  "Не удалось открыть процесс — нужны права администратора":
    "Could not open the process — administrator rights required",
  "Не удалось изменить приоритет": "Could not change the priority",
  "Системный процесс": "System process",
  "Папка защищена в настройках": "The folder is protected in settings",
  "Защищённый системный путь": "Protected system path",
  "Символические ссылки и junction-папки защищены": "Symlinks and junctions are protected",
  "Не найдено": "Not found",
  "Windows.old удалена": "Windows.old removed",
  "Папка Windows.old не найдена": "Windows.old folder not found",
  "Очистка WinSxS завершена": "WinSxS cleanup completed",
  "DISM завершился с ошибкой": "DISM failed",
  "Точка восстановления создана": "Restore point created",
  "Список точек восстановления получен": "Restore point list loaded",
  "Точка восстановления удалена": "Restore point deleted",
  "Системные категории очищены": "System categories cleaned",
  "Не удалось удалить Windows.old": "Could not remove Windows.old",
  "Не удалось создать точку восстановления": "Could not create a restore point",
  "Не удалось удалить точку восстановления": "Could not delete the restore point",
};

let current: Locale = readLocale();

function readLocale(): Locale {
  try {
    if (typeof localStorage !== "undefined" && localStorage.getItem(STORAGE_KEY) === "en") {
      return "en";
    }
  } catch {
    // localStorage can be unavailable in hardened WebView environments
  }
  return DEFAULT_LOCALE;
}

export function currentLocale(): Locale {
  return current;
}

export function setLocale(next: Locale): void {
  current = next;
  try {
    localStorage.setItem(STORAGE_KEY, next);
  } catch {
    // ignore
  }
  if (typeof document !== "undefined") {
    document.documentElement.lang = next;
  }
}

/** Applies the current locale to the document on first render. */
export function initLocale(): Locale {
  if (typeof document !== "undefined") {
    document.documentElement.lang = current;
  }
  return current;
}

/**
 * Translates a source string (Russian). Parameters are substituted into
 * `{name}` placeholders.
 */
export function t(source: string, params?: Record<string, string | number>): string {
  let text = current === "en" ? (EN[source] ?? source) : source;
  if (params) {
    for (const [key, value] of Object.entries(params)) {
      text = text.split(`{${key}}`).join(String(value));
    }
  }
  return text;
}

/** Translates backend error text. Unknown messages keep their original text. */
export function translateError(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  if (current !== "en") return raw;
  if (ERROR_EN[raw]) return ERROR_EN[raw];
  for (const [source, translated] of Object.entries(ERROR_EN)) {
    if (raw.startsWith(source)) {
      return `${translated}${raw.slice(source.length)}`;
    }
  }
  return raw;
}

/** Russian plural form helper (1 файл / 2 файла / 5 файлов). */
export function pluralRu(count: number, one: string, few: string, many: string): string {
  const value = Math.abs(count) % 100;
  const last = value % 10;
  if (value > 10 && value < 20) return many;
  if (last > 1 && last < 5) return few;
  if (last === 1) return one;
  return many;
}

export function registerTranslations(dictionary: Record<string, string>): void {
  Object.assign(EN, dictionary);
}
