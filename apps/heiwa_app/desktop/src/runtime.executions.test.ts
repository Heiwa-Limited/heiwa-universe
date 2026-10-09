import { describe, expect, it } from "vitest";
import { observedAge } from "./lib/format";
import { executionFacts, executionLine, type ProviderSnapshot, type RuntimeHealth } from "./runtime";

const now = new Date("2026-10-08T12:00:00Z");

const row = (overrides: Partial<ProviderSnapshot>): ProviderSnapshot => ({
  provider_id: "openai",
  display_name: "OpenAI",
  status: "connected",
  auth_kind: "api_key",
  default_model: null,
  supported_lanes: [],
  last_error: null,
  last_validated_at: null,
  ...overrides,
});

const health = (providers: ProviderSnapshot[]): RuntimeHealth => ({
  reachable: true,
  snapshot: { data: { providers } },
});

const api = (accountId: string, extra: Partial<ProviderSnapshot> = {}) =>
  row({
    account_id: accountId,
    execution_evidence: "complete",
    execution_channel: { kind: "api_key", account_id: accountId, binary: null },
    last_execution_success_at: null,
    last_execution_failure_at: null,
    last_execution_failure_class: null,
    ...extra,
  });

const cli = (binary: string, extra: Partial<ProviderSnapshot> = {}) =>
  row({
    provider_id: binary,
    auth_kind: "oauth_cli",
    execution_evidence: "complete",
    execution_channel: { kind: "oauth_cli", account_id: null, binary },
    ...extra,
  });

describe("executionFacts", () => {
  it("reports dated success and failure for the exact account only", () => {
    const snapshot = health([
      api("openai-api-a", {
        last_execution_success_at: "2026-10-08T09:00:00Z",
        last_execution_failure_at: "2026-10-08T11:30:00Z",
        last_execution_failure_class: "rate_limited",
      }),
      api("openai-api-b"),
    ]);
    const facts = executionFacts(snapshot, { accountId: "openai-api-a" }, now);
    expect(facts).toEqual({
      state: "recorded",
      partial: false,
      success: { at: "2026-10-08T09:00:00Z", age: "3h ago" },
      failure: { at: "2026-10-08T11:30:00Z", age: "30m ago", class: "rate_limited" },
    });
    expect(executionFacts(snapshot, { accountId: "openai-api-b" }, now)).toEqual({ state: "none" });
  });

  it("shows a registered CLI or local account's own facts by its row account id", () => {
    const snapshot = health([
      row({
        provider_id: "anthropic",
        auth_kind: "oauth_cli",
        account_id: "anthropic-cli",
        execution_evidence: "complete",
        // CLI and local provenance carry no account id; the row does.
        execution_channel: { kind: "oauth_cli", account_id: null, binary: "claude" },
        last_execution_success_at: "2026-10-08T11:00:00Z",
      }),
      row({
        provider_id: "ollama",
        auth_kind: "local_runtime",
        account_id: "ollama-local",
        execution_evidence: "complete",
        execution_channel: { kind: "local_runtime", account_id: null, binary: "ollama" },
        last_execution_failure_at: "2026-10-08T10:00:00Z",
        last_execution_failure_class: "availability",
      }),
      api("anthropic-api-neighbor"),
    ]);
    const cliFacts = executionFacts(snapshot, { accountId: "anthropic-cli" }, now);
    expect(cliFacts).toEqual({ state: "recorded", partial: false, success: { at: "2026-10-08T11:00:00Z", age: "1h ago" } });
    const localFacts = executionFacts(snapshot, { accountId: "ollama-local" }, now);
    expect(localFacts.state).toBe("recorded");
    if (localFacts.state === "recorded") expect(localFacts.failure?.class).toBe("availability");
    // A neighbouring API account of the same vendor inherits nothing.
    expect(executionFacts(snapshot, { accountId: "anthropic-api-neighbor" }, now)).toEqual({ state: "none" });
  });

  it("never borrows another account when a row lacks its own identity", () => {
    const legacyCli = row({
      provider_id: "claude",
      auth_kind: "oauth_cli",
      execution_evidence: "complete",
      execution_channel: { kind: "oauth_cli", account_id: null, binary: "claude" },
      last_execution_success_at: "2026-10-08T11:00:00Z",
    });
    // An older runtime row without `account_id` matches only by its API channel.
    const olderApi = row({
      execution_evidence: "complete",
      execution_channel: { kind: "api_key", account_id: "openai-api-old", binary: null },
      last_execution_success_at: "2026-10-08T09:00:00Z",
    });
    const snapshot = health([legacyCli, olderApi]);
    expect(executionFacts(snapshot, { accountId: "anthropic-cli" }, now)).toEqual({ state: "not_tracked" });
    expect(executionFacts(snapshot, { accountId: "openai-api-old" }, now).state).toBe("recorded");
    expect(executionFacts(snapshot, { accountId: "openai-api-other" }, now)).toEqual({ state: "not_tracked" });
  });

  it("matches a CLI tool by binary and ignores same-vendor API rows", () => {
    const snapshot = health([
      api("google-api-1", { last_execution_success_at: "2026-10-08T11:00:00Z" }),
      cli("gemini", {
        last_execution_failure_at: "2026-10-07T12:00:00Z",
        last_execution_failure_class: "quota_exhausted",
      }),
    ]);
    const facts = executionFacts(snapshot, { binary: "gemini" }, now);
    expect(facts.state).toBe("recorded");
    if (facts.state === "recorded") {
      expect(facts.success).toBeUndefined();
      expect(facts.failure?.class).toBe("quota_exhausted");
      expect(facts.failure?.age).toBe("24h ago");
    }
    expect(executionFacts(snapshot, { binary: "agy" }, now)).toEqual({ state: "not_tracked" });
  });

  it("distinguishes unreachable, older, unavailable, empty and partial evidence", () => {
    expect(executionFacts(null, { binary: "codex" }, now)).toEqual({ state: "runtime_unreachable" });
    expect(executionFacts({ reachable: false }, { binary: "codex" }, now)).toEqual({ state: "runtime_unreachable" });
    const older = row({ provider_id: "codex", auth_kind: "oauth_cli" });
    expect(executionFacts(health([older]), { binary: "codex" }, now)).toEqual({ state: "not_reported" });
    expect(executionFacts(health([cli("codex", { execution_evidence: "unavailable" })]), { binary: "codex" }, now)).toEqual({ state: "unavailable" });
    expect(executionFacts(health([cli("codex", { execution_evidence: "empty" })]), { binary: "codex" }, now)).toEqual({ state: "none" });
    expect(executionFacts(health([cli("codex", { execution_evidence: "partial" })]), { binary: "codex" }, now)).toEqual({ state: "none_in_window" });
  });

  it("renders honest lines and never claims readiness", () => {
    expect(executionLine({ state: "none_in_window" })).toBe("No run in the inspected history");
    expect(executionLine({ state: "runtime_unreachable" })).toMatch(/runtime not reachable/);
    expect(executionLine({ state: "not_reported" })).toMatch(/not reported/);
    expect(executionLine({ state: "not_tracked" })).toBeNull();
    const line = executionLine({
      state: "recorded",
      partial: true,
      success: { at: "2026-10-08T09:00:00Z", age: "3h ago" },
    });
    expect(line).toBe("Last run succeeded 2026-10-08 09:00 UTC (3h ago) · history partial");
    expect(line).not.toMatch(/ready/i);
  });
});

describe("observedAge", () => {
  it("shows unreadable and future times instead of guessing", () => {
    expect(observedAge("2026-99-99T99:99:99Z", now)).toBe("time unreadable");
    expect(observedAge("2026-10-09T12:00:00Z", now)).toBe("in the future");
    expect(observedAge("2026-10-08T11:59:30Z", now)).toBe("just now");
    expect(observedAge("2026-10-01T12:00:00Z", now)).toBe("7d ago");
  });
});
