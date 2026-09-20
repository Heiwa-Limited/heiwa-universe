// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { ProviderConnections } from "./ProviderConnections";
import type { ProviderConnection } from "../state/types";

afterEach(cleanup);

it("submits the selected provider once, clears the credential, and never verifies on mount", async () => {
  let finish!: () => void;
  const connect = vi.fn(() => new Promise<void>((resolve) => { finish = resolve; }));
  const verify = vi.fn();
  render(() => <ProviderConnections connections={[]} onConnectApiProvider={connect} onVerifyApiProvider={verify} />);
  expect(verify).not.toHaveBeenCalled();
  fireEvent.click(screen.getByText("Add an API connection"));
  fireEvent.change(screen.getByLabelText("Provider"), { target: { value: "anthropic" } });
  const field = screen.getByLabelText<HTMLInputElement>("API key");
  fireEvent.input(field, { target: { value: "test-credential" } });
  fireEvent.click(screen.getByRole("button", { name: "Connect account" }));
  expect(connect).toHaveBeenCalledExactlyOnceWith("anthropic", "test-credential");
  expect(field.value).toBe("");
  expect(field.disabled).toBe(true);
  finish();
  await waitFor(() => expect(screen.getByRole("status").textContent).toContain("Connection saved"));
});

it("does not reflect a provider error containing credentials into the UI", async () => {
  render(() => <ProviderConnections connections={[]} onConnectApiProvider={vi.fn().mockRejectedValue(new Error("secret-in-error"))} />);
  fireEvent.click(screen.getByText("Add an API connection"));
  fireEvent.input(screen.getByLabelText("API key"), { target: { value: "secret-in-error" } });
  fireEvent.click(screen.getByRole("button", { name: "Connect account" }));
  await waitFor(() => expect(screen.getByRole("alert")).toBeTruthy());
  expect(document.body.textContent).not.toContain("secret-in-error");
  expect(screen.getByLabelText<HTMLInputElement>("API key").value).toBe("");
});

it("disconnects the explicit account and leaves provider-owned CLI sign-in untouched", async () => {
  const disconnect = vi.fn().mockResolvedValue(undefined);
  render(() => <ProviderConnections connections={[
    { account_id: "api-seat", provider: "openai", channel: "API key", status: "connected", model_count: 3, can_manage_key: true },
    { account_id: "cli-seat", provider: "openai", channel: "Provider CLI", status: "disconnected", model_count: 0, can_manage_key: false },
  ]} onDisconnectApiProvider={disconnect} />);
  expect(screen.getAllByRole("button", { name: "Disconnect OpenAI connection" })).toHaveLength(1);
  fireEvent.click(screen.getByRole("button", { name: "Disconnect OpenAI connection" }));
  await waitFor(() => expect(disconnect).toHaveBeenCalledExactlyOnceWith("api-seat"));
});

// `heiwa doctor` and this panel describe the SAME account, read from the same
// AccountStatus. They must not use different words for it. The CLI says
// "not linked" and "ready"; showing "Disconnected" here would put the product
// in two minds about one fact, and would reintroduce the collision with the
// doctor's "CLI Discovery" section, which uses "connected" to mean something
// else entirely (auth present for the provider's own CLI).
const connection = (status: ProviderConnection["status"]): ProviderConnection => ({
  account_id: "anthropic-cli",
  provider: "anthropic",
  channel: "Provider CLI",
  status,
  model_count: 0,
  can_manage_key: false,
});

/** The row renders "<channel> · <status> · <n> models" in one span. */
const rowText = (status: ProviderConnection["status"]) => {
  cleanup();
  render(() => <ProviderConnections connections={[connection(status)]} />);
  return screen.getByRole("listitem").textContent ?? "";
};

it("uses the same words as `heiwa doctor` for an unlinked account", () => {
  const text = rowText("disconnected");
  expect(text).toContain("Not linked");
  expect(text).not.toContain("Disconnected");
});

it("uses the same words as `heiwa doctor` for a usable account", () => {
  expect(rowText("connected")).toContain("Ready");
});

it("never labels an account with a word the doctor gives a different meaning", () => {
  for (const status of ["connected", "disconnected", "needs_verification", "verification_failed"] as const) {
    // "Connected"/"Disconnected" belong to CLI Discovery, which answers a
    // different question. Reusing them here is what made one install read as
    // broken when it was merely unlinked.
    const text = rowText(status);
    expect(text).not.toMatch(/\bDisconnected\b/);
    expect(text).not.toMatch(/·\s*Connected\s*·/);
  }
});
