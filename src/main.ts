import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open as openDialog } from "@tauri-apps/plugin-dialog";

import {
  type AppState,
  type ClipEntry,
  type ExportOptions,
  type ExportResult,
  type QualityOption,
  type VideoInfo,
  allClipsValid,
  clipFromRange,
  clipsFromPaste,
  emptyClip,
  toIpcClips,
  validateClip,
} from "./state";
import { extractUrlTimestamp, formatTimestamp, parseRange, parseTimestamp } from "./timestamps";

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
const addStartInput = $<HTMLInputElement>("add-start-input");
const addEndInput = $<HTMLInputElement>("add-end-input");
const addClipBtn = $<HTMLButtonElement>("add-clip-btn");
const clipError = $("clip-error");
const outputPath = $("output-path");
const changeFolderBtn = $<HTMLButtonElement>("change-folder-btn");
const qualitySelect = $<HTMLSelectElement>("quality-select");
const formatSelect = $<HTMLSelectElement>("format-select");
const exportBtn = $<HTMLButtonElement>("export-btn");
const exportCount = $("export-count");
const themeToggle = $<HTMLButtonElement>("theme-toggle");
const sidecarError = $("sidecar-error");
const sidecarErrorMsg = $("sidecar-error-msg");
const queueSection = $("queue-section");
const queueList = $("queue-list");
const clearQueueBtn = $<HTMLButtonElement>("clear-queue-btn");

// ── App state ─────────────────────────────────────────────────────────────────

export interface QueueJob {
  id: string;
  video: VideoInfo;
  clips: ClipEntry[];
  format: "mp4" | "webm" | "mkv";
  quality: QualityOption;
  outputDir: string;
  status:
    "queued" | "preparing" | "downloading" | "processing" | "completed" | "error" | "cancelled";
  percent: number; // -1 = indeterminate, 0..100 = percentage
  statusText: string;
  result?: ExportResult;
  error?: string;
  createdAt: number;
}

let appState: AppState = { phase: "empty" };
let clips: ClipEntry[] = [];
let outputDir = "";
let jobQueue: QueueJob[] = [];
let activeJobId: string | null = null;

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
  const isDark =
    current === "dark" || (!current && window.matchMedia("(prefers-color-scheme: dark)").matches);
  const useEl = themeToggle.querySelector("use");
  if (useEl) {
    useEl.setAttribute("href", isDark ? "#icon-sun" : "#icon-moon");
  }
}

themeToggle.addEventListener("click", () => {
  const current = document.documentElement.getAttribute("data-theme");
  const isDark =
    current === "dark" || (!current && window.matchMedia("(prefers-color-scheme: dark)").matches);
  const next = isDark ? "light" : "dark";
  applyTheme(next);
  localStorage.setItem("clipcut-theme", next);
  updateThemeIcon();
});

// ── Event Listeners ───────────────────────────────────────────────────────────

function setupEventListeners() {
  // URL load — auto-load immediately on paste or typing valid URL
  loadBtn.addEventListener("click", handleLoad);
  urlInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") handleLoad();
  });
  urlInput.addEventListener("paste", (e) => {
    e.preventDefault();
    const pasted = e.clipboardData?.getData("text")?.trim() || "";
    if (pasted) {
      urlInput.value = pasted;
      handleLoad();
    }
  });
  urlInput.addEventListener("input", () => {
    const val = urlInput.value.trim();
    if (
      val.includes("youtube.com/watch") ||
      val.includes("youtu.be/") ||
      val.includes("youtube.com/shorts/") ||
      val.includes("youtube.com/live/") ||
      val.includes("youtube.com/embed/") ||
      val.includes("youtube.com/v/") ||
      val.includes("youtube-nocookie.com/embed/")
    ) {
      handleLoad();
    }
  });

  thumbnailLink.addEventListener("click", (e) => {
    e.preventDefault();
    if (
      appState.phase === "video_loaded" ||
      appState.phase === "exporting" ||
      appState.phase === "done"
    ) {
      invoke("open_directory", { path: appState.video.url });
    }
  });

  // Dynamically enable/update Export button as user types in Start and End fields
  addStartInput.addEventListener("input", updateExportButton);
  addEndInput.addEventListener("input", updateExportButton);

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
    const lines = text
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean);
    const ytUrl = lines.find((l) => l.includes("youtu"));
    if (ytUrl) {
      urlInput.value = ytUrl;
      handleLoad();
    }
  });

  // Dual Clip Fields (Tab-navigable, accepts bare seconds or MM:SS)
  addClipBtn.addEventListener("click", handleAddClip);

  addStartInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      addEndInput.focus();
    }
  });
  addStartInput.addEventListener("blur", () => {
    const parsed = parseTimestamp(addStartInput.value);
    if (parsed !== null) {
      addStartInput.value = formatTimestamp(parsed);
    }
  });
  addStartInput.addEventListener("paste", (e) => {
    const text = e.clipboardData?.getData("text") ?? "";
    if (text.includes("-") || text.includes("\n")) {
      e.preventDefault();
      handleRangePaste(text);
    }
  });

  addEndInput.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      handleAddClip();
    }
  });
  addEndInput.addEventListener("blur", () => {
    const parsed = parseTimestamp(addEndInput.value);
    if (parsed !== null) {
      addEndInput.value = formatTimestamp(parsed);
    }
  });
  addEndInput.addEventListener("paste", (e) => {
    const text = e.clipboardData?.getData("text") ?? "";
    if (text.includes("-") || text.includes("\n")) {
      e.preventDefault();
      handleRangePaste(text);
    }
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

  // Preview
  previewBtn.addEventListener("click", handlePreview);

  // Queue click delegation
  queueList.addEventListener("click", (e) => {
    const target = e.target as HTMLElement;
    const cancelBtn = target.closest<HTMLButtonElement>(".queue-cancel-btn");
    const removeBtn = target.closest<HTMLButtonElement>(".queue-remove-btn");
    const openBtn = target.closest<HTMLButtonElement>(".queue-open-btn");
    const dismissBtn = target.closest<HTMLButtonElement>(".queue-dismiss-btn");

    if (cancelBtn) {
      const id = cancelBtn.dataset.id;
      if (id) handleCancelJob(id);
    } else if (removeBtn) {
      const id = removeBtn.dataset.id;
      if (id) {
        jobQueue = jobQueue.filter((j) => j.id !== id);
        renderQueue();
      }
    } else if (openBtn) {
      const dir = openBtn.dataset.dir;
      if (dir) invoke("open_directory", { path: dir });
    } else if (dismissBtn) {
      const id = dismissBtn.dataset.id;
      if (id) {
        jobQueue = jobQueue.filter((j) => j.id !== id);
        renderQueue();
      }
    }
  });

  clearQueueBtn.addEventListener("click", () => {
    jobQueue = jobQueue.filter(
      (j) =>
        j.status === "queued" ||
        j.status === "preparing" ||
        j.status === "downloading" ||
        j.status === "processing"
    );
    renderQueue();
  });

  // Escape cancels active job if running
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && activeJobId) {
      handleCancel();
    }
  });
}

// ── Load Video ────────────────────────────────────────────────────────────────

async function handleLoad() {
  const url = urlInput.value.trim();
  if (!url) {
    showUrlError("Please enter a YouTube URL.");
    return;
  }

  // Skip reloading if already showing this video
  if (appState.phase === "video_loaded" && appState.video.url === url) {
    return;
  }

  clearUrlError();
  setLoadingState(true);

  try {
    const video = await invoke<VideoInfo>("load_video", { url });
    onVideoLoaded(video);
  } catch (e: unknown) {
    const err = e as { code?: string; message?: string };
    const msg = err?.message ?? "";
    if (
      err?.code === "CANCELLED" ||
      msg.toLowerCase().includes("cancelled") ||
      msg.toLowerCase().includes("superseded")
    ) {
      setLoadingState(false);
      return;
    }
    showUrlError(msg || "Could not load video.");
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
  const initialTimestamp = extractUrlTimestamp(video.url);
  if (initialTimestamp !== null && initialTimestamp < video.duration_seconds) {
    addStartInput.value = formatTimestamp(initialTimestamp);
    addEndInput.value = "";
    addEndInput.focus();
  } else {
    addStartInput.value = "";
    addEndInput.value = "";
  }
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
  if (
    appState.phase === "video_loaded" ||
    appState.phase === "exporting" ||
    appState.phase === "done"
  ) {
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

function handleAddClip() {
  const startRaw = addStartInput.value.trim();
  const endRaw = addEndInput.value.trim();

  // If both empty, add an empty row to the list for manual direct typing
  if (!startRaw && !endRaw) {
    addEmptyClip();
    return;
  }

  // Check if someone entered/pasted a combined range into either field
  const range = parseRange(startRaw) || parseRange(endRaw);
  if (range) {
    const dur = currentVideoDuration();
    clips.push(validateClip(clipFromRange(range), dur));
    addStartInput.value = "";
    addEndInput.value = "";
    clipError.classList.add("hidden");
    renderClips();
    updateExportButton();
    addStartInput.focus();
    return;
  }

  // Parse start (accepts bare seconds like "15" or timestamps like "0:15")
  const startSeconds = parseTimestamp(startRaw);
  if (startSeconds === null) {
    showClipError(`Invalid start time: "${startRaw}".`);
    addStartInput.focus();
    return;
  }

  // Parse end (accepts bare seconds like "42" or timestamps like "0:42")
  const endSeconds = parseTimestamp(endRaw);
  if (endSeconds === null) {
    showClipError(`Invalid end time: "${endRaw}".`);
    addEndInput.focus();
    return;
  }

  if (endSeconds <= startSeconds) {
    showClipError("End time must be after start time.");
    addEndInput.focus();
    return;
  }

  const dur = currentVideoDuration();
  if (endSeconds > dur + 1) {
    showClipError(`End time exceeds video duration (${formatTimestamp(dur)}).`);
    addEndInput.focus();
    return;
  }

  const newClip = validateClip(
    {
      id: String(Date.now() + Math.random()),
      startRaw: formatTimestamp(startSeconds),
      endRaw: formatTimestamp(endSeconds),
      startSeconds,
      endSeconds,
      error: null,
    },
    dur
  );

  clips.push(newClip);
  addStartInput.value = "";
  addEndInput.value = "";
  clipError.classList.add("hidden");
  renderClips();
  updateExportButton();
  addStartInput.focus();
}

function handleRangePaste(pastedText: string) {
  const multi = clipsFromPaste(pastedText);
  if (multi.length > 0) {
    const dur = currentVideoDuration();
    clips.push(...multi.map((c) => validateClip(c, dur)));
    addStartInput.value = "";
    addEndInput.value = "";
    clipError.classList.add("hidden");
    renderClips();
    updateExportButton();
    return true;
  }
  return false;
}

function showClipError(msg: string) {
  clipError.textContent = msg;
  clipError.classList.remove("hidden");
  setTimeout(() => clipError.classList.add("hidden"), 3500);
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

function onClipEndKeyDown(e: KeyboardEvent, _id: string) {
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
      <div class="clip-time-group">
        <input
          type="text"
          class="adw-input clip-start${clip.error && clip.startRaw ? " input--error" : ""}"
          value="${escHtml(clip.startRaw)}"
          aria-label="Start time"
        />
        <span class="clip-arrow" aria-hidden="true">
          <svg class="icon icon-sm"><use href="#icon-arrow-right"/></svg>
        </span>
        <input
          type="text"
          class="adw-input clip-end${clip.error ? " input--error" : ""}"
          value="${escHtml(clip.endRaw)}"
          aria-label="End time"
        />
      </div>
      <button class="btn-icon btn-icon--danger clip-remove" aria-label="Remove clip" title="Remove">
        <svg class="icon"><use href="#icon-trash"/></svg>
      </button>
      ${clip.error ? `<p class="clip-row-error" role="alert">${escHtml(clip.error)}</p>` : ""}
    `;

    const startInput = row.querySelector<HTMLInputElement>(".clip-start")!;
    const endInput = row.querySelector<HTMLInputElement>(".clip-end")!;
    const removeBtn = row.querySelector<HTMLButtonElement>(".clip-remove")!;

    startInput.addEventListener("input", () => onClipStartChange(clip.id, startInput.value));
    startInput.addEventListener("blur", () => {
      onClipStartChange(clip.id, startInput.value);
      const parsed = parseTimestamp(startInput.value);
      if (parsed !== null) {
        startInput.value = formatTimestamp(parsed);
      }
    });

    endInput.addEventListener("input", () => onClipEndChange(clip.id, endInput.value));
    endInput.addEventListener("blur", () => {
      onClipEndChange(clip.id, endInput.value);
      const parsed = parseTimestamp(endInput.value);
      if (parsed !== null) {
        endInput.value = formatTimestamp(parsed);
      }
    });
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

function getPendingClip(): ClipEntry | null {
  const startRaw = addStartInput.value.trim();
  const endRaw = addEndInput.value.trim();
  if (!startRaw && !endRaw) return null;

  const startSeconds = parseTimestamp(startRaw);
  const endSeconds = parseTimestamp(endRaw);
  if (startSeconds === null || endSeconds === null || endSeconds <= startSeconds) {
    return null;
  }
  const dur = currentVideoDuration();
  if (endSeconds > dur + 1) return null;

  return validateClip(
    {
      id: "clip-" + Date.now(),
      startRaw: formatTimestamp(startSeconds),
      endRaw: formatTimestamp(endSeconds),
      startSeconds,
      endSeconds,
      error: null,
    },
    dur
  );
}

function updateExportButton() {
  if (appState.phase !== "video_loaded") {
    exportBtn.disabled = true;
    exportCount.textContent = "0";
    return;
  }

  const validExistingCount = clips.filter(
    (c) => c.startSeconds !== null && c.endSeconds !== null && !c.error
  ).length;
  const pending = getPendingClip();

  if (validExistingCount > 0 && allClipsValid(clips)) {
    exportBtn.disabled = false;
    exportCount.textContent = String(validExistingCount + (pending ? 1 : 0));
  } else if (pending !== null) {
    exportBtn.disabled = false;
    exportCount.textContent = "1";
  } else {
    exportBtn.disabled = true;
    exportCount.textContent = "0";
  }
}

// ── Folder Picker ─────────────────────────────────────────────────────────────

async function handleChangeFolder() {
  const selected = await openDialog({ directory: true, defaultPath: outputDir });
  if (selected && typeof selected === "string") {
    outputDir = selected;
    renderOutputPath();
  }
}

// ── Export Queue ─────────────────────────────────────────────────────────────

function formatJobTitle(job: QueueJob): string {
  if (job.clips.length === 0) return "Clip";
  const first = job.clips[0];
  const start =
    first.startRaw || (first.startSeconds !== null ? formatTimestamp(first.startSeconds) : "00:00");
  const end =
    first.endRaw || (first.endSeconds !== null ? formatTimestamp(first.endSeconds) : "00:00");
  if (job.clips.length === 1) {
    return `${start} → ${end}`;
  }
  return `${start} → ${end} (+${job.clips.length - 1} more)`;
}

function renderQueue() {
  if (jobQueue.length === 0) {
    queueSection.classList.add("hidden");
    queueList.innerHTML = "";
    return;
  }

  queueSection.classList.remove("hidden");

  const hasFinished = jobQueue.some(
    (j) => j.status === "completed" || j.status === "error" || j.status === "cancelled"
  );
  if (hasFinished) {
    clearQueueBtn.classList.remove("hidden");
  } else {
    clearQueueBtn.classList.add("hidden");
  }

  queueList.innerHTML = jobQueue
    .map((job) => {
      const isIndeterminate = job.percent < 0;
      const pctDisplay = isIndeterminate ? "" : `${job.percent}%`;
      const fillWidth = isIndeterminate ? 100 : job.percent;
      const indeterminateClass = isIndeterminate ? "progress-fill--indeterminate" : "";
      const isCompleted = job.status === "completed";
      const isError = job.status === "error";
      const isActive =
        job.status === "preparing" || job.status === "downloading" || job.status === "processing";
      const isQueued = job.status === "queued";

      let statusBadge = escHtml(job.statusText);
      if (isCompleted) {
        statusBadge = `<span style="color: var(--color-success)">✓ ${escHtml(job.statusText)}</span>`;
      } else if (isError) {
        statusBadge = `<span style="color: var(--color-danger)">✕ ${escHtml(job.statusText)}</span>`;
      }

      let actionsHtml = "";
      if (isActive) {
        actionsHtml = `<button class="btn btn--flat btn--sm queue-cancel-btn" data-id="${job.id}" type="button">Cancel</button>`;
      } else if (isQueued) {
        actionsHtml = `<button class="btn btn--flat btn--sm queue-remove-btn" data-id="${job.id}" type="button">Remove</button>`;
      } else if (isCompleted) {
        const outDir = job.result?.output_directory || job.outputDir;
        actionsHtml = `
          <button class="btn btn--secondary btn--sm queue-open-btn" data-dir="${escHtml(outDir)}" type="button">
            <svg class="icon icon-sm"><use href="#icon-folder"/></svg>
            <span>Open</span>
          </button>
          <button class="btn-icon btn-icon--sm queue-dismiss-btn" data-id="${job.id}" title="Dismiss" type="button">
            <svg class="icon icon-sm"><use href="#icon-close"/></svg>
          </button>
        `;
      } else {
        // error or cancelled
        actionsHtml = `
          <button class="btn-icon btn-icon--sm queue-dismiss-btn" data-id="${job.id}" title="Dismiss" type="button">
            <svg class="icon icon-sm"><use href="#icon-close"/></svg>
          </button>
        `;
      }

      const cardClasses = [
        "queue-card",
        isCompleted ? "queue-card--completed" : "",
        isError ? "queue-card--error" : "",
      ]
        .filter(Boolean)
        .join(" ");

      return `
        <div class="${cardClasses}" id="queue-job-${job.id}" role="listitem">
          <div class="queue-card-header">
            <div class="queue-card-info">
              <span class="queue-card-title">${escHtml(formatJobTitle(job))}</span>
              <span class="queue-card-meta">${escHtml(job.video.title)} • ${job.quality === "highest" ? "Highest" : job.quality} • ${job.format.toUpperCase()}</span>
            </div>
            <div class="queue-card-actions">
              ${actionsHtml}
            </div>
          </div>
          <div class="queue-progress-bar">
            <div class="adw-progress-track">
              <div id="queue-fill-${job.id}" class="adw-progress-fill ${indeterminateClass}" style="width: ${fillWidth}%"></div>
            </div>
          </div>
          <div class="queue-card-footer">
            <span id="queue-status-${job.id}" class="queue-status-text">${statusBadge}</span>
            <span id="queue-pct-${job.id}" class="queue-percent-text">${pctDisplay}</span>
          </div>
        </div>
      `;
    })
    .join("");
}

function updateActiveJobUI(job: QueueJob) {
  const fill = document.getElementById(`queue-fill-${job.id}`);
  const statusEl = document.getElementById(`queue-status-${job.id}`);
  const pctEl = document.getElementById(`queue-pct-${job.id}`);

  if (fill) {
    if (job.percent < 0) {
      fill.style.width = "100%";
      fill.classList.add("progress-fill--indeterminate");
    } else {
      fill.style.width = `${job.percent}%`;
      fill.classList.remove("progress-fill--indeterminate");
    }
  }

  if (statusEl) {
    statusEl.textContent = job.statusText;
  }

  if (pctEl) {
    pctEl.textContent = job.percent >= 0 ? `${job.percent}%` : "";
  }
}

async function handleCancelJob(jobId: string) {
  if (activeJobId === jobId) {
    await invoke("cancel_export");
  } else {
    jobQueue = jobQueue.filter((j) => j.id !== jobId);
    renderQueue();
  }
}

// ── Export ────────────────────────────────────────────────────────────────────

async function handleExport() {
  if (
    appState.phase !== "video_loaded" &&
    appState.phase !== "exporting" &&
    appState.phase !== "done"
  )
    return;
  const video = appState.video;

  // Auto-commit pending clip in inputs if valid
  const pending = getPendingClip();
  const toExport: ClipEntry[] = [...clips];
  if (pending) {
    toExport.push(pending);
  }

  if (toExport.length === 0) return;

  // In-queue duration entry should be cleared immediately
  addStartInput.value = "";
  addEndInput.value = "";
  clips = [];
  renderClips();
  updateExportButton();

  // The link entry (urlInput) is NOT cleared!
  const jobId = "job-" + Date.now() + "-" + Math.random().toString(36).slice(2, 6);
  const format = formatSelect.value as "mp4" | "webm" | "mkv";
  const quality = (qualitySelect?.value || "highest") as QualityOption;

  const newJob: QueueJob = {
    id: jobId,
    video,
    clips: toExport,
    format,
    quality,
    outputDir,
    status: "queued",
    percent: -1,
    statusText: "Queued…",
    createdAt: Date.now(),
  };

  jobQueue.push(newJob);
  renderQueue();
  processQueue();
}

async function processQueue() {
  if (activeJobId !== null) return;

  const nextJob = jobQueue.find((j) => j.status === "queued");
  if (!nextJob) return;

  activeJobId = nextJob.id;
  nextJob.status = "preparing";
  nextJob.statusText = "Connecting to YouTube…";
  nextJob.percent = -1;
  renderQueue();

  const ipcClips = toIpcClips(nextJob.clips);
  const options: ExportOptions = {
    output_directory: nextJob.outputDir,
    mode: "accurate",
    format: nextJob.format,
    quality: nextJob.quality,
  };

  try {
    const result = await invoke<ExportResult>("export_clips", {
      video: nextJob.video,
      clips: ipcClips,
      options,
    });

    if (result.cancelled) {
      nextJob.status = "cancelled";
      nextJob.statusText = `Cancelled (${result.clips_completed} of ${result.clips_total} clips saved)`;
      nextJob.percent = 100;
    } else {
      nextJob.status = "completed";
      nextJob.result = result;
      nextJob.statusText = `${result.clips_completed} clip${result.clips_completed !== 1 ? "s" : ""} exported`;
      nextJob.percent = 100;
    }
  } catch (e: unknown) {
    const err = e as { code?: string; message?: string };
    const msg = err?.message ?? "";
    if (err?.code === "CANCELLED" || msg.toLowerCase().includes("cancel")) {
      nextJob.status = "cancelled";
      nextJob.statusText = "Export cancelled";
      nextJob.percent = 100;
    } else {
      nextJob.status = "error";
      nextJob.error = msg || "Export failed.";
      nextJob.statusText = nextJob.error;
      nextJob.percent = 0;
    }
  } finally {
    activeJobId = null;
    renderQueue();
    processQueue();
  }
}

async function handleCancel() {
  if (activeJobId) {
    await handleCancelJob(activeJobId);
  }
}

// ── Preview ───────────────────────────────────────────────────────────────────

function handlePreview() {
  if (
    appState.phase !== "video_loaded" &&
    appState.phase !== "exporting" &&
    appState.phase !== "done"
  )
    return;
  const video = appState.video;
  invoke("open_directory", { path: video.url });
}

// ── Progress Listener ─────────────────────────────────────────────────────────

function setupProgressListener() {
  listen("export-progress", (event) => {
    if (!activeJobId) return;
    const currentJob = jobQueue.find((j) => j.id === activeJobId);
    if (!currentJob) return;

    const payload = event.payload as Record<string, unknown>;
    const phase = payload["phase"] as string;

    switch (phase) {
      case "preparing":
        currentJob.status = "preparing";
        currentJob.statusText = "Connecting to YouTube…";
        currentJob.percent = -1;
        break;
      case "downloading": {
        currentJob.status = "downloading";
        const pct = payload["percent"] as number | null;
        if (pct !== null && pct !== undefined) {
          currentJob.percent = Math.round(pct);
          currentJob.statusText = `Downloading… ${Math.round(pct)}%`;
        } else {
          currentJob.percent = -1;
          currentJob.statusText = "Downloading…";
        }
        break;
      }
      case "processing": {
        currentJob.status = "processing";
        const ci = ((payload["clip_index"] as number) ?? 0) + 1;
        const ct = (payload["clips_total"] as number) ?? 1;
        const overall = (payload["overall_percent"] as number) ?? 0;
        currentJob.percent = Math.round(overall);
        currentJob.statusText = ct > 1 ? `Exporting clip ${ci} of ${ct}…` : `Exporting clip…`;
        break;
      }
      case "finalizing":
        currentJob.status = "processing";
        currentJob.statusText = "Finalising…";
        currentJob.percent = 99;
        break;
      case "complete":
        currentJob.status = "completed";
        currentJob.statusText = "Export completed";
        currentJob.percent = 100;
        break;
      case "cancelled":
        currentJob.status = "cancelled";
        currentJob.statusText = "Export cancelled";
        break;
    }

    updateActiveJobUI(currentJob);
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
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}
