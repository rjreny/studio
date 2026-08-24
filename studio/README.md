# Studio

Real app after the bakeoff. Tauri 2 host. Features talk to `src/platform` only.

```bash
npm install
npm run lint
npm run dev:manual   # recommended: manual Update button, no auto-reload
npm run tauri dev    # legacy: Rust changes still auto-restart
npm run tauri build
```

While developing, use `npm run dev:manual` so code changes queue behind an **Update** button in the status bar instead of hot-reloading the UI. Requires `cargo install cargo-watch`. Production updates in Settings also wait for you to click **Update**, show download/install progress, then restart once.

NSIS is per-user. Push a `v*` tag to cut a signed NSIS installer and `latest.json` for auto-update.

Repo: https://github.com/rjreny/studio
