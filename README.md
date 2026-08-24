# Studio

Local-first Windows desktop app. Host: **Tauri 2** (see [docs/runtime-decision.md](docs/runtime-decision.md)).

## Layout

- `prototypes/tauri-shell` — bakeoff prototype (do not treat as product)
- `prototypes/electron-shell` — bakeoff prototype
- `studio` — real app: shell + Notes, `src/platform` host boundary
- `docs/runtime-decision.md` — why Tauri for v1

## Run Studio

```bash
cd studio
npm install
npm run tauri dev
npm run tauri build
```

NSIS installs per-user. After any change that should reach the installed app, bump the version, push `master`, and push a `v*` tag so release CI can cut a signed NSIS installer and `latest.json` for auto-update. A push without a `v*` tag does not update the installed app.

Repo: https://github.com/rjreny/studio

**CI speed:** pushes to `master` warm the Rust cache via `.github/workflows/ci.yml`. Tag releases reuse that cache (often ~5–10 min vs ~20+ min cold). Any `Cargo.lock` change forces a full dependency rebuild once.

**Auto-update requires a public repo** (or a public mirror of `latest.json`). Private repos return 404 to the app.

```powershell
gh repo edit rjreny/studio --visibility public --accept-visibility-change-consequences
```

Set the CI signing secret (PowerShell — `gh` has no `--body-file`):

```powershell
Get-Content -Raw studio\.tauri\studio-updater.key | gh secret set TAURI_SIGNING_PRIVATE_KEY --repo rjreny/studio
```

Features must not import `@tauri-apps/*`. Use `src/platform`. `npm run lint` enforces that.
