import { useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { t } from "@/client/i18n";

export type PaletteItem = {
  id: string;
  label: string;
  group: string;
  icon: ReactNode;
  run: () => void;
};

export function CommandPalette({
  items,
  onClose,
}: {
  items: PaletteItem[];
  onClose: () => void;
}) {
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const timer = window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => window.clearTimeout(timer);
  }, []);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return items;
    return items.filter((item) =>
      `${item.label} ${item.group}`.toLowerCase().includes(needle),
    );
  }, [items, query]);

  const selected = Math.min(active, Math.max(0, filtered.length - 1));

  function handleKey(event: React.KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      onClose();
      return;
    }
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive((current) => Math.min(current + 1, filtered.length - 1));
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive((current) => Math.max(current - 1, 0));
      return;
    }
    if (event.key === "Enter") {
      event.preventDefault();
      filtered[selected]?.run();
    }
  }

  return (
    <div className="fixed inset-0 z-[60] flex items-start justify-center px-4 pt-[12vh]">
      <div className="absolute inset-0 bg-slate-950/70 backdrop-blur-sm" onClick={onClose} />
      <div
        className="relative w-full max-w-xl overflow-hidden rounded-2xl border border-white/10 bg-[var(--app-menu)] shadow-2xl"
        onKeyDown={handleKey}
      >
        <input
          ref={inputRef}
          value={query}
          onChange={(event) => {
            setQuery(event.target.value);
            setActive(0);
          }}
          placeholder={t("Поиск по разделам и действиям…")}
          className="w-full border-b border-white/10 bg-transparent px-4 py-3.5 text-sm text-slate-100 outline-none placeholder:text-slate-600"
        />
        <div className="max-h-[52vh] overflow-auto p-1.5">
          {filtered.length === 0 ? (
            <p className="px-3 py-6 text-center text-xs text-slate-500">{t("Ничего не найдено")}</p>
          ) : (
            filtered.map((item, index) => (
              <button
                key={item.id}
                onClick={() => item.run()}
                className={`flex w-full items-center gap-3 rounded-xl px-3 py-2 text-left transition-colors ${
                  index === selected ? "bg-emerald-500/15 text-emerald-100" : "text-slate-300 hover:bg-white/[0.05]"
                }`}
              >
                <span className="text-slate-400">{item.icon}</span>
                <span className="min-w-0 flex-1 truncate text-sm">{item.label}</span>
                <span className="shrink-0 text-[10px] uppercase tracking-wide text-slate-600">{item.group}</span>
              </button>
            ))
          )}
        </div>
        <div className="flex items-center justify-between border-t border-white/[0.07] px-4 py-2 text-[10px] text-slate-600">
          <span>{t("Стрелки — выбор, Enter — открыть")}</span>
          <span>Esc</span>
        </div>
      </div>
    </div>
  );
}
