#!/usr/bin/env bash
# Download sidecar binaries (yt-dlp, ffmpeg, ffprobe) for the current build target.
# Run before `npm run tauri build` or `npm run dev` if sidecars aren't present.
#
# Usage:
#   bash scripts/download-sidecars.sh                  # auto-detect target
#   bash scripts/download-sidecars.sh x86_64-unknown-linux-gnu
#   bash scripts/download-sidecars.sh x86_64-pc-windows-msvc

set -euo pipefail

# ── Pinned versions ───────────────────────────────────────────────────────────
# Update these when shipping a new ClipCut release that bundles newer tools.
YT_DLP_VERSION="2026.08.19"
FFMPEG_VERSION="7.1"

# ── Target detection ──────────────────────────────────────────────────────────
if [ "${1:-}" != "" ]; then
  TARGET="$1"
else
  TARGET="$(rustc -vV 2>/dev/null | grep '^host:' | cut -d' ' -f2)"
fi

if [ -z "$TARGET" ]; then
  echo "ERROR: Could not detect Rust target triple. Pass it as an argument." >&2
  exit 1
fi

echo "Target: $TARGET"

BINDIR="$(cd "$(dirname "$0")/../src-tauri/binaries" && pwd)"
mkdir -p "$BINDIR"

# ── yt-dlp ────────────────────────────────────────────────────────────────────
download_ytdlp() {
  local target="$1"
  local dest="$BINDIR/yt-dlp-${target}"

  if [ -f "$dest" ] && [ -s "$dest" ]; then
    echo "yt-dlp already present: $dest"
    return
  fi

  case "$target" in
    x86_64-unknown-linux-gnu)
      local url="https://github.com/yt-dlp/yt-dlp/releases/download/${YT_DLP_VERSION}/yt-dlp"
      ;;
    aarch64-unknown-linux-gnu)
      local url="https://github.com/yt-dlp/yt-dlp/releases/download/${YT_DLP_VERSION}/yt-dlp_linux_aarch64"
      ;;
    x86_64-pc-windows-msvc)
      dest="${dest}.exe"
      local url="https://github.com/yt-dlp/yt-dlp/releases/download/${YT_DLP_VERSION}/yt-dlp.exe"
      ;;
    *)
      echo "WARNING: No known yt-dlp binary for target '$target'. Skipping."
      return
      ;;
  esac

  echo "Downloading yt-dlp ${YT_DLP_VERSION} for ${target}..."
  curl -fsSL --retry 3 "$url" -o "$dest"
  chmod +x "$dest" 2>/dev/null || true
  echo "  → $dest"
}

# ── ffmpeg + ffprobe ──────────────────────────────────────────────────────────
download_ffmpeg() {
  local target="$1"
  local dest_ffmpeg="$BINDIR/ffmpeg-${target}"
  local dest_ffprobe="$BINDIR/ffprobe-${target}"

  case "$target" in
    x86_64-unknown-linux-gnu)
      local url="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linux64-gpl.tar.xz"
      local strip_components=2
      ;;
    aarch64-unknown-linux-gnu)
      local url="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linuxarm64-gpl.tar.xz"
      local strip_components=2
      ;;
    x86_64-pc-windows-msvc)
      dest_ffmpeg="${dest_ffmpeg}.exe"
      dest_ffprobe="${dest_ffprobe}.exe"
      local url="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"
      local strip_components=0  # handled differently for zip
      ;;
    *)
      echo "WARNING: No known ffmpeg binary for target '$target'. Skipping."
      return
      ;;
  esac

  if [ -f "$dest_ffmpeg" ] && [ -s "$dest_ffmpeg" ]; then
    echo "ffmpeg already present: $dest_ffmpeg"
    if [ -f "$dest_ffprobe" ] && [ -s "$dest_ffprobe" ]; then
      echo "ffprobe already present: $dest_ffprobe"
      return
    fi
  fi

  echo "Downloading ffmpeg (latest GPL) for ${target}..."

  local tmpdir
  tmpdir="$(mktemp -d)"

  case "$target" in
    x86_64-pc-windows-msvc)
      local zipfile="$tmpdir/ffmpeg.zip"
      curl -fsSL --retry 3 "$url" -o "$zipfile"
      unzip -j "$zipfile" '*/bin/ffmpeg.exe' '*/bin/ffprobe.exe' -d "$tmpdir"
      mv "$tmpdir/ffmpeg.exe" "$dest_ffmpeg"
      mv "$tmpdir/ffprobe.exe" "$dest_ffprobe"
      ;;
    *)
      curl -fsSL --retry 3 "$url" \
        | tar -xJ --strip-components="$strip_components" -C "$tmpdir" --wildcards '*/bin/ffmpeg' '*/bin/ffprobe'
      mv "$tmpdir/ffmpeg" "$dest_ffmpeg"
      mv "$tmpdir/ffprobe" "$dest_ffprobe"
      chmod +x "$dest_ffmpeg" "$dest_ffprobe"
      ;;
  esac

  rm -rf "$tmpdir"
  echo "  → $dest_ffmpeg"
  echo "  → $dest_ffprobe"
}

# ── Run ───────────────────────────────────────────────────────────────────────
download_ytdlp "$TARGET"
download_ffmpeg "$TARGET"

echo ""
echo "Sidecars ready in $BINDIR:"
ls -lh "$BINDIR/"
