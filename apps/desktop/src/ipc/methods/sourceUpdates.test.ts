import { expect, it } from "@effect/vitest";
import type { DesktopSourceState } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import type * as Electron from "electron";
import { ElectronWindow } from "../../electron/ElectronWindow.ts";
import { DesktopSourceUpdates } from "../../updates/DesktopSourceUpdates.ts";
import { runSourceUpdate } from "./sourceUpdates.ts";

it.effect("only the app's main frame can request a bounded source update", () =>
  Effect.gen(function* () {
    const mainFrame = {};
    const window = { webContents: { id: 12, mainFrame } } as Electron.BrowserWindow;
    const states: DesktopSourceState[] = [];
    const state: DesktopSourceState = {
      supported: true,
      reason: null,
      trackedChannel: "dev",
      installed: null,
      rollbackAvailable: false,
      operation: null,
    };
    const invoke = (request: unknown, sender = { sender: { id: 12 }, senderFrame: mainFrame }) =>
      runSourceUpdate.handler(request, sender).pipe(
        Effect.provideService(ElectronWindow, {
          main: Effect.succeedSome(window),
        } as ElectronWindow["Service"]),
        Effect.provideService(DesktopSourceUpdates, {
          state: Effect.succeed(state),
          run: () =>
            Effect.sync(() => {
              states.push(state);
              return state;
            }),
        }),
        Effect.result,
      );
    const request = { action: "update", channel: "dev" };
    expect((yield* invoke(request, { sender: { id: 99 }, senderFrame: mainFrame }))._tag).toBe(
      "Failure",
    );
    expect((yield* invoke(request, { sender: { id: 12 }, senderFrame: {} }))._tag).toBe("Failure");
    expect((yield* invoke({ action: "update", channel: "dev; curl evil" }))._tag).toBe("Failure");
    expect((yield* invoke({ action: "shell", channel: "dev" }))._tag).toBe("Failure");
    expect(states).toEqual([]);
    expect((yield* invoke(request))._tag).toBe("Success");
    expect(states).toHaveLength(1);
  }),
);
