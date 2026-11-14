import { describe, expect, it } from "vite-plus/test";

import { proposalWhen } from "./approvals.logic";

const fixed = { locale: "en-US", timeZone: "America/Toronto", hour12: true };
// Intl separates times with thin and narrow no-break spaces.
const when = (action: Parameters<typeof proposalWhen>[0]) =>
  proposalWhen(action, fixed)?.replace(/[\u2009\u202f]/g, " ") ?? null;

describe("proposalWhen", () => {
  it("shows a timed event in the viewer's zone and an all-day event without its exclusive end", () => {
    expect(when({ start: "2026-11-20T00:00:00Z", end: "2026-11-20T02:00:00Z" })).toBe(
      "Thu, Nov 19, 2026, 7:00 – 9:00 PM",
    );
    expect(
      when({ start: "2026-11-19T00:00:00-05:00", end: "2026-11-20T00:00:00-05:00", allDay: true }),
    ).toBe("Thu, Nov 19, 2026");
    expect(when({})).toBeNull();
  });
});
