/**
 * Timestamp parsing and formatting utilities.
 * Tested independently before connecting to UI.
 */

/**
 * Parse a human-entered timestamp string into seconds.
 *
 * Accepted formats:
 *   15        → 15.0
 *   0:15      → 15.0
 *   01:20     → 80.0
 *   1:02:15   → 3735.0
 *
 * Returns null on any parse failure.
 */
export function parseTimestamp(input: string): number | null {
  const s = input.trim();
  if (!s) return null;

  // Allow digits, colons, and a single decimal point
  if (!/^[\d:.]+$/.test(s)) return null;

  const parts = s.split(":").map((p) => parseFloat(p));
  if (parts.some((p) => isNaN(p) || !isFinite(p) || p < 0)) return null;

  let seconds: number;
  if (parts.length === 1) {
    seconds = parts[0];
  } else if (parts.length === 2) {
    // MM:SS or M:SS — minutes must be integer
    if (parts[0] !== Math.floor(parts[0])) return null;
    seconds = parts[0] * 60 + parts[1];
  } else if (parts.length === 3) {
    // H:MM:SS — hours and minutes must be integer
    if (parts[0] !== Math.floor(parts[0])) return null;
    if (parts[1] !== Math.floor(parts[1]) || parts[1] >= 60) return null;
    seconds = parts[0] * 3600 + parts[1] * 60 + parts[2];
  } else {
    return null;
  }

  if (!isFinite(seconds) || seconds < 0) return null;
  return seconds;
}

/**
 * Format seconds into a display string.
 *   < 1 hour  → MM:SS
 *   ≥ 1 hour  → H:MM:SS
 */
export function formatTimestamp(seconds: number): string {
  const total = Math.floor(seconds);
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;

  if (h > 0) {
    return `${h}:${pad(m)}:${pad(s)}`;
  }
  return `${pad(m)}:${pad(s)}`;
}

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

export interface ParsedRange {
  start: number;
  end: number;
}

/**
 * Parse a single range string like "0:15-0:42", "1:02:15 - 1:04:20",
 * "0:15 to 0:42", "0:15 – 0:42" (en-dash), "0:15 -> 0:42", "[0:15 - 0:42]".
 * Also strips annotations like "0:15 - 0:42 (Intro)".
 * Returns null if either timestamp is invalid.
 */
export function parseRange(input: string): ParsedRange | null {
  const trimmed = input.trim();
  if (!trimmed) return null;

  // Clean outer brackets/parentheses e.g. "[0:15 - 0:42]" or "(0:15 - 0:42)"
  const clean = trimmed.replace(/^[\[(]\s*/, "").replace(/\s*[\])]$/, "");

  // Match: part1 separated by range delimiter (->, -, –, —, →, .., to)
  const delimiterRegex = /\s*(?:->|[-–—→]|\.{2,}|\bto\b)\s*/i;
  const parts = clean.split(delimiterRegex);
  if (parts.length < 2) return null;

  // Extract the timestamp tokens nearest the delimiter, stripping any non-timestamp markers
  const rawP1 = parts[0].trim().split(/\s+/).pop() ?? "";
  const rawP2 = parts[1].trim().split(/\s+/)[0] ?? "";
  const p1 = rawP1.replace(/^[^\d]+|[^\d]+$/g, "");
  const p2 = rawP2.replace(/^[^\d]+|[^\d]+$/g, "");

  const start = parseTimestamp(p1);
  const end = parseTimestamp(p2);

  if (start === null || end === null) return null;
  return { start, end };
}

/**
 * Extract timestamp from YouTube URL parameters if present (e.g. ?t=85s or &t=1m25s or ?start=85).
 */
export function extractUrlTimestamp(url: string): number | null {
  try {
    const parsed = new URL(url);
    const tParam = parsed.searchParams.get("t") || parsed.searchParams.get("start");
    if (!tParam) return null;

    if (/^\d+$/.test(tParam)) {
      return parseInt(tParam, 10);
    }
    const h = tParam.match(/(\d+)h/i)?.[1] ? parseInt(tParam.match(/(\d+)h/i)![1], 10) : 0;
    const m = tParam.match(/(\d+)m/i)?.[1] ? parseInt(tParam.match(/(\d+)m/i)![1], 10) : 0;
    const s = tParam.match(/(\d+)s/i)?.[1] ? parseInt(tParam.match(/(\d+)s/i)![1], 10) : 0;
    if (h > 0 || m > 0 || s > 0) {
      return h * 3600 + m * 60 + s;
    }
    return parseTimestamp(tParam);
  } catch {
    return null;
  }
}

/**
 * Parse multiple ranges from a pasted block.
 * Supports newline-separated, semicolon-separated, or comma-separated ranges.
 * Silently skips unparseable lines.
 */
export function parseMultipleRanges(input: string): ParsedRange[] {
  const lines = input
    .split(/[\n,;]+/)
    .map((l) => l.trim())
    .filter((l) => l.length > 0);

  const results: ParsedRange[] = [];
  for (const line of lines) {
    const r = parseRange(line);
    if (r) results.push(r);
  }
  return results;
}

// ── Unit tests (run with vitest) ─────────────────────────────────────────────

if (import.meta.vitest) {
  const { describe, it, expect } = import.meta.vitest;

  describe("parseTimestamp", () => {
    it("parses bare seconds", () => expect(parseTimestamp("15")).toBe(15));
    it("parses 0:15", () => expect(parseTimestamp("0:15")).toBe(15));
    it("parses 01:20", () => expect(parseTimestamp("01:20")).toBe(80));
    it("parses 1:02:15", () => expect(parseTimestamp("1:02:15")).toBe(3735));
    it("returns null for empty", () => expect(parseTimestamp("")).toBeNull());
    it("returns null for letters", () => expect(parseTimestamp("abc")).toBeNull());
    it("returns null for negative", () => expect(parseTimestamp("-5")).toBeNull());
    it("returns null for too many parts", () => expect(parseTimestamp("1:2:3:4")).toBeNull());
    it("returns null for NaN", () => expect(parseTimestamp("1:xx")).toBeNull());
    it("handles leading zeros", () => expect(parseTimestamp("00:00:15")).toBe(15));
  });

  describe("formatTimestamp", () => {
    it("formats under 1 hour as MM:SS", () => expect(formatTimestamp(75)).toBe("01:15"));
    it("formats 0", () => expect(formatTimestamp(0)).toBe("00:00"));
    it("formats over 1 hour", () => expect(formatTimestamp(3735)).toBe("1:02:15"));
  });

  describe("parseRange", () => {
    it("parses simple range", () => {
      const r = parseRange("0:15-0:42");
      expect(r).toEqual({ start: 15, end: 42 });
    });
    it("parses with spaces", () => {
      const r = parseRange("2:10 - 2:35");
      expect(r).toEqual({ start: 130, end: 155 });
    });
    it("parses en-dash and em-dash", () => {
      expect(parseRange("0:15 – 0:42")).toEqual({ start: 15, end: 42 });
      expect(parseRange("0:15 — 0:42")).toEqual({ start: 15, end: 42 });
    });
    it("parses 'to' and arrows", () => {
      expect(parseRange("0:15 to 0:42")).toEqual({ start: 15, end: 42 });
      expect(parseRange("0:15 -> 0:42")).toEqual({ start: 15, end: 42 });
      expect(parseRange("0:15 → 0:42")).toEqual({ start: 15, end: 42 });
      expect(parseRange("0:15..0:42")).toEqual({ start: 15, end: 42 });
    });
    it("parses brackets and annotations", () => {
      expect(parseRange("[0:15 - 0:42]")).toEqual({ start: 15, end: 42 });
      expect(parseRange("0:15 - 0:42 Intro")).toEqual({ start: 15, end: 42 });
    });
    it("returns null for invalid", () => expect(parseRange("abc")).toBeNull());
  });

  describe("extractUrlTimestamp", () => {
    it("extracts bare seconds ?t=85", () => {
      expect(extractUrlTimestamp("https://youtu.be/abc?t=85")).toBe(85);
    });
    it("extracts formatted time &t=1m25s", () => {
      expect(extractUrlTimestamp("https://www.youtube.com/watch?v=abc&t=1m25s")).toBe(85);
    });
    it("extracts start parameter ?start=120", () => {
      expect(extractUrlTimestamp("https://www.youtube.com/embed/abc?start=120")).toBe(120);
    });
    it("returns null when no timestamp in url", () => {
      expect(extractUrlTimestamp("https://youtu.be/abc")).toBeNull();
    });
  });

  describe("parseMultipleRanges", () => {
    it("parses newline-separated", () => {
      const r = parseMultipleRanges("0:15-0:42\n2:10-2:35");
      expect(r).toHaveLength(2);
    });
    it("parses comma-separated", () => {
      const r = parseMultipleRanges("0:15-0:42, 2:10-2:35");
      expect(r).toHaveLength(2);
    });
    it("parses semicolon-separated", () => {
      const r = parseMultipleRanges("0:15 to 0:42; 2:10 – 2:35");
      expect(r).toHaveLength(2);
    });
    it("skips invalid lines", () => {
      const r = parseMultipleRanges("0:15-0:42\nbad line\n2:10-2:35");
      expect(r).toHaveLength(2);
    });
  });
}
