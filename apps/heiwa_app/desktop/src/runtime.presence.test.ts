import { describe, expect, it } from "vitest";
import { providerPresence, type ProviderSnapshot } from "./runtime";

const provider = (overrides: Partial<ProviderSnapshot>): ProviderSnapshot => ({
  provider_id: "gemini",
  display_name: "Gemini",
  status: "connected",
  auth_kind: "oauth_cli",
  default_model: null,
  supported_lanes: [],
  last_error: null,
  last_validated_at: null,
  ...overrides,
});

describe("providerPresence", () => {
  it("does not count sign-in presence as a recorded run", () => {
    expect(providerPresence([provider({})])).toEqual({ connected: 1, withRuns: 0 });
  });

  it("counts only connected providers whose own channel recorded a success", () => {
    expect(
      providerPresence([
        provider({ last_execution_success_at: "2026-10-08T10:00:00Z", execution_evidence: "complete" }),
        provider({ provider_id: "codex", last_execution_failure_at: "2026-10-08T10:01:00Z" }),
        provider({ provider_id: "claude", status: "degraded", last_execution_success_at: "2026-10-08T10:02:00Z" }),
      ]),
    ).toEqual({ connected: 2, withRuns: 1 });
  });

  it("treats rows from an older runtime without execution fields as unobserved", () => {
    const legacy = provider({});
    delete (legacy as Partial<ProviderSnapshot>).execution_evidence;
    expect(providerPresence([legacy]).withRuns).toBe(0);
  });
});
