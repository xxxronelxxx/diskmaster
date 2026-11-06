import type { FolderEntry } from "@/shared/types";
import { formatBytes } from "@/client/lib/format";

type Rect = { x: number; y: number; w: number; h: number };
type Cell = { entry: FolderEntry; rect: Rect; index: number };

const COLORS = [
  "#34d399",
  "#38bdf8",
  "#a78bfa",
  "#fbbf24",
  "#fb7185",
  "#f472b6",
  "#22d3ee",
  "#84cc16",
  "#f97316",
  "#818cf8",
];

/** Simple binary slice-and-dice treemap: predictable and fast for ≤500 items. */
function layout(entries: FolderEntry[], rect: Rect): Cell[] {
  if (entries.length === 0) return [];
  if (entries.length === 1) {
    return [{ entry: entries[0], rect, index: 0 }];
  }

  const total = entries.reduce((sum, entry) => sum + Math.max(1, entry.size), 0);
  let running = 0;
  let split = 1;
  for (let index = 0; index < entries.length - 1; index += 1) {
    running += Math.max(1, entries[index].size);
    split = index + 1;
    if (running >= total / 2) break;
  }

  const first = entries.slice(0, split);
  const second = entries.slice(split);
  const firstSize = first.reduce((sum, entry) => sum + Math.max(1, entry.size), 0);
  const ratio = firstSize / total;

  if (rect.w >= rect.h) {
    const width = rect.w * ratio;
    return [
      ...layout(first, { x: rect.x, y: rect.y, w: width, h: rect.h }),
      ...layout(second, { x: rect.x + width, y: rect.y, w: rect.w - width, h: rect.h }),
    ].map((cell, index) => ({ ...cell, index }));
  }

  const height = rect.h * ratio;
  return [
    ...layout(first, { x: rect.x, y: rect.y, w: rect.w, h: height }),
    ...layout(second, { x: rect.x, y: rect.y + height, w: rect.w, h: rect.h - height }),
  ].map((cell, index) => ({ ...cell, index }));
}

export function Treemap({
  entries,
  total,
  onOpen,
}: {
  entries: FolderEntry[];
  total: number;
  onOpen: (path: string) => void;
}) {
  const sorted = [...entries].sort((a, b) => b.size - a.size);
  const cells = layout(sorted, { x: 0, y: 0, w: 100, h: 100 });

  return (
    <div className="h-[520px] w-full overflow-hidden bg-[#0a0f18]">
      <svg viewBox="0 0 100 100" preserveAspectRatio="none" className="h-full w-full">
        {cells.map((cell) => {
          const share = total > 0 ? cell.entry.size / total : 0;
          const color = COLORS[cell.index % COLORS.length];
          const showText = share > 0.035;
          return (
            <g key={cell.entry.path}>
              <rect
                x={cell.rect.x + 0.12}
                y={cell.rect.y + 0.12}
                width={Math.max(0.1, cell.rect.w - 0.24)}
                height={Math.max(0.1, cell.rect.h - 0.24)}
                rx={0.5}
                fill={color}
                fillOpacity={cell.entry.isDir ? 0.32 : 0.2}
                stroke={color}
                strokeOpacity={0.55}
                strokeWidth={0.12}
                style={{ cursor: cell.entry.isDir ? "pointer" : "default" }}
                onClick={() => {
                  if (cell.entry.isDir) onOpen(cell.entry.path);
                }}
              >
                <title>
                  {cell.entry.path} · {formatBytes(cell.entry.size)}
                </title>
              </rect>
              {showText ? (
                <text
                  x={cell.rect.x + 0.8}
                  y={cell.rect.y + 2.4}
                  fontSize={Math.min(2.2, Math.max(1.1, cell.rect.w / 12))}
                  fill="#e2e8f0"
                  style={{ pointerEvents: "none", fontWeight: 600 }}
                >
                  {cell.entry.name.length > 22
                    ? `${cell.entry.name.slice(0, 20)}…`
                    : cell.entry.name}
                </text>
              ) : null}
              {share > 0.02 ? (
                <text
                  x={cell.rect.x + 0.8}
                  y={cell.rect.y + 4.6}
                  fontSize={Math.min(1.8, Math.max(0.9, cell.rect.w / 16))}
                  fill="#cbd5e1"
                  fillOpacity={0.8}
                  style={{ pointerEvents: "none" }}
                >
                  {formatBytes(cell.entry.size)}
                </text>
              ) : null}
            </g>
          );
        })}
      </svg>
    </div>
  );
}
