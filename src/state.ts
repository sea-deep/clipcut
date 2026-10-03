import {
  parseMultipleRanges,
  parseTimestamp,
  formatTimestamp,
  type ParsedRange,
} from "./timestamps";

// Re-export domain types that mirror the Rust structs
export interface VideoInfo {
  id: string;
  url: string;
  title: string;
  duration_seconds: number;
  thumbnail_url: string;
}

export interface Clip {
  start_seconds: number;
  end_seconds: number;
}

export type QualityOption = "highest" | "1080p" | "720p" | "480p" | "360p";

export interface ExportOptions {
  output_directory: string;
  mode: "accurate" | "fast";
  format: "mp4" | "webm" | "mkv";
  quality?: QualityOption;
}

export interface ExportResult {
  files: string[];
  output_directory: string;
  cancelled: boolean;
  clips_completed: number;
  clips_total: number;
}

export type AppState =
  | { phase: "empty" }
  | { phase: "loading_metadata" }
  | { phase: "video_loaded"; video: VideoInfo }
  | { phase: "exporting"; video: VideoInfo }
  | { phase: "done"; video: VideoInfo; result: ExportResult };

export interface ClipEntry {
  id: string;
  startRaw: string;
  endRaw: string;
  startSeconds: number | null;
  endSeconds: number | null;
  error: string | null;
}

export interface AppStore {
  state: AppState;
  clips: ClipEntry[];
  outputDir: string;
  format: "mp4" | "webm" | "mkv";
  quality: QualityOption;
  mode: "accurate" | "fast";
  urlInput: string;
}

let _nextId = 0;
function nextId(): string {
  return String(_nextId++);
}

/** Create a new empty ClipEntry */
export function emptyClip(): ClipEntry {
  return {
    id: nextId(),
    startRaw: "",
    endRaw: "",
    startSeconds: null,
    endSeconds: null,
    error: null,
  };
}

/** Parse and validate a ClipEntry given the video duration */
export function validateClip(clip: ClipEntry, videoDuration: number): ClipEntry {
  const start = parseTimestamp(clip.startRaw);
  const end = parseTimestamp(clip.endRaw);

  if (start === null && clip.startRaw.trim()) {
    return {
      ...clip,
      startSeconds: null,
      endSeconds: null,
      error: `Invalid start time: "${clip.startRaw}"`,
    };
  }
  if (end === null && clip.endRaw.trim()) {
    return {
      ...clip,
      startSeconds: start,
      endSeconds: null,
      error: `Invalid end time: "${clip.endRaw}"`,
    };
  }
  if (start === null || end === null) {
    return { ...clip, startSeconds: start, endSeconds: end, error: null };
  }
  if (end <= start) {
    return {
      ...clip,
      startSeconds: start,
      endSeconds: end,
      error: "End time must be after start time.",
    };
  }
  if (end > videoDuration + 1) {
    return {
      ...clip,
      startSeconds: start,
      endSeconds: end,
      error: `End time ${formatTimestamp(end)} exceeds video length (${formatTimestamp(videoDuration)}).`,
    };
  }
  return { ...clip, startSeconds: start, endSeconds: end, error: null };
}

/** Check that all clips are fully valid (both times set, no errors) */
export function allClipsValid(clips: ClipEntry[]): boolean {
  return (
    clips.length > 0 &&
    clips.every((c) => c.startSeconds !== null && c.endSeconds !== null && c.error === null)
  );
}

/** Convert ClipEntry[] into Clip[] for IPC */
export function toIpcClips(clips: ClipEntry[]): Clip[] {
  return clips
    .filter((c) => c.startSeconds !== null && c.endSeconds !== null && c.error === null)
    .map((c) => ({
      start_seconds: c.startSeconds!,
      end_seconds: c.endSeconds!,
    }));
}

/** Create ClipEntry from a parsed range */
export function clipFromRange(range: ParsedRange): ClipEntry {
  return {
    id: nextId(),
    startRaw: formatTimestamp(range.start),
    endRaw: formatTimestamp(range.end),
    startSeconds: range.start,
    endSeconds: range.end,
    error: null,
  };
}

/** Parse pasted text into ClipEntry array. Returns entries for each valid range. */
export function clipsFromPaste(text: string): ClipEntry[] {
  const ranges = parseMultipleRanges(text);
  return ranges.map(clipFromRange);
}
