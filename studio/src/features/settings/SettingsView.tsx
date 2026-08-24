import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ask } from "@tauri-apps/plugin-dialog";
import { type Accent, type Theme } from "../../core/types";
import { pickExportZipPath } from "../../platform/files";
import {
  formatCoverage,
  formatEnrich,
  formatImport,
  importExportZip,
  importGetDiagnostics,
  tmdbClearKey,
  tmdbEnrich,
  tmdbKeyStatus,
  tmdbSetKey,
} from "../../platform/filmLibrary";
import type { EnrichReport, ImportResult, InstallInfo, JobProgress, LibraryCoverage, TmdbKeyStatus } from "../../platform/types/film";
import {
  getInstallInfo,
  installKindLabel,
  launchUninstaller,
  openDataFolder,
  openLogFile,
  resetStudioData,
} from "../../platform/install";
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
  onRefresh: _onRefresh,
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
  const [keyStatus, setKeyStatus] = useState<TmdbKeyStatus | null>(null);
  const [replacing, setReplacing] = useState(false);
  const [busy, setBusy] = useState(false);
  const [keyInput, setKeyInput] = useState("");
  const [diagnostics, setDiagnostics] = useState<string[]>([]);
  const [installInfo, setInstallInfo] = useState<InstallInfo | null>(null);
  const [lastImport, setLastImport] = useState<ImportResult | null>(null);
  const [lastEnrich, setLastEnrich] = useState<EnrichReport | null>(null);

  useEffect(() => {
    void (async () => {
      try {
        const status = await tmdbKeyStatus();
        setKeyStatus(status);
      } catch {
        /* dev without tauri */
      }
      try {
        const d = await importGetDiagnostics();
        setDiagnostics(d.warnings);
      } catch {
        /* dev without tauri */
      }
      try {
        setInstallInfo(await getInstallInfo());
      } catch (err) {
        log("warn", "install info unavailable", err);
      }
    })();
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<JobProgress>("studio-job", (event) => {
      const next = event.payload;
      if (next.import) setLastImport(next.import);
      if (next.enrich) {
        setLastEnrich(next.enrich);
        onStatus(formatEnrich(next.enrich));
      }
      if (next.done && next.import) {
        onStatus(formatImport(next.import));
      }
    }).then((fn) => {
      unlisten = fn;
    });
    return () => unlisten?.();
  }, [onStatus]);

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
      setBusy(true);
      await importExportZip(path);
      onStatus("Importing ZIP in the background — you can keep using Studio");
      setBusy(false);
    } catch (err) {
      log("error", "settings import failed", err);
      onStatus("Import failed — library unchanged");
    } finally {
      setBusy(false);
    }
  }

  async function saveKey() {
    if (!keyInput.trim()) return;
    try {
      setBusy(true);
      const status = await tmdbSetKey(keyInput.trim());
      const confirmed = status.stored ? await tmdbKeyStatus().catch(() => status) : status;
      setKeyStatus(confirmed);
      setKeyInput("");
      if (confirmed.valid !== true || !confirmed.stored) {
        onStatus(
          confirmed.lastError ?? "TMDB accepted this key, but Windows did not keep it — it was not saved",
        );
        return;
      }
      setReplacing(false);
      onStatus("TMDB accepted this key — matching posters in the background");
      await tmdbEnrich();
    } catch (err) {
      log("error", "tmdb key save failed", err);
      onStatus("Could not store TMDB key or fetch posters");
    } finally {
      setBusy(false);
    }
  }

  async function runEnrich() {
    try {
      setBusy(true);
      await tmdbEnrich();
      onStatus("Matching TMDB in the background — you can keep browsing");
    } catch (err) {
      log("error", "enrich failed", err);
      onStatus("Poster fetch failed — open studio.log for details");
    } finally {
      setBusy(false);
    }
  }

  const keyConnected = Boolean(keyStatus?.stored && keyStatus.valid === true && !replacing);

  async function confirmResetData() {
    const ok = await ask(
      "This removes your library, friends, posters, and saved preferences from this device. It cannot be undone.",
      { title: "Reset Studio?", kind: "warning", okLabel: "Reset everything", cancelLabel: "Cancel" },
    );
    if (!ok) return;
    try {
      await resetStudioData();
    } catch (err) {
      log("error", "reset failed", err);
      onStatus("Could not reset Studio data");
    }
  }

  async function runUninstaller() {
    try {
      await launchUninstaller();
      onStatus("Uninstaller opened — your library data stays until you reset it or remove the data folder");
    } catch (err) {
      log("warn", "uninstaller unavailable", err);
      onStatus("No uninstaller for this build — remove dev shortcuts manually or use Installed apps in Windows Settings");
    }
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
          <button type="button" className="primary" disabled={busy} onClick={() => void importExport()}>
            {busy ? "Working…" : "Import ZIP"}
          </button>
        </div>
        {lastImport ? (
          <div className="result-card">
            <dl>
              <div>
                <dt>Films</dt>
                <dd>{lastImport.movies}</dd>
              </div>
              <div>
                <dt>Viewings</dt>
                <dd>{lastImport.viewings}</dd>
              </div>
              <div>
                <dt>Ratings</dt>
                <dd>{lastImport.ratings}</dd>
              </div>
              <div>
                <dt>Already present</dt>
                <dd>{lastImport.skipped}</dd>
              </div>
            </dl>
            <p className="hint">{formatImport(lastImport)}</p>
            {lastImport.warnings.map((w) => (
              <p key={w} className="hint">
                {w}
              </p>
            ))}
          </div>
        ) : null}
      </section>
      <section className="settings-group solid-card">
        <h2>Catalog</h2>
        <p className="hint">
          TMDB key is stored in Windows Credential Manager, never in studio.json. Studio asks TMDB
          whether the key works before saving it.
        </p>
        {keyConnected ? (
          <p className="key-status is-ok">
            TMDB accepted this key{keyStatus?.kind ? ` · ${keyStatus.kind}` : ""}. The insert field stays
            hidden until you replace it.
          </p>
        ) : keyStatus?.stored && keyStatus.valid === false ? (
          <p className="key-status is-bad">{keyStatus.lastError ?? "TMDB rejected this key."}</p>
        ) : keyStatus?.stored && keyStatus.valid == null ? (
          <p className="key-status is-warn">
            A key is stored, but Studio could not reach TMDB to verify it.
            {keyStatus.lastError ? ` ${keyStatus.lastError}` : ""}
          </p>
        ) : (
          <p className="key-status is-warn">
            No TMDB key yet. ZIP import still works; posters need this key or Letterboxd oEmbed.
          </p>
        )}
        {!keyConnected ? (
          <div className="setting-row">
            <label>TMDB API key</label>
            <input
              value={keyInput}
              onChange={(e) => setKeyInput(e.target.value)}
              placeholder={
                keyStatus?.stored
                  ? "Paste a replacement key"
                  : "API Key (v3) from themoviedb.org/settings/api"
              }
              spellCheck={false}
              disabled={busy}
            />
          </div>
        ) : null}
        <div className="row-actions">
          {!keyConnected ? (
            <button type="button" className="ghost" disabled={busy || !keyInput.trim()} onClick={() => void saveKey()}>
              Save key
            </button>
          ) : (
            <button type="button" className="ghost" disabled={busy} onClick={() => setReplacing(true)}>
              Replace key
            </button>
          )}
          <button
            type="button"
            className="ghost"
            disabled={busy || !keyStatus?.stored}
            onClick={() =>
              void tmdbClearKey().then((status) => {
                setKeyStatus(status);
                setReplacing(false);
                onStatus("TMDB key removed");
              })
            }
          >
            Remove key
          </button>
          <button type="button" className="ghost" disabled={busy} onClick={() => void runEnrich()}>
            Enrich unmatched
          </button>
        </div>
        {lastEnrich ? (
          <div className="result-card">
            <dl>
              <div>
                <dt>Matched</dt>
                <dd>
                  {lastEnrich.matched}/{lastEnrich.attempted}
                </dd>
              </div>
              <div>
                <dt>Posters</dt>
                <dd>{lastEnrich.posters}</dd>
              </div>
              <div>
                <dt>Still unmatched</dt>
                <dd>{lastEnrich.remainingUnmatched}</dd>
              </div>
              <div>
                <dt>Missing poster</dt>
                <dd>{lastEnrich.remainingWithoutPoster}</dd>
              </div>
            </dl>
            <p className="hint">{formatEnrich(lastEnrich)}</p>
          </div>
        ) : null}
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
        <h2>Installation</h2>
        {installInfo ? (
          <>
            <div className="setting-row">
              <label>Build</label>
              <span>{installKindLabel(installInfo.installKind)}</span>
            </div>
            <div className="setting-row">
              <label>Data folder</label>
              <span className="mono-path">{installInfo.appDataDir}</span>
            </div>
            <p className="hint">
              Library, friends, posters, and your username live in one SQLite database here. Reinstalling
              or updating Studio keeps this folder unless you reset it.
            </p>
            {installInfo.installKind === "dev" ? (
              <p className="hint">
                You are running a dev build. Use the installed release from GitHub for normal install,
                update, and uninstall behavior.
              </p>
            ) : null}
          </>
        ) : null}
        <div className="row-actions">
          <button type="button" className="ghost" onClick={() => void openDataFolder().catch(() => onStatus("Could not open data folder"))}>
            Open data folder
          </button>
          <button type="button" className="ghost" onClick={() => void openLogFile().catch(() => onStatus("Could not open studio.log"))}>
            Open log
          </button>
          {installInfo?.uninstallerPath ? (
            <button type="button" className="ghost" onClick={() => void runUninstaller()}>
              Uninstall Studio
            </button>
          ) : null}
          <button type="button" className="ghost" onClick={() => void confirmResetData()}>
            Reset all data
          </button>
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
        {import.meta.env.DEV ? (
          <p className="hint">
            Dev mode checks the release feed but cannot install updates — use the NSIS installer from{" "}
            <a href="https://github.com/rjreny/studio/releases" target="_blank" rel="noreferrer">
              GitHub Releases
            </a>
            , then Update works in the installed app.
          </p>
        ) : (
          <p className="hint">
            Updates download only when you click Update. Installing a new release replaces the existing
            Studio install and keeps your library data.
          </p>
        )}
        {!signingConfigured && !import.meta.env.DEV ? (
          <p className="hint">
            Signing is not configured in this build. Reinstall from a signed GitHub release.
          </p>
        ) : null}
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
