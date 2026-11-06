/**
 * Builds the portable folder: one executable, a marker file that switches the
 * app to portable storage, and a short readme. No installer, no self-extraction.
 *
 * Run: pnpm build:portable
 */
import { execSync } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import path from "node:path";

const root = process.cwd();
const version = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")).version as string;
const target = path.join(root, "output", `DiskMaster-portable-${version}`);
const exeCandidates = [
  path.join(root, "src-tauri", "target", "release", "DiskMaster.exe"),
  path.join(root, "src-tauri", "target", "release", "disk-master.exe"),
];

const log = (text: string) => console.log(`\n[portable] ${text}`);

log("version");
execSync("pnpm version:sync", { stdio: "inherit", cwd: root });

log("interface");
execSync("pnpm build:web", { stdio: "inherit", cwd: root });

log("release build");
execSync("pnpm tauri build --no-bundle", { stdio: "inherit", cwd: root });

const exe = exeCandidates.find((candidate) => existsSync(candidate));
if (!exe) {
  throw new Error("release executable not found — expected src-tauri/target/release/DiskMaster.exe");
}

try {
  rmSync(target, { recursive: true, force: true });
  mkdirSync(target, { recursive: true });
} catch (error) {
  throw new Error(
    `Не удалось обновить папку ${target}. Закройте запущенный DiskMaster.exe и повторите.
${String(error)}`,
  );
}

const destination = path.join(target, "DiskMaster.exe");
try {
  copyFileSync(exe, destination);
} catch (error) {
  throw new Error(
    `Не удалось скопировать DiskMaster.exe — возможно, приложение запущено.
${String(error)}`,
  );
}
writeFileSync(path.join(target, "portable.marker"), "", "utf8");
writeFileSync(
  path.join(target, "README.txt"),
  [
    "Диск-Мастер — портативная версия",
    "",
    "Запуск: DiskMaster.exe",
    "",
    "Данные (история, настройки виджета, журнал расписания) хранятся",
    "в папке disk-master-data рядом с исполняемым файлом.",
    "Чтобы перенести приложение, скопируйте всю папку целиком.",
    "",
    "Требуется Windows 11 (WebView2 уже входит в систему).",
    "Автообновление доступно только в версии, установленной через установщик.",
    "",
  ].join("\r\n"),
  "utf8",
);

const sizeMb = (statSync(destination).size / 1024 / 1024).toFixed(1);
log(`готово: ${path.relative(root, target)} (${sizeMb} МБ)`);
