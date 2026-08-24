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

NSIS installs per-user. Push a `v*` tag to cut a signed NSIS installer and `latest.json` for auto-update.

Repo: https://github.com/rjreny/studio

**Auto-update requires a public repo** (or a public mirror of `latest.json`). Private repos return 404 to the app.

```powershell
gh repo edit rjreny/studio --visibility public --accept-visibility-change-consequences
```

Set the CI signing secret (PowerShell — `gh` has no `--body-file`):

```powershell
Get-Content -Raw studio\.tauri\studio-updater.key | gh secret set TAURI_SIGNING_PRIVATE_KEY --repo rjreny/studio
```

Features must not import `@tauri-apps/*`. Use `src/platform`. `npm run lint` enforces that.
