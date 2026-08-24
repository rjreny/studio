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

NSIS is per-user. Tag `v*` on GitHub to cut an unsigned installer. For signed auto-updates: `npm run signer:generate`, paste the public key into `src-tauri/tauri.conf.json`, set your GitHub `endpoints` URL, and add `TAURI_SIGNING_PRIVATE_KEY` to CI. Authenticode is the separate Windows trust step.
