export type Locale = "ru" | "en";

type Key = "about" | "close" | "checkUpdates" | "copyDiagnostics";

const DICTIONARY: Record<Locale, Record<Key, string>> = {
  ru: {
    about: "О программе",
    close: "Закрыть",
    checkUpdates: "Проверить обновления",
    copyDiagnostics: "Скопировать диагностику",
  },
  en: {
    about: "About",
    close: "Close",
    checkUpdates: "Check for updates",
    copyDiagnostics: "Copy diagnostics",
  },
};

export function currentLocale(): Locale {
  if (typeof localStorage === "undefined") return "ru";
  return localStorage.getItem("dm.locale") === "en" ? "en" : "ru";
}

export function t(key: Key): string {
  return DICTIONARY[currentLocale()][key];
}
