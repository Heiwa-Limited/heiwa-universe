import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { shortDate } from "./format";

// West of UTC is where a date-only string parsed as UTC midnight lands on the
// previous local day. CI runs in UTC, where that bug is invisible, so the zone
// is pinned rather than inherited.
beforeAll(() => {
  vi.stubEnv("TZ", "America/Vancouver");
});
afterAll(() => {
  vi.unstubAllEnvs();
});

const localLabel = (year: number, monthIndex: number, day: number) =>
  new Date(year, monthIndex, day).toLocaleDateString([], { month: "short", day: "numeric" });

describe("shortDate", () => {
  it("keeps a calendar day on that day west of UTC", () => {
    expect(shortDate("2026-09-14")).toBe(localLabel(2026, 8, 14));
  });

  it("shows an instant on the day it falls in this machine's zone", () => {
    // 01:00 UTC on the 15th is the evening of the 14th in Vancouver.
    expect(shortDate("2026-09-15T01:00:00Z")).toBe(localLabel(2026, 8, 14));
  });

  it("passes through text it cannot read as a date", () => {
    expect(shortDate("someday")).toBe("someday");
    expect(shortDate("")).toBe("");
  });
});
