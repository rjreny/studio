import { useState } from "react";
import { importExportZip, formatEnrich, formatImport, syncSelf, tmdbEnrich } from "../../platform/filmLibrary";
import { pickExportZipPath } from "../../platform/files";
import { log } from "../../platform/log";

export function ConnectView({
  username,
  onUsername,
  onStatus,
  onConnected,
}: {
  username: string;
  onUsername: (name: string) => void;
  onStatus: (s: string) => void;
  onConnected: () => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function connectRss() {
    const name = username.trim().replace(/^@/, "");
    if (!/^[a-zA-Z0-9_]+$/.test(name)) {
      setError("Use your Letterboxd username (letters, numbers, underscore).");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await syncSelf(name);
      onUsername(name);
      onStatus(
        `Connected @${name} · ${result.entriesAdded} new diary entries (${result.entriesSeen} in feed)`,
      );
      await onConnected();
    } catch (err) {
      log("error", "rss connect failed", err);
      setError("Could not reach that public diary. Check the username.");
    } finally {
      setBusy(false);
    }
  }

  async function importExport() {
    setBusy(true);
    setError(null);
    try {
      const path = await pickExportZipPath();
      if (!path) return;
      const result = await importExportZip(path);
      onStatus(formatImport(result));
      await onConnected();
      try {
        const report = await tmdbEnrich();
        onStatus(`${formatImport(result)} · ${formatEnrich(report)}`);
        await onConnected();
      } catch (err) {
        log("warn", "poster enrich after import failed", err);
      }
    } catch (err) {
      log("error", "export import failed", err);
      setError("Could not read that export. Use an official Letterboxd ZIP.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="connect solid-card">
      <p className="eyebrow">Your log</p>
      <h1>Bring your Letterboxd life in.</h1>
      <p className="lede">
        Studio preserves every diary event — rewatches, rating changes, and overlapping exports — not
        just a collapsed film count.
      </p>
      <label>
        Username
        <input
          value={username}
          onChange={(e) => onUsername(e.target.value)}
          placeholder="letterboxd username"
          autoCapitalize="off"
          spellCheck={false}
        />
      </label>
      <div className="connect-actions">
        <button type="button" className="primary" disabled={busy} onClick={() => void connectRss()}>
          {busy ? "Connecting…" : "Connect public diary"}
        </button>
        <button type="button" className="ghost" disabled={busy} onClick={() => void importExport()}>
          Import export ZIP
        </button>
      </div>
      {error ? <p className="form-error">{error}</p> : null}
      <p className="hint">
        Full history: Letterboxd Settings → Import & Export → download ZIP. RSS only covers the latest
        ~50 diary entries and never counts as full history.
      </p>
    </div>
  );
}
