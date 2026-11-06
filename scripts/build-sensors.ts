/**
 * Builds the managed sensor host and stages it into `src-tauri/resources/sensors`.
 *
 * The .NET Framework 4.8 target avoids shipping a runtime: Windows 11 already
 * has it. PawnIO's official signed installer is downloaded once and verified by
 * SHA-256, so the repository stays source-only.
 */
import { execSync } from "node:child_process";
import { createHash } from "node:crypto";
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const target = path.join(root, "src-tauri", "resources", "sensors");
const project = path.join(root, "sensors", "DiskMasterSensors.csproj");
const pawnIo = {
  version: "2.2.0",
  url: "https://github.com/namazso/PawnIO.Setup/releases/download/2.2.0/PawnIO_setup.exe",
  sha256: "1f519a22e47187f70a1379a48ca604981c4fcf694f4e65b734aaa74a9fba3032",
};

const log = (text: string) => console.log(`[sensors] ${text}`);

function sha256(file: string): string {
  return createHash("sha256").update(readFileSync(file)).digest("hex");
}

async function ensurePawnIo(): Promise<void> {
  const destination = path.join(target, "PawnIO_setup.exe");
  if (existsSync(destination) && sha256(destination) === pawnIo.sha256) {
    log(`PawnIO ${pawnIo.version} уже загружен`);
    return;
  }

  log(`скачивание PawnIO ${pawnIo.version}`);
  const response = await fetch(pawnIo.url);
  if (!response.ok) {
    throw new Error(`Не удалось скачать PawnIO: HTTP ${response.status}`);
  }
  const buffer = Buffer.from(await response.arrayBuffer());
  const hash = createHash("sha256").update(buffer).digest("hex");
  if (hash !== pawnIo.sha256) {
    throw new Error(`SHA-256 PawnIO не совпал: ${hash}`);
  }
  writeFileSync(destination, buffer);
}

async function main(): Promise<void> {
  try {
    execSync("dotnet --version", { stdio: "ignore" });
  } catch {
    throw new Error("Не найден dotnet SDK. Установите .NET SDK 8+ для сборки модуля датчиков.");
  }

  log("сборка DiskMasterSensors (net472, x64)");
  execSync(
    `dotnet publish "${project}" -c Release -p:PlatformTarget=x64 -o "${target}"`,
    { stdio: "inherit", cwd: root },
  );

  mkdirSync(path.join(target, "licenses"), { recursive: true });
  for (const file of ["MPL-2.0.txt", "GPL-2.0.txt"]) {
    copyFileSync(
      path.join(root, "sensors", "licenses", file),
      path.join(target, "licenses", file),
    );
  }
  copyFileSync(
    path.join(root, "sensors", "THIRD-PARTY-NOTICES.md"),
    path.join(target, "THIRD-PARTY-NOTICES.md"),
  );

  await ensurePawnIo();
  log("готово: src-tauri/resources/sensors");
}

main().catch((error) => {
  console.error(`[sensors] ${error instanceof Error ? error.message : String(error)}`);
  process.exit(1);
});
