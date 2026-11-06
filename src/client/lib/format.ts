import { currentLocale } from "@/client/i18n";

function intlLocale(): string {
  return currentLocale() === "en" ? "en-US" : "ru-RU";
}

export function formatBytes(bytes: number, digits = 1): string {
  const english = currentLocale() === "en";
  if (!bytes || bytes <= 0) return english ? "0 B" : "0 Б";
  const units = english ? ["B", "KB", "MB", "GB", "TB"] : ["Б", "КБ", "МБ", "ГБ", "ТБ"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  const decimals = unit === 0 ? 0 : value >= 100 ? 0 : digits;
  const formatted = new Intl.NumberFormat(intlLocale(), {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  }).format(value);
  return `${formatted} ${units[unit]}`;
}

export function formatCount(value: number): string {
  return new Intl.NumberFormat(intlLocale()).format(value);
}

export function formatDate(value: number | string | null): string {
  if (!value) return "—";
  const date =
    typeof value === "number"
      ? new Date(value)
      : /^\d{8}$/.test(value)
        ? new Date(
          Number(value.slice(0, 4)),
          Number(value.slice(4, 6)) - 1,
          Number(value.slice(6, 8)),
        )
        : new Date(value);
  if (Number.isNaN(date.getTime())) return "—";
  return new Intl.DateTimeFormat(intlLocale(), {
    day: "2-digit",
    month: "short",
    year: "numeric",
  }).format(date);
}

export function formatUptime(lastBoot: string | null): string {
  if (!lastBoot) return "—";
  const started = new Date(lastBoot).getTime();
  if (Number.isNaN(started)) return "—";
  const minutes = Math.max(0, Math.floor((Date.now() - started) / 60000));
  const days = Math.floor(minutes / 1440);
  const hours = Math.floor((minutes % 1440) / 60);
  if (currentLocale() === "en") {
    if (days > 0) return `${days} d ${hours} h`;
    return `${hours} h ${minutes % 60} min`;
  }
  if (days > 0) return `${days} д ${hours} ч`;
  return `${hours} ч ${minutes % 60} мин`;
}

export function usedPercent(used: number, total: number): number {
  if (!total) return 0;
  return Math.min(100, Math.round((used / total) * 100));
}

export function shortPath(value: string, max = 64): string {
  if (value.length <= max) return value;
  return `…${value.slice(value.length - max + 1)}`;
}
