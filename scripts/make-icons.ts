/**
 * Draws the app icon procedurally (no external image tools):
 * a dark rounded square with an emerald gauge, exported as PNG and ICO.
 */
import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";
import { deflateSync } from "node:zlib";

type Rgba = [number, number, number, number];

function mix(a: Rgba, b: Rgba, t: number): Rgba {
  return [
    a[0] + (b[0] - a[0]) * t,
    a[1] + (b[1] - a[1]) * t,
    a[2] + (b[2] - a[2]) * t,
    a[3] + (b[3] - a[3]) * t,
  ];
}

const clamp01 = (value: number) => Math.max(0, Math.min(1, value));

/** Anti-aliased coverage of a rounded rectangle. */
function roundedRectCoverage(x: number, y: number, size: number, radius: number): number {
  const inset = size * 0.03;
  const left = inset;
  const top = inset;
  const right = size - inset;
  const bottom = size - inset;
  const r = radius;

  const cx = Math.min(Math.max(x, left + r), right - r);
  const cy = Math.min(Math.max(y, top + r), bottom - r);
  const dx = x - cx;
  const dy = y - cy;
  const dist = Math.hypot(dx, dy);
  const edge = 1.2;

  if (x < left - edge || x > right + edge || y < top - edge || y > bottom + edge) return 0;
  if (x >= left + r && x <= right - r) return 1;
  if (y >= top + r && y <= bottom - r) return 1;
  return clamp01((r - dist) / edge + 0.5);
}

function renderIcon(size: number): Buffer {
  const buffer = Buffer.alloc(size * size * 4);
  const center = size / 2;
  const outer = size * 0.34;
  const inner = size * 0.235;
  const stroke = size * 0.075;

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      const px = x + 0.5;
      const py = y + 0.5;

      const cover = roundedRectCoverage(px, py, size, size * 0.235);

      // Background: vertical gradient from deep slate to near-black emerald.
      const gradient = mix([18, 26, 38, 255], [7, 14, 22, 255], py / size);
      let color = gradient;

      // Gauge arc from 150° to 390° (open at the bottom).
      const dx = px - center;
      const dy = py - center;
      const radius = Math.hypot(dx, dy);
      let angle = (Math.atan2(dy, dx) * 180) / Math.PI;
      if (angle < 0) angle += 360;
      const inArc = angle >= 150 && angle <= 390 - 360 + 360 ? angle >= 150 : angle <= 30;

      if (inArc && Math.abs(radius - (outer + inner) / 2) <= stroke / 2) {
        const t = (angle >= 150 ? angle - 150 : angle + 210) / 240;
        const emerald = mix([52, 211, 153, 255], [16, 185, 129, 255], clamp01(t));
        color = mix(color, emerald, 0.96);
      }

      // Needle pointing to ~65 % of the scale.
      const needleAngle = ((150 + 0.65 * 240) * Math.PI) / 180;
      const nx = Math.cos(needleAngle);
      const ny = Math.sin(needleAngle);
      const projection = dx * nx + dy * ny;
      const perpendicular = Math.abs(dx * -ny + dy * nx);
      const needleLength = outer * 0.82;
      if (projection > 0 && projection < needleLength && perpendicular < size * 0.035) {
        color = mix(color, [236, 253, 245, 255], 0.95);
      }

      // Hub.
      if (radius < size * 0.075) {
        color = mix(color, [16, 185, 129, 255], 0.95);
      }

      const index = (y * size + x) * 4;
      buffer[index] = Math.round(color[0]);
      buffer[index + 1] = Math.round(color[1]);
      buffer[index + 2] = Math.round(color[2]);
      buffer[index + 3] = Math.round(color[3] * cover);
    }
  }

  return buffer;
}

function png(size: number): Buffer {
  const pixels = renderIcon(size);

  // Raw scanlines with filter byte 0.
  const stride = size * 4;
  const raw = Buffer.alloc((stride + 1) * size);
  for (let y = 0; y < size; y++) {
    raw[y * (stride + 1)] = 0;
    pixels.copy(raw, y * (stride + 1) + 1, y * stride, (y + 1) * stride);
  }

  const chunk = (type: string, data: Buffer): Buffer => {
    const length = Buffer.alloc(4);
    length.writeUInt32BE(data.length);
    const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
    const crc = Buffer.alloc(4);
    crc.writeUInt32BE(crc32(body) >>> 0);
    return Buffer.concat([length, body, crc]);
  };

  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  ihdr[10] = 0;
  ihdr[11] = 0;
  ihdr[12] = 0;

  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

let crcTable: number[] | null = null;
function crc32(buffer: Buffer): number {
  if (!crcTable) {
    crcTable = [];
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crcTable[n] = c >>> 0;
    }
  }
  let crc = 0xffffffff;
  for (const byte of buffer) crc = crcTable[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  return (crc ^ 0xffffffff) >>> 0;
}

/** ICO container with PNG-compressed entries (supported since Vista). */
function ico(sizes: number[]): Buffer {
  const images = sizes.map((size) => ({ size, data: png(size) }));

  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(images.length, 4);

  const entries: Buffer[] = [];
  let offset = 6 + images.length * 16;

  for (const image of images) {
    const entry = Buffer.alloc(16);
    entry[0] = image.size >= 256 ? 0 : image.size;
    entry[1] = image.size >= 256 ? 0 : image.size;
    entry[2] = 0;
    entry[3] = 0;
    entry.writeUInt16LE(1, 4);
    entry.writeUInt16LE(32, 6);
    entry.writeUInt32LE(image.data.length, 8);
    entry.writeUInt32LE(offset, 12);
    offset += image.data.length;
    entries.push(entry);
  }

  return Buffer.concat([header, ...entries, ...images.map((image) => image.data)]);
}

const outDir = path.resolve("./build");
await mkdir(outDir, { recursive: true });

await writeFile(path.join(outDir, "icon.ico"), ico([256, 128, 64, 48, 32, 16]));
await writeFile(path.join(outDir, "icon.png"), png(512));
await writeFile(path.join(outDir, "tray.png"), png(32));

console.log("иконки: build/icon.ico, build/icon.png, build/tray.png");
