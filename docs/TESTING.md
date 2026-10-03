## Test Coverage

### Timestamp parsing (src/timestamps.ts)
Run: `npm run test:ts`

Covered:
- Bare seconds: `15`
- MM:SS: `0:15`, `01:20`
- H:MM:SS: `1:02:15`
- Empty string → null
- Letters → null
- Negative → null
- Too many parts → null
- NaN → null
- Leading zeros
- Range parsing: `0:15-0:42`, `2:10 - 2:35`
- Multi-range: newline-separated, comma-separated, mixed with invalid lines

### Filename sanitization (src-tauri/src/paths.rs)
Run: `cargo test`

Covered:
- Normal title
- Forbidden Windows characters: `/ \ : * ? " < > |`
- Trailing dot
- Windows reserved names: CON, NUL, COM1, etc.
- Empty string
- Unicode (Japanese)
- Emoji
- Very long string (truncation at 200 chars)

### Clip filename generation (src-tauri/src/paths.rs)
- Under 1 hour format
- Over 1 hour format

### Clip validation (src-tauri/src/clip.rs)
- Valid clip
- End before start
- Equal times
- Timestamp beyond duration
- Negative start

### Download strategy (src-tauri/src/download.rs)
- Small coverage → Partial
- Dense coverage → Full
- Cached full → always Full
- Exactly at threshold (40%)
- Just below threshold

### Progress parsing (src-tauri/src/progress.rs)
- yt-dlp `[download] XX.X%` line
- yt-dlp 100%
- Non-download line → None
- ffmpeg `out_time_us=N`
- ffmpeg other line → None

---

## Manual Test Sheet

Run before each release. Document results in: test results go into issue tracker or KNOWN_ISSUES.md.

### Core Flow
- [ ] Public short YouTube video (< 5 min), 1 clip, accurate mode, MP4
- [ ] 20+ minute video, 3 clips, accurate mode
- [ ] 1+ hour video, 1 clip near start, 1 clip near end
- [ ] 10 clips from same video
- [ ] Overlapping clips (should produce separate files)
- [ ] Clips near beginning (0s → 10s)
- [ ] Clips near end (last 30s)

### Quality
- [ ] 1080p source: verify output is 1080p
- [ ] Source with separate video/audio streams: verify audio present in output
- [ ] Play several outputs — verify start/end timing matches input

### Error Cases
- [ ] Invalid URL (non-YouTube) → error message shown, no crash
- [ ] Unavailable video → clear error message
- [ ] Cancel during download → partial temp files removed
- [ ] Cancel during ffmpeg → partial output files removed
- [ ] Output folder doesn't exist → auto-created

### Platform
- [ ] Linux: XDG Videos directory resolved correctly
- [ ] Linux: Wayland — no visual glitches
- [ ] Windows: Videos directory resolved via OS API
- [ ] Windows: Paths with spaces and Unicode
- [ ] Windows: 125% display scaling — no clipped text
- [ ] Windows: 150% display scaling — all controls usable
- [ ] Both: Dark theme — adequate contrast
- [ ] Both: Light theme — adequate contrast

### Packaging
- [ ] AppImage launches on Ubuntu 22.04 LTS (fresh install)
- [ ] NSIS installer on Windows 10 — installs without admin
- [ ] Uninstall and reinstall — no leftover state causing errors
