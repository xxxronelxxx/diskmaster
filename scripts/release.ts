/**
 * Publishes a release: signed updater artifacts, installer, portable zip,
 * latest.json and SHA-256 sums, then creates the GitHub release.
 *
 * Usage:
 *   pnpm release                 # stable release
 *   pnpm release --prerelease    # test channel, ignored by releases/latest
 *   pnpm release --draft
 *
 * Requires the updater private key in ~/.tauri/diskmaster.key (or
 * TAURI_SIGNING_PRIVATE_KEY) and an authenticated `gh` CLI.
 */
import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from "node:fs";
import os from "node:os";
import path from "node:path";

const root = process.cwd();
const args = new Set(process.argv.slice(2));
const prerelease = args.has("--prerelease");
const draft = args.has("--draft");
const dryRun = args.has("--dry-run");
const notesArgIndex = process.argv.indexOf("--notes-file");
const notesFile = notesArgIndex > 0 ? process.argv[notesArgIndex + 1] : null;

const pkg = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")) as {
  version: string;
};
const version = pkg.version;
const tag = `v${version}`;
const repo = "xxxronelxxx/diskmaster";
const baseUrl = `https://github.com/${repo}/releases/download/${tag}`;

const keyPath = path.join(os.homedir(), ".tauri", "diskmaster.key");
if (!process.env.TAURI_SIGNING_PRIVATE_KEY) {
  if (!existsSync(keyPath)) {
    throw new Error(
      `updater key not found at ${keyPath}. Generate it with: pnpm tauri signer generate -w ~/.tauri/diskmaster.key`,
    );
  }
  process.env.TAURI_SIGNING_PRIVATE_KEY = keyPath;
}

const env = { ...process.env, CI: "true" };
const run = (command: string) => execSync(command, { stdio: "inherit", cwd: root, env });

run("pnpm version:sync");
run("pnpm tauri build");

const bundleDir = path.join(root, "src-tauri", "target", "release", "bundle", "nsis");
if (!existsSync(bundleDir)) {
  throw new Error(`NSIS bundle directory not found: ${bundleDir}`);
}
const bundleFiles = readdirSync(bundleDir);
const setupExe = bundleFiles.find((name) => name.endsWith("-setup.exe"));
// Tauri 2 emits either a `.nsis.zip` updater bundle or signs the setup exe
// directly, depending on the bundler version. Support both.
const updaterFile = bundleFiles.find((name) => name.endsWith(".nsis.zip")) ?? setupExe;
const updaterSig = `${updaterFile ?? ""}.sig`;

if (!setupExe || !updaterFile || !existsSync(path.join(bundleDir, updaterSig))) {
  throw new Error(
    "updater artifacts are missing — the build must run with TAURI_SIGNING_PRIVATE_KEY set",
  );
}

// Portable folder + zip use the same release binary.
const exePath = [
  path.join(root, "src-tauri", "target", "release", "DiskMaster.exe"),
  path.join(root, "src-tauri", "target", "release", "disk-master.exe"),
].find((candidate) => existsSync(candidate));
if (!exePath) throw new Error("release executable not found");

const outputDir = path.join(root, "output");
const portableDir = path.join(outputDir, `DiskMaster-portable-${version}`);
rmSync(portableDir, { recursive: true, force: true });
mkdirSync(portableDir, { recursive: true });
copyFileSync(exePath, path.join(portableDir, "DiskMaster.exe"));
writeFileSync(path.join(portableDir, "portable.marker"), "", "utf8");
writeFileSync(
  path.join(portableDir, "README.txt"),
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

const portableZip = path.join(outputDir, `DiskMaster-portable-${version}.zip`);
rmSync(portableZip, { force: true });
execSync(
  `powershell -NoProfile -Command "Compress-Archive -Path '${portableDir}\\*' -DestinationPath '${portableZip}' -Force"`,
  { stdio: "inherit", cwd: root },
);

const sha256 = (file: string) =>
  createHash("sha256").update(readFileSync(file)).digest("hex");

const artifacts = {
  setup: path.join(bundleDir, setupExe),
  updater: path.join(bundleDir, updaterFile),
  signature: path.join(bundleDir, updaterSig),
  portable: portableZip,
};

const latest = {
  version,
  notes: "См. описание релиза",
  pub_date: new Date().toISOString(),
  platforms: {
    "windows-x86_64": {
      signature: readFileSync(artifacts.signature, "utf8").trim(),
      url: `${baseUrl}/${encodeURIComponent(path.basename(artifacts.updater))}`,
    },
  },
};

const latestPath = path.join(outputDir, "latest.json");
writeFileSync(latestPath, `${JSON.stringify(latest, null, 2)}\n`, "utf8");

const sums = [
  `${sha256(artifacts.setup)}  ${path.basename(artifacts.setup)}`,
  `${sha256(artifacts.updater)}  ${path.basename(artifacts.updater)}`,
  `${sha256(artifacts.portable)}  ${path.basename(artifacts.portable)}`,
].join("\n");
const sumsPath = path.join(outputDir, "SHA256SUMS.txt");
writeFileSync(sumsPath, `${sums}\n`, "utf8");

const notes = notesFile
  ? readFileSync(path.resolve(root, notesFile), "utf8")
  : [
      `## Диск-Мастер ${version}`,
      "",
      "- Установщик: `" + setupExe + "` (выбор папки, без прав администратора)",
      "- Портативная версия: `" + path.basename(portableZip) + "` — распакуйте и запустите",
      "",
      "### Автообновление",
      "",
      "Версия из установщика сама предложит обновление при запуске.",
      "Портативную версию обновляют вручную, скачав новый архив.",
      "",
      "### Первый запуск",
      "",
      "Windows SmartScreen может показать «Неизвестный издатель» — это ожидаемо,",
      "нажмите «Подробнее» → «Выполнить в любом случае».",
      "",
      "SHA-256: см. `SHA256SUMS.txt`.",
      "",
    ].join("\n");

const assets = [
  artifacts.setup,
  artifacts.updater,
  artifacts.signature,
  latestPath,
  sumsPath,
  artifacts.portable,
]
  .map((file) => `'${file}'`)
  .join(" ");

const flags = [draft ? "--draft" : "", prerelease ? "--prerelease" : ""].filter(Boolean).join(" ");
if (!dryRun) {
  run(
    `gh release create ${tag} ${flags} --repo ${repo} --title "Диск-Мастер ${version}" --notes ${JSON.stringify(notes)} ${assets}`,
  );
} else {
  console.log("\n[release] dry run: артефакты собраны, релиз не опубликован");
}

const sizeMb = (file: string) => (statSync(file).size / 1024 / 1024).toFixed(1);
console.log(`\n[release] ${tag} ${dryRun ? "готов" : "опубликован"}`);
console.log(`[release] установщик ${sizeMb(artifacts.setup)} МБ`);
console.log(`[release] portable ${sizeMb(artifacts.portable)} МБ`);
console.log(
  `[release] проверка: https://github.com/${repo}/releases/latest/download/latest.json`,
);
