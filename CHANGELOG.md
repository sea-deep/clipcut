# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
  - Sidecar downloading script (`scripts/download-sidecars.sh`) for Linux (`x86_64-unknown-linux-gnu`) and Windows (`x86_64-pc-windows-msvc`).
  - GitHub Actions matrix workflow generating Windows setup executables (`.exe`) and Linux packages (`.AppImage`, `.deb`).
