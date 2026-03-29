# Focus Mode

> Your game. Your screen. Your rules.

Focus Mode is a Windows desktop app that automatically disables your secondary monitors when a game starts — and restores them when you quit. No distractions, no alt-tabbing to a dark screen, no manual display switching.

---

## Features

- **Auto-detection** — Scans your Steam library and detects installed games
- **Per-game monitor config** — Choose exactly which monitors to disable for each game
- **Custom games** — Add any non-Steam executable to the monitoring list
- **Background monitoring** — Runs silently in the system tray, always watching
- **Smart polling** — Adaptive check intervals to minimize CPU usage
- **Auto-updater** — Notifies you of new versions and installs them in one click
- **Persistent config** — All settings survive restarts (saved to `%APPDATA%\FocusMode\config.json`)

---

## How It Works

1. You enable Focus Mode for a game from the app
2. Focus Mode monitors running processes in the background
3. When the game's executable is detected, your secondary monitors are disabled
4. When the game exits, your full display setup is restored automatically

For single-toggle simplicity it uses `DisplaySwitch.exe /internal`. For precise per-monitor control it uses the Windows CCD API (`SetDisplayConfig`).

---

## Installation

Download the latest `.exe` installer from the [Releases](https://github.com/gabriel-mase/focus-mode-exe/releases) page and run it. Focus Mode will add itself to the system tray on startup.

---

## Usage

### Enable a game
1. Find your game in the list (Steam games are auto-discovered)
2. Click the toggle to enable Focus Mode for it
3. That's it — Focus Mode handles the rest

### Configure which monitors to disable
- Click the monitor icon on any enabled game
- The popover shows all connected displays
- Toggle the secondary monitors you want disabled for that specific game
- Leave it on default to disable all non-primary monitors

### Add a non-Steam game
- Click **Add Game** in the toolbar
- Enter the game name and browse to its `.exe`
- Enable it like any Steam game

### Tray icon
Focus Mode hides to the system tray when you close the window. Right-click the tray icon to show the window or quit the app. On quit, all monitors are restored.

---

## Tech Stack

| Layer | Technology |
|---|---|
| UI | React 19 + TypeScript + Mantine |
| Desktop runtime | Tauri 2 (Rust) |
| Build tool | Vite |
| Process detection | sysinfo |
| Display control | Windows CCD API + WinAPI |
| Steam integration | Registry + VDF/ACF parsing |

---

## Development

### Prerequisites

- [Node.js](https://nodejs.org/) 20+
- [Rust](https://rustup.rs/) (stable)
- [Tauri prerequisites for Windows](https://tauri.app/start/prerequisites/)

### Run locally

```bash
npm install
npm run tauri dev
```

### Build installer

```bash
npm run tauri build
```

The signed installer will be in `src-tauri/target/release/bundle/`.

---

## Releases

Releases are automated via GitHub Actions. Push a tag matching `v*` to trigger the workflow:

```bash
git tag v1.0.2
git push origin v1.0.2
```

The workflow builds the app, signs it, creates a GitHub release, and uploads `latest.json` for the auto-updater.

---

## License

MIT
