import type { UpdateProgress } from "../../platform/updater";

const phaseLabel: Record<UpdateProgress["phase"], string> = {
  idle: "Ready",
  checking: "Checking",
  available: "Update ready",
  downloading: "Downloading",
  installing: "Installing",
  relaunching: "Restarting",
  error: "Update failed",
};

export function UpdateOverlay({
  open,
  title,
  progress,
  onClose,
}: {
  open: boolean;
  title: string;
  progress: UpdateProgress;
  onClose?: () => void;
}) {
  if (!open) return null;

  const showBar = progress.percent !== null && progress.phase !== "error";
  const canClose = progress.phase === "error" || progress.phase === "idle";

  return (
    <div className="update-overlay" role="dialog" aria-modal="true" aria-labelledby="update-overlay-title">
      <div className="update-card solid-card">
        <div className="update-head">
          <h2 id="update-overlay-title">{title}</h2>
          <span className="update-phase">{phaseLabel[progress.phase]}</span>
        </div>
        <p className="update-label">{progress.label}</p>
        {progress.version ? <p className="update-version">Version {progress.version}</p> : null}
        {showBar ? (
          <div className="update-progress" aria-hidden>
            <div className="update-progress-fill" style={{ width: `${progress.percent ?? 0}%` }} />
          </div>
        ) : null}
        {progress.error ? <p className="update-error">{progress.error}</p> : null}
        {canClose && onClose ? (
          <div className="update-actions">
            <button type="button" className="ghost" onClick={onClose}>Close</button>
          </div>
        ) : null}
      </div>
    </div>
  );
}
