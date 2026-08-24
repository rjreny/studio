import { app, BrowserWindow, dialog, ipcMain } from "electron";
import fs from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const isDev = !app.isPackaged;
const settingsPath = () => path.join(app.getPath("userData"), "studio.json");

/** @type {BrowserWindow | null} */
let win = null;

async function readSettings() {
  try {
    return JSON.parse(await fs.readFile(settingsPath(), "utf8"));
  } catch {
    return {};
  }
}

async function writeSettings(next) {
  await fs.mkdir(path.dirname(settingsPath()), { recursive: true });
  await fs.writeFile(settingsPath(), JSON.stringify(next, null, 2));
}

function overlayFor(theme) {
  const dark = theme !== "light";
  return {
    color: dark ? "#22262e" : "#f2f4f8",
    symbolColor: dark ? "#eceff4" : "#1c2430",
    height: 36,
  };
}

async function createWindow() {
  const settings = await readSettings();
  const width = Number(settings.width) || 1280;
  const height = Number(settings.height) || 800;

  win = new BrowserWindow({
    width,
    height,
    minWidth: 960,
    minHeight: 640,
    title: "Studio",
    show: false,
    titleBarStyle: "hidden",
    titleBarOverlay: overlayFor(settings.theme === "light" ? "light" : "dark"),
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });

  win.on("ready-to-show", () => win?.show());
  win.on("resized", async () => {
    if (!win) return;
    const [w, h] = win.getSize();
    const cur = await readSettings();
    cur.width = w;
    cur.height = h;
    await writeSettings(cur);
  });

  if (isDev) await win.loadURL("http://localhost:5173");
  else await win.loadFile(path.join(__dirname, "../dist/index.html"));
}

app.whenReady().then(() => {
  ipcMain.handle("app:version", () => app.getVersion());
  ipcMain.handle("settings:get", async (_e, key) => {
    const s = await readSettings();
    return s[key] ?? null;
  });
  ipcMain.handle("settings:set", async (_e, key, value) => {
    const s = await readSettings();
    s[key] = value;
    await writeSettings(s);
    if (key === "theme" && win) {
      win.setTitleBarOverlay(overlayFor(value === "light" ? "light" : "dark"));
    }
  });
  ipcMain.handle("file:open", async () => {
    if (!win) return null;
    const picked = await dialog.showOpenDialog(win, {
      properties: ["openFile"],
    });
    if (picked.canceled || !picked.filePaths[0]) return null;
    const filePath = picked.filePaths[0];
    const stat = await fs.stat(filePath);
    return { name: path.basename(filePath), bytes: stat.size };
  });
  ipcMain.handle("window:overlay", async (_e, theme) => {
    win?.setTitleBarOverlay(overlayFor(theme));
  });

  return createWindow();
});

app.on("window-all-closed", () => app.quit());
