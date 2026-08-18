import { describe, expect, it } from "vitest";
import { localIsoDate } from "./format";

describe("localIsoDate", () => {
  it("answers the machine's calendar day, not the UTC one", () => {
    // 23:30 local. Whatever the machine's offset is, the day the user is
    // having is the 18th — `toISOString()` would say the 19th anywhere west
    // of UTC and drop that evening's events from Today.
    expect(localIsoDate(new Date(2026, 7, 18, 23, 30))).toBe("2026-08-18");
  });

  it("pads month and day so the string sorts and compares", () => {
    // Event dates are compared as strings, so a single-digit month has to
    // arrive as `01` or every comparison against it is wrong.
    expect(localIsoDate(new Date(2026, 0, 5, 12, 0))).toBe("2026-01-05");
  });
});
