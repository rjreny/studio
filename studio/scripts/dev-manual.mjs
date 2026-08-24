import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const studioRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tauriRoot = path.join(studioRoot, "src-tauri");
const stampPath = path.join(tauriRoot, "target", ".studio-dev-stamp");

function run(cmd: string, args: string[], cwd: string) {
  const child = spawn(cmd, args, {
    cwd,
    shell: process.platform === "win32",
    stdio: "inherit",
  });
  child.on("error", (err) => {
    console.error(`[dev-manual] failed to start ${cmd}:`, err.message);
    process.exit(1);
  });
  return child;
}

const stampCmd =
  process.platform === "win32"
    ? `cargo build --quiet && node -e "require('fs').writeFileSync('${stampPath.replace(/\\/g, "\\\\")}', String(Date.now()))"`
    : `cargo build --quiet && node -e "require('fs').writeFileSync('${stampPath}', String(Date.now()))"`;

console.log("[dev-manual] Rust builds on change; app restarts only when you click Update.");
console.log("[dev-manual] Requires cargo-watch: cargo install cargo-watch");

const rustWatch = run("cargo", ["watch", "-q", "-w", "src", "-s", stampCmd], tauriRoot);
const tauriDev = run("npm", ["run", "tauri", "dev", "--", "--no-watch"], studioRoot);

function shutdown() {
  rustWatch.kill();
  tauriDev.kill();
  process.exit(0);
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

tauriDev.on("exit", (code) => {
  rustWatch.kill();
  process.exit(code ?? 0);
});
