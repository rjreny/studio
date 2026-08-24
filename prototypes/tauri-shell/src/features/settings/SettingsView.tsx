import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import type { Accent, Theme } from "../library/data";
import { formatBytes } from "../library/data";

export function SettingsView({
  theme,
  accent,
  onTheme,
  onAccent,
  onStatus,
}: {
  theme: Theme;
  accent: Accent;
  onTheme: (t: Theme) => void;
  onAccent: (a: Accent) => void;
  onStatus: (text: string) => void;
}) {
  const [version, setVersion] = useState("…");
  const [file, setFile] = useState<string>("No file opened");
  const [updateNote, setUpdateNote] = useState("Not checked");

  useEffect(() => {
    void getVersion().then(setVersion).catch(() => setVersion("dev"));
  }, []);

  async function pickFile() {
    const picked = await open({ multiple: false, directory: false });
    if (!picked || Array.isArray(picked)) return;
    const info = await invoke<{ name: string; bytes: number }>("file_info", {
      path: picked,
    });
    const line = `${info.name} (${formatBytes(info.bytes)})`;
    setFile(line);
    onStatus(`Opened ${line}`);
  }

  async function smokeUpdate() {
    try {
      const res = await fetch(
        "https://github.com/local/studio/releases/latest/download/latest.json",
      );
      setUpdateNote(`HTTP ${res.status} — bakeoff smoke, no updater pipeline`);
    } catch (err) {
      setUpdateNote(`Fetch failed (${String(err)}) — expected in bakeoff`);
    }
  }

  return (
    <div className="settings">
      <h1>Settings</h1>
      <div className="setting-row">
        <label>Theme</label>
        <div className="seg">
          {(["system", "dark", "light"] as const).map((t) => (
            <button
              key={t}
              type="button"
              className={theme === t ? "is-on" : ""}
              onClick={() => onTheme(t)}
            >
              {t[0].toUpperCase() + t.slice(1)}
            </button>
          ))}
        </div>
      </div>
      <div className="setting-row">
        <label>Accent</label>
        <div className="seg">
          {(["app", "system"] as const).map((a) => (
            <button
              key={a}
              type="button"
              className={accent === a ? "is-on" : ""}
              onClick={() => onAccent(a)}
            >
              {a === "app" ? "App" : "System"}
            </button>
          ))}
        </div>
      </div>
      <div className="setting-row">
        <label>Version</label>
        <span>{version}</span>
      </div>
      <div className="setting-row">
        <label>Native file</label>
        <button type="button" className="primary" onClick={() => void pickFile()}>
          Open file…
        </button>
      </div>
      <p className="file-result">{file}</p>
      <div className="setting-row">
        <label>Updater</label>
        <button type="button" className="primary" onClick={() => void smokeUpdate()}>
          Check for updates
        </button>
      </div>
      <p className="file-result">{updateNote}</p>
    </div>
  );
}
