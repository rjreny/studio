import { useEffect, useState } from "react";
import { type Accent, type Theme } from "../../core/types";
import { pickExportZipPath } from "../../platform/files";
import {
  formatCoverage,
  importExportZip,
  importGetDiagnostics,
  tmdbClearKey,
  tmdbEnrich,
  tmdbHasKey,
  tmdbSetKey,
} from "../../platform/filmLibrary";
import type { LibraryCoverage } from "../../platform/types/film";
import { log } from "../../platform/log";
import {
  checkAppUpdate,
  downloadAndInstallUpdate,
  type UpdateProgress,
} from "../../platform/updater";
import { UpdateOverlay } from "../../app/shell/UpdateOverlay";

const idleProgress: UpdateProgress = {
  phase: "idle",
  label: "Ready",
  percent: null,
  version: null,
  error: null,
};

export function SettingsView({
  theme,
  accent,
  version,
  username,
  coverage,
  onTheme,
  onAccent,
  onUsername,
  onStatus,
  onRefresh,
}: {
  theme: Theme;
  accent: Accent;
  version: string;
  username: string;
  coverage: LibraryCoverage | null;
  onTheme: (t: Theme) => void;
  onAccent: (a: Accent) => void;
  onUsername: (name: string) => void;
  onStatus: (text: string) => void;
  onRefresh: () => Promise<void>;
}) {
  const [updateNote, setUpdateNote] = useState("Not checked");
  const [signingConfigured, setSigningConfigured] = useState(true);
  const [pendingVersion, setPendingVersion] = useState<string | null>(null);
  const [updateOpen, setUpdateOpen] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<UpdateProgress>(idleProgress);
  const [hasKey, setHasKey] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const [diagnostics, setDiagnostics] = useState<string[]>([]);

  useEffect(() => {
    void (async () => {
      setHasKey(await tmdbHasKey());
      try {
        const d = await importGetDiagnostics();
        setDiagnostics(d.warnings);
      } catch {
        /* dev without tauri */
      }
    })();
  }, []);

  async function checkUpdates() {
    const result = await checkAppUpdate();
    setSigningConfigured(result.signingConfigured);
    if (result.error) {
      setUpdateNote(result.error);
      setPendingVersion(null);
      return;
    }
    if (result.available) {
      setPendingVersion(result.version);
      setUpdateNote(`Update available: ${result.version}`);
      return;
    }
    setPendingVersion(null);
    setUpdateNote(result.message ?? "You're up to date");
  }

  async function installUpdate() {
    setUpdateOpen(true);
    setUpdateProgress({
      phase: "checking",
      label: "Preparing update…",
      percent: null,
      version: pendingVersion,
      error: null,
    });
    await downloadAndInstallUpdate(setUpdateProgress);
  }

  async function importExport() {
    try {
      const path = await pickExportZipPath();
      if (!path) return;
      const result = await importExportZip(path);
      onStatus(
        `Imported · ${result.viewings} viewings · ${result.coverage.uniqueMovies} unique films`,
      );
      await onRefresh();
    } catch (err) {
      log("error", "settings import failed", err);
      onStatus("Import failed — library unchanged");
    }
  }

  async function saveKey() {
    if (!keyInput.trim()) return;
    await tmdbSetKey(keyInput.trim());
    setKeyInput("");
    setHasKey(true);
    onStatus("TMDB key stored in Windows Credential Manager");
  }

  return (
    <div className="settings-page">
      <h1>Settings</h1>
      <section className="settings-group solid-card">
        <h2>Library</h2>
        {coverage ? <p className="coverage-line">{formatCoverage(coverage)}</p> : null}
        {diagnostics.map((w) => (
          <p key={w} className="hint">
            {w}
          </p>
        ))}
        <div className="setting-row">
          <label>Letterboxd user</label>
          <input
            value={username}
            onChange={(e) => onUsername(e.target.value)}
            placeholder="username"
          />
        </div>
        <div className="setting-row">
          <label>Letterboxd export</label>
          <button type="button" className="primary" onClick={() => void importExport()}>
            Import ZIP
          </button>
        </div>
      </section>
      <section className="settings-group solid-card">
        <h2>Catalog</h2>
        <p className="hint">
          TMDB key is stored in Windows Credential Manager, never in studio.json.
        </p>
        <div className="setting-row">
          <label>TMDB API key</label>
          <input
            value={keyInput}
            onChange={(e) => setKeyInput(e.target.value)}
            placeholder={hasKey ? "Key saved — paste to replace" : "optional enrichment key"}
            spellCheck={false}
          />
        </div>
        <div className="row-actions">
          <button type="button" className="ghost" onClick={() => void saveKey()}>
            Save key
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => void tmdbClearKey().then(() => setHasKey(false))}
          >
            Clear key
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() =>
              void tmdbEnrich().then((count) => {
                onStatus(
                  count > 0
                    ? `Enriched ${count} poster${count === 1 ? "" : "s"}`
                    : "No new posters to fetch — add a TMDB key or re-import",
                );
                return onRefresh();
              })
            }
          >
            Enrich unmatched
          </button>
        </div>
      </section>
      <section className="settings-group solid-card">
        <h2>Appearance</h2>
        <div className="setting-row">
          <label>Theme</label>
          <div className="seg">
            {(["system", "dark", "light"] as const).map((t) => (
              <button key={t} type="button" className={theme === t ? "is-on" : ""} onClick={() => onTheme(t)}>
                {t[0].toUpperCase() + t.slice(1)}
              </button>
            ))}
          </div>
        </div>
        <div className="setting-row">
          <label>Accent</label>
          <div className="seg">
            {(["app", "system"] as const).map((a) => (
              <button key={a} type="button" className={accent === a ? "is-on" : ""} onClick={() => onAccent(a)}>
                {a === "app" ? "App" : "System"}
              </button>
            ))}
          </div>
        </div>
      </section>
      <section className="settings-group solid-card">
        <h2>About</h2>
        <div className="setting-row">
          <label>Version</label>
          <span>{version}</span>
        </div>
        <div className="setting-row">
          <label>Updates</label>
          <div className="row-actions">
            <button type="button" className="ghost" onClick={() => void checkUpdates()}>
              Check for updates
            </button>
            {pendingVersion ? (
              <button type="button" className="primary" onClick={() => void installUpdate()}>
                Update to {pendingVersion}
              </button>
            ) : null}
          </div>
        </div>
        <p className="file-result">{updateNote}</p>
        {signingConfigured ? (
          <p className="hint">
            Updates download only when you click Update. Studio shows progress here, then restarts
            once the installer finishes.
          </p>
        ) : (
          <p className="hint">
            This is normal for a local dev build. To enable signed auto-updates: run{" "}
            <code>npm run signer:generate</code> in <code>studio/</code>, copy the public key into{" "}
            <code>src-tauri/tauri.conf.json</code> (<code>plugins.updater.pubkey</code>), set your
            GitHub releases URL in <code>endpoints</code>, and tag a release (<code>v*</code>).
          </p>
        )}
      </section>
      <UpdateOverlay
        open={updateOpen}
        title="Installing update"
        progress={updateProgress}
        onClose={() => {
          setUpdateOpen(false);
          setUpdateProgress(idleProgress);
        }}
      />
    </div>
  );
}
