import type { DesktopBridge, DesktopSourceState } from "@t3tools/contracts";
import { expect, it, vi } from "vite-plus/test";
import { page, userEvent } from "vite-plus/test/browser";
import { render } from "../../../test/render";
import { SourceUpdatesSettings } from "./SourceUpdatesSettings";
import "~/index.css";

const dialogs = vi.hoisted(() => ({ confirm: vi.fn() }));
vi.mock("../../localApi", () => ({ ensureLocalApi: () => ({ dialogs }) }));

function setup() {
  dialogs.confirm.mockReset().mockResolvedValue(false);
  const state: DesktopSourceState = {
    supported: true,
    reason: null,
    trackedChannel: "dev",
    rollbackAvailable: true,
    installed: { channel: "dev", commit: "0123456789abcdef", version: "0.4.0-preview.20261115.1" },
    operation: null,
  };
  const getState = vi.fn().mockImplementation(async () => state);
  const run = vi.fn().mockImplementation(async () => ({
    ...state,
    operation: {
      action: "update",
      channel: "main",
      pid: 1,
      status: "running",
      message: "Building main…",
      logPath: "/tmp/test.log",
      targetCommit: null,
    },
  }));
  return {
    getState,
    run,
    state,
    bridge: { sourceUpdates: { getState, run } } as unknown as DesktopBridge,
  };
}

it("keeps the followed channel until a confirmed update starts and prevents repeat actions", async () => {
  const t = setup();
  vi.stubGlobal("desktopBridge", t.bridge);
  using _bridge = { [Symbol.dispose]: () => vi.unstubAllGlobals() };
  await using screen = await render(<SourceUpdatesSettings />);
  await expect.element(screen.getByRole("button", { name: "Update & Restart" })).toBeEnabled();
  await userEvent.click(screen.getByRole("combobox", { name: "Follow channel" }));
  await userEvent.click(page.getByRole("option", { name: "Stable (main)" }));
  await expect.element(screen.getByText(/Currently following Development/)).toBeVisible();
  await userEvent.click(screen.getByRole("button", { name: "Update & Restart" }));
  expect(t.run).not.toHaveBeenCalled();
  dialogs.confirm.mockResolvedValueOnce(true);
  await userEvent.click(screen.getByRole("button", { name: "Update & Restart" }));
  await expect.element(screen.getByText("Building main…")).toBeVisible();
  expect(t.run).toHaveBeenCalledExactlyOnceWith({ action: "update", channel: "main" });
  await expect.element(screen.getByRole("button", { name: "Working…" })).toBeDisabled();
  await expect.element(screen.getByRole("button", { name: "Roll Back" })).toBeDisabled();
});
