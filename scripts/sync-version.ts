/**
 * Single source of truth for the version: `package.json`.
 * Propagates it into tauri.conf.json and Cargo.toml before every build.
 */
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const root = process.cwd();
const pkg = JSON.parse(readFileSync(path.join(root, "package.json"), "utf8")) as {
  version: string;
};
const version = pkg.version;

const confPath = path.join(root, "src-tauri", "tauri.conf.json");
const conf = JSON.parse(readFileSync(confPath, "utf8")) as Record<string, unknown>;
conf.version = version;
writeFileSync(confPath, `${JSON.stringify(conf, null, 2)}\n`, "utf8");

const cargoPath = path.join(root, "src-tauri", "Cargo.toml");
const cargo = readFileSync(cargoPath, "utf8").replace(
  /^version = ".*"$/m,
  `version = "${version}"`,
);
writeFileSync(cargoPath, cargo, "utf8");

console.log(`version ${version} synced to tauri.conf.json and Cargo.toml`);
