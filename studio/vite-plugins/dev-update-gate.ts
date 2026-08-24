import fs from "node:fs";
import path from "node:path";
import type { Plugin, ViteDevServer } from "vite";

type PendingKind = "frontend" | "rust";

export function devUpdateGate(): Plugin {
  let server: ViteDevServer | undefined;
  let frontendPending = false;
  let rustPending = false;
  const stampPath = path.resolve("src-tauri/target/.studio-dev-stamp");

  function snapshot() {
    return {
      pending: frontendPending || rustPending,
      frontend: frontendPending,
      rust: rustPending,
    };
  }

  function notify() {
    if (server) {
      server.ws.send({ type: "custom", event: "studio:update-available", data: snapshot() });
    }
  }

  function mark(kind: PendingKind) {
    if (kind === "frontend") frontendPending = true;
    if (kind === "rust") rustPending = true;
    notify();
  }

  function clear() {
    frontendPending = false;
    rustPending = false;
    notify();
  }

  return {
    name: "studio-dev-update-gate",
    apply: "serve",
    configureServer(devServer) {
      server = devServer;

      devServer.middlewares.use("/__studio/dev-update", (_req, res) => {
        res.setHeader("Content-Type", "application/json");
        res.end(JSON.stringify(snapshot()));
      });

      devServer.middlewares.use("/__studio/dev-update/clear", (_req, res) => {
        clear();
        res.statusCode = 204;
        res.end();
      });

      if (fs.existsSync(stampPath)) {
        devServer.watcher.add(stampPath);
      } else {
        fs.mkdirSync(path.dirname(stampPath), { recursive: true });
        fs.writeFileSync(stampPath, "0");
        devServer.watcher.add(stampPath);
      }

      devServer.watcher.on("change", (file) => {
        if (path.resolve(file) === stampPath) {
          mark("rust");
        }
      });
    },
    handleHotUpdate() {
      mark("frontend");
      return [];
    },
  };
}
