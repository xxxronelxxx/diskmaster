/**
 * Builds the NSIS installer.
 *
 * If the updater private key exists (env or ~/.tauri/diskmaster.key), updater
 * artifacts are signed as usual. Without a key the installer is still built,
 * just without the updater package, so local builds do not fail.
 */
import { execFileSync, spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";

const root = process.cwd();
const key =
  process.env.TAURI_SIGNING_PRIVATE_KEY ||
  path.join(os.homedir(), ".tauri", "diskmaster.key");

const env = { ...process.env };
const args = ["tauri", "build"];

execFileSync("pnpm", ["version:sync"], { stdio: "inherit", cwd: root });

if (existsSync(key)) {
  env.TAURI_SIGNING_PRIVATE_KEY = key;
  console.log(`[installer] ключ обновлений: ${key}`);
} else {
  console.warn(
    "[installer] ключ обновлений не найден — установщик будет собран без автообновления",
  );
  args.push("--config", JSON.stringify({ bundle: { createUpdaterArtifacts: false } }));
}

const result = spawnSync("pnpm", args, {
  stdio: "inherit",
  cwd: root,
  env,
  shell: true,
});

process.exit(result.status ?? 1);
