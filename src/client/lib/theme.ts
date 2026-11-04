export type Theme = "dark" | "light";

const STORAGE_KEY = "disk-master:theme";

export function readStoredTheme(): Theme {
  try {
    const saved = window.localStorage.getItem(STORAGE_KEY);
    if (saved === "light" || saved === "dark") return saved;
  } catch {
    /* storage may be unavailable */
  }
  // The interface was designed dark-first, so that stays the default; the
  // light theme is one click away and is remembered from then on.
  return "dark";
}

export function applyTheme(theme: Theme): void {
  document.documentElement.dataset.theme = theme;
  try {
    window.localStorage.setItem(STORAGE_KEY, theme);
  } catch {
    /* ignore */
  }
}

/** Reads the theme the document already has, avoiding a flash on startup. */
export function currentTheme(): Theme {
  return document.documentElement.dataset.theme === "light" ? "light" : "dark";
}

/** Applies the saved theme before the first render. */
export function initTheme(): Theme {
  const theme = readStoredTheme();
  applyTheme(theme);
  return theme;
}
