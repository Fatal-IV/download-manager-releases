# Download Manager (İndirme Yöneticisi)

A free, modern download manager for Windows, in the spirit of IDM. It splits files into segments and downloads them over several connections, supports pause/resume, and integrates with the browser through a small extension.

Built with [Tauri 2](https://tauri.app) (Rust backend) and React + TypeScript + Tailwind (UI). The interface is currently in Turkish.

Installers are published at [Fatal-IV/download-manager-releases](https://github.com/Fatal-IV/download-manager-releases/releases). This repository contains the complete source code of those builds.

## Features

- Segmented multi-connection downloads, pause/resume, and a download queue with a concurrency limit
- Mirror addresses and CDN IP spreading; adaptive back-off when a server rate-limits connections (HTTP 429)
- Speed limiter, schedule window (auto pause/resume), and pause/resume on network loss and recovery
- Sorting into category folders, search/sort, and a live speed graph
- Optional SHA-256 verification and an optional Windows Defender scan of finished files
- Authenticated downloads (HTTP basic auth, cookie header)
- Video/audio downloads through [yt-dlp](https://github.com/yt-dlp/yt-dlp)
- System tray, start with Windows, desktop notifications
- Connection speed test
- Browser extension (Chromium, Manifest V3) that hands browser downloads over to the app
- Automatic, signed updates

## What the app does when it runs

This section lists everything the app does outside its own window, so it can be reviewed.

| Behavior | Details |
|---|---|
| Downloads | Only files that the user adds, or that the browser extension hands over. Files go to the configured download folder. |
| Local server | Listens on `127.0.0.1:38653` (loopback only) so the browser extension can send downloads. Requests carrying an `Origin` header that is not `chrome-extension://` or `moz-extension://` are rejected, and web pages cannot reach it. |
| Data storage | A SQLite database (`downloads.db`) and logs in the app data folder. Nothing is sent to any server other than the download hosts the user chose. |
| Auto-update | Checks `github.com/Fatal-IV/download-manager-releases/releases/latest/download/latest.json` at startup and periodically. Update packages are verified against the public key in `src-tauri/tauri.conf.json` before they are installed. |
| Start with Windows | Off unless the user enables it in Settings. |
| Video downloads | On first use of the video feature the app downloads `yt-dlp.exe` (from the official yt-dlp GitHub releases) and `ffmpeg` (from gyan.dev) into its own `tools` folder and runs them. The feature is not used otherwise. |
| Defender scan | Optional. Runs the local `MpCmdRun.exe` of Windows Defender on a finished download. |
| Notifications | Shows a Windows notification when a download finishes. |

The app does not collect telemetry and does not run remote code other than the tools above and its signed updates.

## Building from source

Requirements: a recent Node.js, Rust 1.90+, and the [Tauri prerequisites for Windows](https://tauri.app/start/prerequisites/).

```bash
npm install
npm run tauri dev      # development
npm run build          # build the UI
npm run lint
cd src-tauri && cargo test --lib
npm run tauri build    # NSIS installer (needs a signing key, see below)
```

Release builds are signed with a Tauri updater key (`TAURI_SIGNING_PRIVATE_KEY`). The matching public key is in `src-tauri/tauri.conf.json`. The private key is not part of this repository; to build your own installer, generate a key with `npm run tauri signer generate` and replace the public key. `scripts/release.mjs` is the script used to publish releases.

Installers are not code-signed with a Windows certificate, so Windows SmartScreen may show a warning and some antivirus products may flag them heuristically. You can verify a build by compiling the source yourself.

## Browser extension

The `extension/` folder is an unpacked Manifest V3 extension. In Chrome or Edge open `chrome://extensions`, enable developer mode, choose "Load unpacked" and select the folder. It only works while the app is running; otherwise the browser downloads the file itself. Downloads that start from `blob:` or `data:` addresses stay in the browser because the app cannot fetch them.

## Project layout

```
src/            React UI
src-tauri/      Rust backend (download engine, manager, local server, tray, updater)
extension/      Browser extension
scripts/        Release script
```

## License

Source-available, all rights reserved. The code is public so it can be reviewed and built for personal use, but it may not be redistributed, modified for distribution or used commercially without permission. See [LICENSE](LICENSE).
