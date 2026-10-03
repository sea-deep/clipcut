## Download Strategy Threshold

**Initial assumption:**
Use `--download-sections` (partial download) when total clip coverage < 40% of video duration.
Use full download otherwise.

**Rationale:**
40% is a reasonable starting point. For a 1-hour video with 3 clips totalling 10 minutes,
partial download fetches ~17% of the data. For a 10-minute video with clips spanning 8 minutes,
full download and local clipping is faster due to lower overhead.

**Status:**
Partial download is not yet fully wired (download.rs v0.1 uses full as safe fallback while
partial section file matching is verified). See export.rs TODO comment.

**Measurement required:**
Benchmark partial vs full on:
- Dev Arch machine: 1hr video, 3x30s clips
- GitHub CI: synthetic fixture

**Decision will be revised** after benchmarking on real hardware.

---

## Cancellation Implementation

**Initial assumption:**
Store child PIDs globally; kill on cancel.

**Observed:**
Tauri's shell plugin does not expose a synchronous kill() on a spawned sidecar handle
in the same way as std::process::Child. PIDs are retrieved from child.pid() and killed
via OS signal (SIGTERM on Unix, TerminateProcess on Windows).

**Decision:**
Use atomic cancel flag + PID tracking in cancel.rs.
On cancel: set flag, kill tracked PIDs, clean up .part files.

---

## Accurate vs Fast Cut Mode

**Context:**
User-facing toggle. "Accurate" re-encodes to hit exact timestamps.
"Fast" uses stream copy, which cuts at the nearest keyframe.

**Decision:**
Default to Accurate. The app is used for clip editing — timestamp precision matters.
Fast mode is opt-in with a clear explanation that avoids codec terminology.

---

## Output Format Defaults

**Context:**
MP4 with H.264 + AAC chosen as default.

**Reason:**
Broadest compatibility with video editing software (Premiere, DaVinci, Final Cut,
iMovie, Windows Video Editor). WebM and MKV are available but users must opt in.
Changing the container extension without changing the codec is explicitly prevented
in ffmpeg.rs — the codec args differ per format.

---

## Stale Metadata Prevention

**Problem:**
User loads URL A, immediately loads URL B. URL A may return first (fast cache hit).

**Solution:**
Generation counter in commands/mod.rs. Each load_video call increments the counter.
If the counter has changed when the response arrives, the response is silently dropped.
Returns Cancelled error, which the frontend ignores for stale loads.

---

## sidecar_command Fallback

**Decision:**
In release builds, only Tauri sidecar binaries are used.
System PATH binaries are not used in release.
During development, Tauri's sidecar() may fail if binaries are not downloaded —
developers must run `scripts/download-sidecars.sh` first.
