# ClipCut

**Paste a YouTube URL, set timestamps, export clips.**

A small, fast desktop utility for extracting multiple clips from a YouTube video.
No Python, no ffmpeg setup, no yt-dlp configuration required.

---

## Download

| Platform | File |
|---|---|
| Windows 10/11 x86_64 | `ClipCut-x.y.z-x64-setup.exe` |
| Linux x86_64 | `ClipCut-x.y.z.AppImage` |

Get the latest release from [GitHub Releases](https://github.com/sea-deep/clipcut/releases).

---

## Usage

```
1. Paste a YouTube URL
2. Click Load
3. Enter timestamp ranges (e.g. 0:15-0:42)
4. Click Export
5. Open Videos/ClipCut/<title>/
```

**Timestamp formats accepted:**

```
15         →  0:15
0:15       →  0:15
01:20      →  1:20
1:02:15    →  1:02:15
```

**Paste multiple ranges at once:**

```
0:15-0:42
2:10-2:35
5:04-6:12
```

---

## Features

- Downloads the video once, exports all clips efficiently
- **Accurate mode** — frame-exact cuts (re-encodes at CRF 18)
- **Fast mode** — stream copy, instant, cuts at keyframe boundaries
- Output formats: MP4, WebM, MKV
- Dark and light theme with manual override
- Drag-and-drop URL support
- Keyboard-first: Enter/Tab/Ctrl+Enter/Escape
- `Videos/ClipCut/<title>/` output structure on both Windows and Linux
- No accounts, no cloud, no analytics

---

## Output

```
Videos/
└── ClipCut/
    └── Example Video Title/
        ├── 01_00-15_00-42.mp4
        ├── 02_02-10_02-35.mp4
        └── 03_05-04_06-12.mp4
```

---

## Build from Source

### 1. System Dependencies

**Arch Linux / Manjaro:**
```bash
sudo pacman -S --needed base-devel webkit2gtk-4.1 gtk3 nodejs npm rust ffmpeg
```

**Ubuntu / Debian:**
```bash
sudo apt update && sudo apt install -y build-essential libwebkit2gtk-4.1-dev libgtk-3-dev nodejs npm rustc cargo ffmpeg
```

**Windows:**
- Install [Node.js 18+](https://nodejs.org)
- Install [Rust](https://rustup.rs)
- Install [Visual Studio C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/)

### 2. Clone and Setup

```bash
git clone https://github.com/sea-deep/clipcut.git
cd clipcut

# Install frontend packages
npm install

# Fetch bundled sidecars (yt-dlp + ffmpeg + ffprobe)
bash scripts/download-sidecars.sh

# Run development app
npm run tauri dev

# Or build standalone release package (AppImage on Linux, .exe on Windows)
npm run tauri build
```

---

## Architecture

```
clipcut/
├── src/             # Vanilla TypeScript frontend (Vite)
├── src-tauri/       # Rust backend (Tauri 2)
│   └── src/
│       ├── commands/    # IPC commands exposed to frontend
│       ├── download.rs  # yt-dlp wrapper, download strategy
│       ├── ffmpeg.rs    # ffmpeg clip extraction
│       ├── export.rs    # Export pipeline
│       ├── paths.rs     # Platform-safe paths + filename sanitization
│       └── error.rs     # Typed error model
└── scripts/         # Sidecar binary download
```

**Technology:** Tauri 2 · Rust · Vanilla TypeScript · yt-dlp · ffmpeg

---

## License

MIT — see [LICENSE](LICENSE).
