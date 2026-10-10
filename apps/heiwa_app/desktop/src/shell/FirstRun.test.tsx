// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { FirstRun } from "./FirstRun";
import type { OnboardingState } from "../state/types";

afterEach(cleanup);
const localOnly = (): OnboardingState => ({
  complete: false, display_name: "Ada",
  gaps: [{ step: "provider", detail: "No connected provider", remedy: "Connect an account" }],
  workspace: { can_enter: true, setup_complete: false, resources: [
    { id: "anthropic", name: "Claude", category: "inference", app_detected: true, tools_detected: [], registered_accounts: 0, detail: "Detection does not grant image generation access.", surface: null, has_guide: true },
    { id: "calendar", name: "Apple Calendar", category: "apple", app_detected: true, tools_detected: [], registered_accounts: 0, detail: "Choose calendars in setup.", surface: "calendar", has_guide: false },
  ] },
});

describe("per-user workspace setup", () => {
  it("permits local entry without implying provider readiness", async () => {
    const finish = vi.fn();
    render(() => <FirstRun state={localOnly()} onEstablishIdentity={() => {}} onRecheck={() => {}} onCompleteWorkspace={finish} />);
    fireEvent.click(screen.getByText("Enter workspace"));
    await waitFor(() => expect(finish).toHaveBeenCalledOnce());
    expect(screen.getByText(/0 registered accounts/)).toBeTruthy();
    expect(screen.queryByText("Connected")).toBeNull();
  });

  it("keeps a failed save visible and permits retry", async () => {
    const finish = vi.fn().mockRejectedValueOnce(new Error("Disk is full")).mockResolvedValue(undefined);
    render(() => <FirstRun state={localOnly()} onEstablishIdentity={() => {}} onRecheck={() => {}} onCompleteWorkspace={finish} />);
    fireEvent.click(screen.getByText("Enter workspace"));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("Disk is full"));
    fireEvent.click(screen.getByText("Enter workspace"));
    await waitFor(() => expect(finish).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("does not allow repeated verification while a probe is pending", async () => {
    let resolve!: () => void;
    const verify = vi.fn(() => new Promise<void>((done) => { resolve = done; }));
    render(() => <FirstRun state={localOnly()} onEstablishIdentity={() => {}} onRecheck={() => {}} onVerifyProviders={verify} />);
    fireEvent.click(screen.getByText("Verify providers"));
    expect((screen.getByText("Check again") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByText("Verify providers"));
    expect(verify).toHaveBeenCalledOnce();
    resolve();
    await waitFor(() => expect((screen.getByText("Check again") as HTMLButtonElement).disabled).toBe(false));
  });

  it("requires identity before entry and opens the real Calendar setup", async () => {
    const initial = localOnly();
    initial.workspace!.can_enter = false;
    initial.display_name = null;
    initial.gaps.unshift({ step: "identity", detail: "Name needed", remedy: "Choose a name" });
    const [state, setState] = createSignal(initial);
    const open = vi.fn();
    const establish = vi.fn(async () => { setState(localOnly()); });
    render(() => <FirstRun state={state()} onEstablishIdentity={establish} onRecheck={() => {}} onCompleteWorkspace={() => {}} onOpenSurface={open} />);
    expect((screen.getByText("Enter workspace") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.input(screen.getByLabelText("What should Heiwa call you?"), { target: { value: " Ada " } });
    fireEvent.submit(screen.getByLabelText("What should Heiwa call you?").closest("form")!);
    await waitFor(() => expect(establish).toHaveBeenCalledWith("Ada"));
    fireEvent.click(screen.getByText("Open Calendar setup"));
    await waitFor(() => expect(open).toHaveBeenCalledWith("calendar"));
  });
});

it("names the Reminders destination and opens it without implying enrollment", async () => {
  const state = localOnly();
  state.workspace!.resources.push({ id: "reminders", name: "Apple Reminders", category: "apple", app_detected: true, tools_detected: [], registered_accounts: 0, detail: "Connect lists separately.", surface: "reminders", has_guide: false });
  const open = vi.fn();
  render(() => <FirstRun state={state} onEstablishIdentity={() => {}} onRecheck={() => {}} onOpenSurface={open} />);
  fireEvent.click(screen.getByRole("button", { name: "Open Reminders" }));
  await waitFor(() => expect(open).toHaveBeenCalledWith("reminders"));
  expect(screen.queryByText("Connected")).toBeNull();
});

describe("inference tool cards", () => {
  const now = new Date("2026-10-08T12:00:00Z");
  const withTools = (): OnboardingState => ({
    complete: true, display_name: "Ada", gaps: [],
    workspace: { can_enter: true, setup_complete: true, resources: [
      { id: "google", name: "Google / Antigravity", category: "inference", app_detected: false, tools_detected: ["gemini"], registered_accounts: 0, detail: "Detection proves nothing.", surface: null, has_guide: false },
      { id: "openai", name: "ChatGPT / OpenAI", category: "inference", app_detected: true, tools_detected: [], registered_accounts: 0, detail: "Detection proves nothing.", surface: null, has_guide: false },
    ] },
  });

  it("shows the CLI channel's dated failure only on the detected tool", () => {
    const health = {
      reachable: true,
      snapshot: { data: { providers: [{
        provider_id: "gemini", display_name: "Gemini", status: "connected", auth_kind: "oauth_cli",
        default_model: null, supported_lanes: [], last_error: null, last_validated_at: null,
        execution_evidence: "partial" as const,
        execution_channel: { kind: "oauth_cli", account_id: null, binary: "gemini" },
        last_execution_failure_at: "2026-10-07T12:00:00Z",
        last_execution_failure_class: "quota_exhausted",
      }] } },
    };
    render(() => <FirstRun state={withTools()} onEstablishIdentity={() => {}} onRecheck={() => {}}
      executions={{ health, now }} />);
    const line = screen.getByText(/gemini: Last run failed: quota_exhausted/);
    expect(line.textContent).toContain("(24h ago)");
    expect(line.textContent).toContain("history partial");
    expect(screen.queryByText(/codex:/)).toBeNull();
  });
});
