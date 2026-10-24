# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-10-24

### Fixed
- **Packaging & Bundling**:
  - Rebuilt valid high-resolution multi-size Windows icon (`icon.ico`) and Apple icon (`icon.icns`), resolving the ICO image data size mismatch that blocked macOS DMG and Windows NSIS packaging.
  - Fixed directory initialization order in `scripts/download-sidecars.sh` before running target resolution.
  - Added Windows `.exe` extension detection prior to cache checks in sidecar downloads.
  - Repaired CI matrix and runner definitions to target macOS Apple Silicon (`aarch64-apple-darwin`), macOS Intel (`x86_64-apple-darwin`), Windows (`x86_64-pc-windows-msvc`), and Linux (`x86_64-unknown-linux-gnu`).
  - Added missing system libraries and sidecar provisioning to CI development checks.
- **Extraction & Transcoding**:
  - Bound FFmpeg `-vf` scale filters to user quality presets (`1080p`, `720p`, `480p`, `360p`) during local source cuts, guaranteeing outputs match chosen dimensions.
  - Added `-avoid_negative_ts make_zero` to prevent audio offset and timestamp drift during frame seeking.
  - Automated deletion of partial `.part` and `.ytdl` files upon export cancellation or non-zero exit codes.
  - Tree-wide process termination (`taskkill /T /F`) on Windows to terminate child helper processes cleanly.
  - Guaranteed default output directory fallback when directory selection is empty.
- **Edge Cases & URL Handling**:
  - Added support for YouTube embeds (`youtube.com/embed/`), short links, live broadcast archives, and `youtube-nocookie.com`.
  - Added detection for active live streams with explicit warnings instead of generic unavailable errors.
  - Automated start timestamp extraction from incoming URL parameters (`?t=`, `&t=`, `?start=`).
  - Added support for range formats using en-dash (`–`), em-dash (`—`), arrows (`->`, `→`), range dots (`..`), `to`, bracketed blocks (`[0:15 - 0:42]`), and inline chapter titles.
  - Handled device name collisions on Windows (e.g. `con.mp4`, `aux.webm`) and stripped trailing spaces/dots in filename sanitizer.
  - Granted `opener:allow-open-url` permissions and routed external web links through the system browser.
  - Added `prefers-reduced-motion` media queries for accessibility.

## [0.1.0] - 2026-10-03

### Added
- **Core Extraction Engine**:
  - Section-based extraction using `yt-dlp --download-sections` and FFmpeg.
  - Pre-fetched metadata stream caching (`info.json`) to bypass duplicate YouTube HTML/API handshakes and JavaScript challenge parsing.
  - Quality selector supporting `Highest` (default), `1080p`, `720p`, `480p`, and `360p`.
  - Container format selection for MP4, WebM, and MKV.
  - Multi-socket parallel fragment acceleration (`-N 4`) and forced IPv4 routing (`-4`).
- **User Interface**:
  - GNOME Libadwaita design language with pure vector SVG iconography and zero emoji dependencies.
  - Non-blocking export queue positioned beneath the export button with real-time per-job progress indicators, cancellation controls, and folder opening shortcuts.
  - Dual tab-navigable Start and End input fields with auto-formatting and support for bare seconds (e.g. `15`), timestamp notation (`0:15`, `1:02:30`), and batch range clipboard pasting.
  - Auto-load trigger on valid YouTube URL paste.
  - System theme integration with manual dark and light mode toggle.
- **Packaging & CI/CD**:
  - Sidecar downloading script (`scripts/download-sidecars.sh`) for Linux, Windows, and macOS (Apple Silicon and Intel).
  - Arch Linux native package (`clipcut-bin-0.1.0-1-x86_64.pkg.tar.zst`) with PKGBUILD.
  - GitHub Actions matrix workflow generating Windows setup executables (`.exe`), Linux packages (`.AppImage`, `.deb`), and macOS disk images (`.dmg`).
