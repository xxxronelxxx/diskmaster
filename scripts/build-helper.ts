/**
 * Builds the elevated helper binary and copies it into `src-tauri/resources`,
 * where Tauri bundles it as an application resource.
 */
import { execSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const profile = process.argv.includes("--debug") ? "debug" : "release";

execSync(`cargo build --${profile === "debug" ? "debug" : "release"} --bin DiskMasterHelper`, {
  stdio: "inherit",
  cwd: path.join(root, "src-tauri"),
});

const source = path.join(root, "src-tauri", "target", profile, "DiskMasterHelper.exe");
if (!existsSync(source)) {
  throw new Error(`helper binary not found: ${source}`);
}

const target = path.join(root, "src-tauri", "resources");
mkdirSync(target, { recursive: true });
copyFileSync(source, path.join(target, "DiskMasterHelper.exe"));
console.log(`[helper] ${profile} build copied to src-tauri/resources/DiskMasterHelper.exe`);
