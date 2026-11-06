import { useEffect } from "react";
import type { ButtonHTMLAttributes, ReactNode } from "react";
import { SpinnerIcon } from "./icons";

export function Card({
  className = "",
  children,
}: {
  className?: string;
  children: ReactNode;
}) {
  return (
    <div className={`rounded-2xl border border-white/10 bg-white/[0.035] ${className}`}>
      {children}
    </div>
  );
}

type ButtonVariant = "primary" | "secondary" | "ghost" | "danger" | "success";
type ButtonSize = "sm" | "md" | "lg";

const VARIANTS: Record<ButtonVariant, string> = {
  primary:
    "bg-emerald-500 text-emerald-950 hover:bg-emerald-400 font-semibold shadow-[0_10px_30px_-14px_rgba(16,185,129,0.9)]",
  success:
    "bg-emerald-500/15 text-emerald-200 border border-emerald-400/30 hover:bg-emerald-500/25",
  secondary:
    "bg-white/[0.06] text-slate-100 border border-white/10 hover:bg-white/10",
  ghost: "text-slate-300 hover:text-white hover:bg-white/[0.07]",
  danger:
    "bg-rose-500/15 text-rose-200 border border-rose-400/30 hover:bg-rose-500/25",
};

const SIZES: Record<ButtonSize, string> = {
  sm: "px-2.5 py-1.5 text-xs gap-1.5 rounded-lg",
  md: "px-3.5 py-2 text-sm gap-2 rounded-xl",
  lg: "px-5 py-2.5 text-sm gap-2 rounded-xl",
};

export function Button({
  variant = "secondary",
  size = "md",
  className = "",
  loading = false,
  children,
  disabled,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: ButtonVariant;
  size?: ButtonSize;
  loading?: boolean;
}) {
  return (
    <button
      {...props}
      disabled={disabled || loading}
      className={`inline-flex items-center justify-center whitespace-nowrap transition-colors disabled:cursor-not-allowed disabled:opacity-45 ${VARIANTS[variant]} ${SIZES[size]} ${className}`}
    >
      {loading ? <SpinnerIcon className="size-4" /> : null}
      {children}
    </button>
  );
}

export function Progress({
  value,
  tone = "emerald",
  className = "",
}: {
  value: number;
  tone?: "emerald" | "amber" | "rose" | "sky" | "violet";
  className?: string;
}) {
  const tones: Record<string, string> = {
    emerald: "from-emerald-400 to-teal-400",
    amber: "from-amber-400 to-orange-400",
    rose: "from-rose-500 to-red-400",
    sky: "from-sky-400 to-cyan-400",
    violet: "from-violet-400 to-fuchsia-400",
  };
  return (
    <div className={`h-1.5 w-full overflow-hidden rounded-full bg-white/10 ${className}`}>
      <div
        className={`h-full rounded-full bg-gradient-to-r ${tones[tone]} transition-[width] duration-500`}
        style={{ width: `${Math.max(value > 0 ? 1.5 : 0, Math.min(100, value))}%` }}
      />
    </div>
  );
}

type BadgeVariant = "neutral" | "success" | "warn" | "danger" | "info";

const BADGES: Record<BadgeVariant, string> = {
  neutral: "bg-white/[0.07] text-slate-300 border-white/10",
  success: "bg-emerald-500/12 text-emerald-300 border-emerald-400/25",
  warn: "bg-amber-400/12 text-amber-200 border-amber-400/25",
  danger: "bg-rose-500/12 text-rose-200 border-rose-400/25",
  info: "bg-sky-500/12 text-sky-200 border-sky-400/25",
};

export function Badge({
  variant = "neutral",
  className = "",
  children,
}: {
  variant?: BadgeVariant;
  className?: string;
  children: ReactNode;
}) {
  return (
    <span
      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-[11px] font-medium ${BADGES[variant]} ${className}`}
    >
      {children}
    </span>
  );
}

export function Modal({
  open,
  onClose,
  title,
  description,
  children,
  footer,
}: {
  open: boolean;
  onClose: () => void;
  title: string;
  description?: ReactNode;
  children?: ReactNode;
  footer?: ReactNode;
}) {
  useEffect(() => {
    if (!open) return;
    const handler = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div
        className="absolute inset-0 bg-slate-950/70 backdrop-blur-sm"
        onClick={onClose}
      />
      <div className="relative w-full max-w-lg rounded-2xl border border-white/10 bg-[var(--app-menu)] p-5 shadow-2xl">
        <h3 className="text-base font-semibold text-white">{title}</h3>
        {description ? (
          <div className="mt-1.5 text-sm leading-relaxed text-slate-400">{description}</div>
        ) : null}
        {children ? <div className="mt-4">{children}</div> : null}
        {footer ? <div className="mt-5 flex justify-end gap-2">{footer}</div> : null}
      </div>
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  description,
  action,
}: {
  icon: ReactNode;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-col items-center gap-3 rounded-2xl border border-dashed border-white/12 bg-white/[0.02] px-6 py-12 text-center">
      <div className="grid size-12 place-items-center rounded-2xl bg-white/[0.06] text-slate-300">
        {icon}
      </div>
      <div>
        <p className="text-sm font-medium text-slate-100">{title}</p>
        <p className="mx-auto mt-1 max-w-sm text-xs leading-relaxed text-slate-400">{description}</p>
      </div>
      {action}
    </div>
  );
}

export function SectionTitle({
  title,
  subtitle,
  action,
}: {
  title: string;
  subtitle?: string;
  action?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-3">
      <div>
        <h2 className="text-lg font-semibold tracking-tight text-white">{title}</h2>
        {subtitle ? <p className="mt-0.5 text-xs text-slate-400">{subtitle}</p> : null}
      </div>
      {action}
    </div>
  );
}

export function Switch({
  checked,
  onChange,
  disabled = false,
  label,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  disabled?: boolean;
  label?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!checked)}
      className={`relative h-5 w-9 shrink-0 rounded-full border transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
        checked ? "border-emerald-400/40 bg-emerald-500/80" : "border-white/10 bg-white/10"
      }`}
    >
      <span
        className={`absolute left-0.5 top-0.5 size-4 rounded-full bg-white shadow transition-transform ${
          checked ? "translate-x-4" : ""
        }`}
      />
    </button>
  );
}
