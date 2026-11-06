export type Locale = "ru" | "en";

const STORAGE_KEY = "dm.locale";
const DEFAULT_LOCALE: Locale = "ru";

/**
 * English translations keyed by the Russian source string. A missing entry
 * falls back to the Russian text, so new UI strings never break the build.
 */
const EN: Record<string, string> = {};

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
