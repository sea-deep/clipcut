import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import {
  type AppState,
  type ClipEntry,
  type ExportOptions,
  type ExportResult,
  type VideoInfo,
  allClipsValid,
  clipFromRange,
  clipsFromPaste,
  emptyClip,
  toIpcClips,
  validateClip,
} from "./state";
import { formatTimestamp, parseRange } from "./timestamps";

// ── DOM refs ─────────────────────────────────────────────────────────────────

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

const urlInput = $<HTMLInputElement>("url-input");
const loadBtn = $<HTMLButtonElement>("load-btn");
const urlError = $("url-error");
const videoInfoSection = $("video-info");
const thumbnail = $<HTMLImageElement>("thumbnail");
const thumbnailLink = $<HTMLAnchorElement>("thumbnail-link");
const videoTitleEl = $("video-title");
const videoDurationEl = $("video-duration");
const previewBtn = $<HTMLButtonElement>("preview-btn");
const clipsList = $("clips-list");
const rangeInput = $<HTMLInputElement>("range-input");
const addClipBtn = $<HTMLButtonElement>("add-clip-btn");
const clipError = $("clip-error");
const outputPath = $("output-path");
const changeFolderBtn = $<HTMLButtonElement>("change-folder-btn");
const formatSelect = $<HTMLSelectElement>("format-select");
const exportBtn = $<HTMLButtonElement>("export-btn");
const exportCount = $("export-count");
const progressOverlay = $("progress-overlay");
const progressPhase = $("progress-phase");
const progressFill = $<HTMLElement>("progress-fill");
const progressPercent = $("progress-percent");
const progressClips = $("progress-clips");
const cancelBtn = $<HTMLButtonElement>("cancel-btn");
const doneOverlay = $("done-overlay");
const doneMessage = $("done-message");
const openFolderBtn = $<HTMLButtonElement>("open-folder-btn");
const doneCloseBtn = $<HTMLButtonElement>("done-close-btn");
const themeToggle = $<HTMLButtonElement>("theme-toggle");
const sidecarError = $("sidecar-error");
const sidecarErrorMsg = $("sidecar-error-msg");

// ── App state ─────────────────────────────────────────────────────────────────

let appState: AppState = { phase: "empty" };
let clips: ClipEntry[] = [];
let outputDir = "";
let lastExportResult: ExportResult | null = null;

// ── Init ──────────────────────────────────────────────────────────────────────

document.addEventListener("DOMContentLoaded", async () => {
  // Fetch default output dir from Rust (never hardcode paths)
  outputDir = await invoke<string>("get_default_output_dir");
  renderOutputPath();

  setupTheme();
  setupEventListeners();
  setupProgressListener();
  setupSidecarListener();
});

// ── Theme ─────────────────────────────────────────────────────────────────────

function setupTheme() {
  const saved = localStorage.getItem("clipcut-theme");
  if (saved) applyTheme(saved);
  updateThemeIcon();
}

function applyTheme(theme: string) {
  if (theme === "auto") {
    document.documentElement.removeAttribute("data-theme");
  } else {
    document.documentElement.setAttribute("data-theme", theme);
  }
}

function updateThemeIcon() {
  const current = document.documentElement.getAttribute("data-theme");
  themeToggle.querySelector(".theme-icon")!.textContent = current === "dark" ? "☀" : "🌙";
}

themeToggle.addEventListener("click", () => {
  const current = document.documentElement.getAttribute("data-theme");
  const next = current === "dark" ? "light" : "dark";
  applyTheme(next);
  localStorage.setItem("clipcut-theme", next);
  updateThemeIcon();
});

// ── Event Listeners ───────────────────────────────────────────────────────────

function setupEventListeners() {
  // URL load
  loadBtn.addEventListener("click", handleLoad);
  urlInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") handleLoad();
  });

  // URL drag-and-drop
  const urlSection = $("url-section");
  urlSection.addEventListener("dragover", (e) => {
    e.preventDefault();
    urlSection.classList.add("drag-over");
  });
  urlSection.addEventListener("dragleave", () => urlSection.classList.remove("drag-over"));
  urlSection.addEventListener("drop", (e) => {
    e.preventDefault();
    urlSection.classList.remove("drag-over");
    const text = e.dataTransfer?.getData("text/plain") ?? "";
    const lines = text.split("\n").map((l) => l.trim()).filter(Boolean);
    const ytUrl = lines.find((l) => l.includes("youtu"));
    if (ytUrl) {
      urlInput.value = ytUrl;
      handleLoad();
    }
  });

  // Add clip
  addClipBtn.addEventListener("click", addEmptyClip);

  // Paste range
  rangeInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") handleRangeInput();
  });
  rangeInput.addEventListener("paste", (e) => {
    e.preventDefault();
    const text = e.clipboardData?.getData("text") ?? "";
    handleRangeInput(text);
  });

  // Change folder
  changeFolderBtn.addEventListener("click", handleChangeFolder);

  // Export
  exportBtn.addEventListener("click", handleExport);

  // Ctrl+Enter to export
  document.addEventListener("keydown", (e) => {
    if ((e.ctrlKey || e.metaKey) && e.key === "Enter" && !exportBtn.disabled) {
      handleExport();
    }
  });

  // Cancel
  cancelBtn.addEventListener("click", handleCancel);

  // Done overlay
  openFolderBtn.addEventListener("click", () => {
    if (lastExportResult) {
      invoke("open_directory", { path: lastExportResult.output_directory });
    }
  });
  doneCloseBtn.addEventListener("click", hideDoneOverlay);

  // Escape closes overlays
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      if (!progressOverlay.classList.contains("hidden")) handleCancel();
      else if (!doneOverlay.classList.contains("hidden")) hideDoneOverlay();
    }
  });

  // Preview
  previewBtn.addEventListener("click", handlePreview);
}

// ── Load Video ────────────────────────────────────────────────────────────────

async function handleLoad() {
  const url = urlInput.value.trim();
  if (!url) {
    showUrlError("Please enter a YouTube URL.");
    return;
  }

  clearUrlError();
  setLoadingState(true);

  try {
    const video = await invoke<VideoInfo>("load_video", { url });
    onVideoLoaded(video);
  } catch (e: unknown) {
    const err = e as { message?: string };
    showUrlError(err?.message ?? "Could not load video.");
    setLoadingState(false);
    hideVideoInfo();
  }
}

function onVideoLoaded(video: VideoInfo) {
  appState = { phase: "video_loaded", video };
  setLoadingState(false);

  thumbnail.src = video.thumbnail_url;
  thumbnail.alt = video.title;
  thumbnailLink.href = video.url;
  videoTitleEl.textContent = video.title;
  videoDurationEl.textContent = formatTimestamp(video.duration_seconds);
  videoInfoSection.classList.remove("hidden");

  // Reset clips on new video load
  clips = [];
  renderClips();
  updateExportButton();
}

function setLoadingState(loading: boolean) {
  loadBtn.disabled = loading;
  loadBtn.textContent = loading ? "Loading…" : "Load";
  urlInput.disabled = loading;
}

function hideVideoInfo() {
  videoInfoSection.classList.add("hidden");
}

// ── Clip Management ───────────────────────────────────────────────────────────

function currentVideoDuration(): number {
  if (appState.phase === "video_loaded" || appState.phase === "exporting" || appState.phase === "done") {
    return appState.video.duration_seconds;
  }
  return Infinity;
}

function addEmptyClip() {
  clips.push(emptyClip());
  renderClips();
  updateExportButton();
  // Focus the first input of the new row
  const rows = clipsList.querySelectorAll<HTMLInputElement>(".clip-row input.clip-start");
  const last = rows[rows.length - 1];
  last?.focus();
}

function handleRangeInput(text?: string) {
  const raw = text ?? rangeInput.value.trim();
  if (!raw) {
    addEmptyClip();
    return;
  }

  // Try multi-range paste first
  const multi = clipsFromPaste(raw);
  if (multi.length > 0) {
    const dur = currentVideoDuration();
    clips.push(...multi.map((c) => validateClip(c, dur)));
    rangeInput.value = "";
    renderClips();
    updateExportButton();
  } else {
    // Show error below range input
    clipError.textContent = "Could not parse range. Use format: 0:15-0:42";
    clipError.classList.remove("hidden");
    setTimeout(() => clipError.classList.add("hidden"), 3000);
  }
}

function removeClip(id: string) {
  clips = clips.filter((c) => c.id !== id);
  renderClips();
  updateExportButton();
}

function onClipStartChange(id: string, raw: string) {
  clips = clips.map((c) => {
    if (c.id !== id) return c;
    const updated = { ...c, startRaw: raw };
    return validateClip(updated, currentVideoDuration());
  });
  renderClipErrors();
  updateExportButton();
}

function onClipEndChange(id: string, raw: string) {
  clips = clips.map((c) => {
    if (c.id !== id) return c;
    const updated = { ...c, endRaw: raw };
    return validateClip(updated, currentVideoDuration());
  });
  renderClipErrors();
  updateExportButton();
}

function onClipEndKeyDown(e: KeyboardEvent, id: string) {
  if (e.key === "Enter") {
    e.preventDefault();
    // Add a new clip row and focus it
    addEmptyClip();
  }
}

// ── Render ────────────────────────────────────────────────────────────────────

function renderClips() {
  clipsList.innerHTML = "";
  clips.forEach((clip, _i) => {
    const row = document.createElement("div");
    row.className = "clip-row";
    row.setAttribute("role", "listitem");
    row.dataset.id = clip.id;

    row.innerHTML = `
      <input
        type="text"
        class="input clip-start${clip.error && clip.startRaw ? " input--error" : ""}"
        value="${escHtml(clip.startRaw)}"
        placeholder="0:15"
        aria-label="Start time"
      />
      <span class="clip-arrow" aria-hidden="true">→</span>
      <input
        type="text"
        class="input clip-end${clip.error ? " input--error" : ""}"
        value="${escHtml(clip.endRaw)}"
        placeholder="0:42"
        aria-label="End time"
      />
      <button class="btn btn--ghost btn--icon clip-remove" aria-label="Remove clip" title="Remove">×</button>
      ${clip.error ? `<p class="clip-row-error" role="alert">${escHtml(clip.error)}</p>` : ""}
    `;

    const startInput = row.querySelector<HTMLInputElement>(".clip-start")!;
    const endInput = row.querySelector<HTMLInputElement>(".clip-end")!;
    const removeBtn = row.querySelector<HTMLButtonElement>(".clip-remove")!;

    startInput.addEventListener("input", () => onClipStartChange(clip.id, startInput.value));
    startInput.addEventListener("blur", () => onClipStartChange(clip.id, startInput.value));
    endInput.addEventListener("input", () => onClipEndChange(clip.id, endInput.value));
    endInput.addEventListener("blur", () => onClipEndChange(clip.id, endInput.value));
    endInput.addEventListener("keydown", (e) => onClipEndKeyDown(e, clip.id));
    removeBtn.addEventListener("click", () => removeClip(clip.id));

    clipsList.appendChild(row);
  });
}

function renderClipErrors() {
  const rows = clipsList.querySelectorAll<HTMLElement>(".clip-row");
  clips.forEach((clip, i) => {
    const row = rows[i];
    if (!row) return;
    const startInput = row.querySelector<HTMLInputElement>(".clip-start");
    const endInput = row.querySelector<HTMLInputElement>(".clip-end");
    let errorEl = row.querySelector<HTMLElement>(".clip-row-error");

    if (startInput) startInput.classList.toggle("input--error", !!(clip.error && clip.startRaw));
    if (endInput) endInput.classList.toggle("input--error", !!clip.error);

    if (clip.error) {
      if (!errorEl) {
        errorEl = document.createElement("p");
        errorEl.className = "clip-row-error";
        errorEl.setAttribute("role", "alert");
        row.appendChild(errorEl);
      }
      errorEl.textContent = clip.error;
    } else if (errorEl) {
      errorEl.remove();
    }
  });
}

function renderOutputPath() {
  // Show tilde-shortened path on Linux/macOS
  const home = "/home/";
  const display = outputDir.startsWith(home)
    ? "~/" + outputDir.slice(home.indexOf("/", 1) + 1)
    : outputDir;
  // Simplistic: just show last 2 segments if very long
  const parts = display.split(/[/\\]/);
  const short = parts.length > 4 ? "…/" + parts.slice(-3).join("/") : display;
  outputPath.textContent = short;
  outputPath.title = outputDir;
}

function updateExportButton() {
  const valid = appState.phase === "video_loaded" && allClipsValid(clips);
  exportBtn.disabled = !valid;
  exportCount.textContent = String(clips.filter((c) => c.startSeconds !== null && c.endSeconds !== null && !c.error).length);
}

// ── Folder Picker ─────────────────────────────────────────────────────────────

async function handleChangeFolder() {
  const selected = await openDialog({ directory: true, defaultPath: outputDir });
  if (selected && typeof selected === "string") {
    outputDir = selected;
    renderOutputPath();
  }
}

// ── Export ────────────────────────────────────────────────────────────────────

async function handleExport() {
  if (appState.phase !== "video_loaded") return;
  const video = appState.video;
  const ipcClips = toIpcClips(clips);
  if (ipcClips.length === 0) return;

  const modeInput = document.querySelector<HTMLInputElement>('input[name="mode"]:checked');
  const mode = (modeInput?.value ?? "accurate") as "accurate" | "fast";
  const format = formatSelect.value as "mp4" | "webm" | "mkv";

  const options: ExportOptions = {
    output_directory: outputDir,
    mode,
    format,
  };

  appState = { phase: "exporting", video };
  showProgressOverlay();
  setProgressPhase("Preparing…", 0);

  try {
    const result = await invoke<ExportResult>("export_clips", {
      video,
      clips: ipcClips,
      options,
    });

    lastExportResult = result;
    hideProgressOverlay();

    if (result.cancelled) {
      doneMessage.textContent = `Cancelled — ${result.clips_completed} of ${result.clips_total} clips saved.`;
    } else {
      doneMessage.textContent = `✓ ${result.clips_completed} clip${result.clips_completed !== 1 ? "s" : ""} exported.`;
    }
    showDoneOverlay();
    appState = { phase: "done", video, result };
  } catch (e: unknown) {
    const err = e as { message?: string };
    hideProgressOverlay();
    showUrlError(err?.message ?? "Export failed.");
    appState = { phase: "video_loaded", video };
  }
}

async function handleCancel() {
  await invoke("cancel_export");
  hideProgressOverlay();
}

// ── Preview ───────────────────────────────────────────────────────────────────

function handlePreview() {
  if (appState.phase !== "video_loaded" && appState.phase !== "exporting" && appState.phase !== "done") return;
  const video = appState.video;
  // Open in default browser as reliable fallback (WebView embed can be restricted)
  invoke("open_directory", { path: video.url });
}

// ── Progress Listener ─────────────────────────────────────────────────────────

function setupProgressListener() {
  listen("export-progress", (event) => {
    const payload = event.payload as Record<string, unknown>;
    const phase = payload["phase"] as string;

    switch (phase) {
      case "preparing":
        setProgressPhase("Preparing…", 0);
        break;
      case "downloading": {
        const pct = payload["percent"] as number | null;
        if (pct !== null && pct !== undefined) {
          setProgressPhase(`Downloading source…`, Math.round(pct * 0.6));
          progressPercent.textContent = `${Math.round(pct)}%`;
        } else {
          setProgressPhase("Downloading source…", -1); // indeterminate
        }
        break;
      }
      case "processing": {
        const ci = payload["clip_index"] as number;
        const ct = payload["clips_total"] as number;
        const overall = payload["overall_percent"] as number;
        setProgressPhase(`Processing clips`, Math.round(overall));
        progressClips.textContent = `${ci} of ${ct}`;
        break;
      }
      case "finalizing":
        setProgressPhase("Finalising…", 99);
        break;
      case "complete":
        setProgressPhase("Complete", 100);
        break;
      case "cancelled":
        hideProgressOverlay();
        break;
    }
  });
}

function setupSidecarListener() {
  listen("sidecar-status", (event) => {
    const payload = event.payload as { ok: boolean; yt_dlp: string };
    if (!payload.ok) {
      sidecarErrorMsg.textContent =
        "ClipCut could not start its video tools. Try reinstalling ClipCut.";
      sidecarError.classList.remove("hidden");
    }
  });
}

// ── Progress UI ───────────────────────────────────────────────────────────────

function showProgressOverlay() {
  progressOverlay.classList.remove("hidden");
  progressFill.style.width = "0%";
  progressPercent.textContent = "0%";
  progressClips.textContent = "";
}

function hideProgressOverlay() {
  progressOverlay.classList.add("hidden");
}

function setProgressPhase(label: string, percent: number) {
  progressPhase.textContent = label;
  if (percent < 0) {
    // Indeterminate
    progressFill.style.width = "100%";
    progressFill.classList.add("progress-fill--indeterminate");
    progressPercent.textContent = "…";
  } else {
    progressFill.style.width = `${percent}%`;
    progressFill.classList.remove("progress-fill--indeterminate");
    progressPercent.textContent = `${percent}%`;
    progressOverlay
      .querySelector(".progress-bar")!
      .setAttribute("aria-valuenow", String(percent));
  }
}

function showDoneOverlay() {
  doneOverlay.classList.remove("hidden");
  openFolderBtn.focus();
}

function hideDoneOverlay() {
  doneOverlay.classList.add("hidden");
  if (appState.phase === "done") {
    appState = { phase: "video_loaded", video: appState.video };
  }
}

// ── Error Helpers ─────────────────────────────────────────────────────────────

function showUrlError(msg: string) {
  urlError.textContent = msg;
  urlError.classList.remove("hidden");
  urlInput.setAttribute("aria-invalid", "true");
}

function clearUrlError() {
  urlError.classList.add("hidden");
  urlInput.removeAttribute("aria-invalid");
}

// ── Utils ─────────────────────────────────────────────────────────────────────

function escHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}
