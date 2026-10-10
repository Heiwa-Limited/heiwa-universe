import { describe, expect, it } from "vitest";
import { RuntimeActionError, runtimeErrorMessage } from "./runtime";

describe("safe runtime error messages", () => {
  const fallback = "Calendar choices could not be saved. Refresh the calendar list and retry.";
  const privateDetail = "private account, token and /private/helper-path";

  it("explains native offline errors without displaying native details", () => {
    expect(runtimeErrorMessage({ kind: "Offline", detail: privateDetail }, fallback))
      .toBe("The Heiwa runtime is unavailable. Check the connection and try again.");
  });

  it.each([
    { kind: "AuthNotConfigured" },
    { kind: "Http", detail: { status: 401, body: privateDetail } },
  ])("explains authentication rejection without revealing response bodies: %j", (cause) => {
    expect(runtimeErrorMessage(cause, fallback))
      .toBe("Heiwa could not authenticate with its local runtime. Check the connection and try again.");
  });

  it("distinguishes an authorization denial from missing authentication", () => {
    expect(runtimeErrorMessage({ kind: "Http", detail: { status: 403, body: privateDetail } }, fallback))
      .toBe("The Heiwa runtime denied access to this action. Check the connection and try again.");
  });

  it.each([
    { kind: "Http", detail: { status: 400, body: privateDetail } },
    { kind: "Http", detail: { status: 500, body: privateDetail } },
    { kind: "Decode", detail: privateDetail },
    { kind: "InvalidPath", detail: privateDetail },
    { kind: "Http", detail: null },
    { kind: "Http", detail: { status: "401", body: privateDetail } },
    { message: privateDetail },
    new Error(privateDetail),
    privateDetail,
    null,
    undefined,
  ])("uses the action fallback for other failures: %j", (cause) => {
    expect(runtimeErrorMessage(cause, fallback)).toBe(fallback);
  });

  it("retains only explicitly safe local validation instructions", () => {
    const message = "Refresh the calendar list before saving choices.";
    expect(runtimeErrorMessage(new RuntimeActionError(message), fallback)).toBe(message);
  });
});
