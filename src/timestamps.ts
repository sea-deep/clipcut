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
 * Parse a single range string like "0:15-0:42" or "1:02:15 - 1:04:20".
 * Returns null if either timestamp is invalid.
 */
export function parseRange(input: string): ParsedRange | null {
  // Split on "-" but not inside a timestamp (which also uses digits and colons)
  // Strategy: find a "-" not at position 0 that is surrounded by non-colon chars
  const match = input.match(/^([^-]+?)\s*-\s*(.+)$/);
  if (!match) return null;

  const start = parseTimestamp(match[1]);
  const end = parseTimestamp(match[2]);

  if (start === null || end === null) return null;
  return { start, end };
}

/**
 * Parse multiple ranges from a pasted block.
 * Supports newline-separated or comma-separated ranges.
 * Silently skips unparseable lines.
 */
export function parseMultipleRanges(input: string): ParsedRange[] {
  // Split on newlines or commas
  const lines = input
    .split(/[\n,]+/)
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
    it("returns null for invalid", () => expect(parseRange("abc")).toBeNull());
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
    it("skips invalid lines", () => {
      const r = parseMultipleRanges("0:15-0:42\nbad line\n2:10-2:35");
      expect(r).toHaveLength(2);
    });
  });
}
