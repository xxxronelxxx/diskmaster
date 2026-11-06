/* eslint-disable react-refresh/only-export-components */
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { initLocale, setLocale, type Locale } from "./i18n";

type I18nValue = {
  locale: Locale;
  changeLocale: (locale: Locale) => void;
};

const I18nContext = createContext<I18nValue>({
  locale: "ru",
  changeLocale: () => undefined,
});

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(() => initLocale());

  const changeLocale = useCallback((next: Locale) => {
    setLocale(next);
    setLocaleState(next);
  }, []);

  useEffect(() => {
    void invoke("set_locale", { localeName: locale }).catch(() => undefined);
  }, [locale]);

  const value = useMemo(() => ({ locale, changeLocale }), [locale, changeLocale]);
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18nValue {
  return useContext(I18nContext);
}
