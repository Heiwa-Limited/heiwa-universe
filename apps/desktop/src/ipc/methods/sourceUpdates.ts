import { DesktopSourceRequest, DesktopSourceState } from "@t3tools/contracts";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";
import * as Option from "effect/Option";
import { ElectronWindow } from "../../electron/ElectronWindow.ts";
import type { DesktopIpcInvokeEvent } from "../DesktopIpc.ts";
import { DesktopSourceUpdates, SourceUpdateError } from "../../updates/DesktopSourceUpdates.ts";
import { makeIpcMethod } from "../DesktopIpc.ts";
import * as Channels from "../channels.ts";

const requireAppWindow = Effect.fn("desktop.ipc.sourceUpdates.requireAppWindow")(function* (
  event?: DesktopIpcInvokeEvent,
) {
  const main = yield* (yield* ElectronWindow).main;
  if (
    Option.isNone(main) ||
    event?.sender.id !== main.value.webContents.id ||
    event.senderFrame !== main.value.webContents.mainFrame
  ) {
    return yield* new SourceUpdateError({
      message: "App updates are only available from Heiwa's main window.",
    });
  }
});

export const getSourceUpdateState = makeIpcMethod({
  channel: Channels.SOURCE_UPDATE_STATE_CHANNEL,
  payload: Schema.Void,
  result: DesktopSourceState,
  handler: Effect.fn("desktop.ipc.sourceUpdates.state")(function* (_, event) {
    yield* requireAppWindow(event);
    return yield* (yield* DesktopSourceUpdates).state;
  }),
});
export const runSourceUpdate = makeIpcMethod({
  channel: Channels.SOURCE_UPDATE_RUN_CHANNEL,
  payload: DesktopSourceRequest,
  result: DesktopSourceState,
  handler: Effect.fn("desktop.ipc.sourceUpdates.run")(function* (request, event) {
    yield* requireAppWindow(event);
    return yield* (yield* DesktopSourceUpdates).run(request);
  }),
});
