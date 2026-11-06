import { t } from "@/client/i18n";

type SparklineProps = {
  values: number[];
  max?: number;
  height?: number;
  tone?: string;
  fill?: boolean;
  className?: string;
};

const TONES: Record<string, { stroke: string; from: string }> = {
  emerald: { stroke: "#34d399", from: "rgba(52,211,153,0.28)" },
  sky: { stroke: "#38bdf8", from: "rgba(56,189,248,0.28)" },
  violet: { stroke: "#a78bfa", from: "rgba(167,139,250,0.28)" },
  amber: { stroke: "#fbbf24", from: "rgba(251,191,36,0.28)" },
  rose: { stroke: "#fb7185", from: "rgba(251,113,133,0.28)" },
};

export function Sparkline({
  values,
  max,
  height = 44,
  tone = "emerald",
  fill = true,
  className = "",
}: SparklineProps) {
  const palette = TONES[tone] ?? TONES.emerald;
  const width = 100;
  const count = values.length;

  if (count === 0) {
    return (
      <div
        className={`flex items-center justify-center rounded-lg bg-white/[0.03] text-[10px] text-slate-600 ${className}`}
        style={{ height }}
      >
        {t("нет данных")}
      </div>
    );
  }

  const ceiling = max ?? Math.max(1, ...values);
  const step = count > 1 ? width / (count - 1) : width;
  const points = values.map((value, index) => {
    const x = index * step;
    const y = height - Math.max(0, Math.min(1, value / ceiling)) * (height - 2) - 1;
    return `${x.toFixed(2)},${y.toFixed(2)}`;
  });

  const line = `M${points.join(" L")}`;
  const area = `${line} L${width},${height} L0,${height} Z`;
  const gradientId = `spark-${tone}-${count}-${Math.round(ceiling)}`;

  return (
    <svg
      className={className}
      viewBox={`0 0 ${width} ${height}`}
      preserveAspectRatio="none"
      style={{ height, width: "100%" }}
    >
      <defs>
        <linearGradient id={gradientId} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={palette.from} />
          <stop offset="100%" stopColor="rgba(0,0,0,0)" />
        </linearGradient>
      </defs>
      {fill ? <path d={area} fill={`url(#${gradientId})`} /> : null}
      <path
        d={line}
        fill="none"
        stroke={palette.stroke}
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
        vectorEffect="non-scaling-stroke"
      />
    </svg>
  );
}

export function BarMeter({
  value,
  tone = "emerald",
  height = 6,
}: {
  value: number;
  tone?: string;
  height?: number;
}) {
  const palette = TONES[tone] ?? TONES.emerald;
  return (
    <div
      className="w-full overflow-hidden rounded-full bg-white/[0.08]"
      style={{ height }}
    >
      <div
        className="h-full rounded-full transition-[width] duration-300"
        style={{
          width: `${Math.max(value > 0 ? 2 : 0, Math.min(100, value))}%`,
          background: `linear-gradient(90deg, ${palette.from}, ${palette.stroke})`,
        }}
      />
    </div>
  );
}
