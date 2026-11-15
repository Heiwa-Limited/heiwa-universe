import { assert, describe } from "@effect/vitest";

import { createOxlintRuleHarness } from "../test/utils.ts";

const harness = (filename: string) =>
  createOxlintRuleHarness("t3code/no-host-imports-in-clients", { filename });
const web = harness("apps/web/src/component.ts");

describe("t3code/no-host-imports-in-clients", () => {
  web.valid(
    "allows portable imports, local modules and erased Node types",
    `
    import { Schema } from "effect";
    import { value } from "./local";
    import { passkeys } from "@clerk/electron/passkeys";
    import type { Stats } from "node:fs";
    import { type Buffer } from "node:buffer";
    export type { Readable } from "node:stream";
    export { type BrowserWindow } from "electron";
    export { value, passkeys, Schema };
  `,
  );
  web.invalid(
    "rejects Node builtins with or without the node prefix",
    `
    import * as NodeFS from "node:fs";
    import * as fs from "fs/promises";
    export { NodeFS, fs };
  `,
    (output) => {
      assert.lengthOf(output.match(/server service or the desktop bridge/g) ?? [], 2);
    },
  );
  web.invalid(
    "rejects side effects and mixed type and runtime imports",
    `
    import "electron";
    import { type Stats, readFile } from "node:fs";
  `,
  );
  web.invalid(
    "rejects host re-exports and dynamic imports",
    `
    export * from "electron";
    export { readFile } from "node:fs";
    export const load = () => import("@effect/platform-node/NodeRuntime");
    export const template = () => import(\`node:child_process\`);
  `,
  );
  web.invalid(
    "rejects require and TypeScript import aliases",
    `
    import fs = require("node:fs");
    export const host = require("@t3tools/shared/HostProcess");
  `,
  );
  web.invalid(
    "rejects relative host source and type dependencies",
    `
    import type { Server } from "../../server/src/server";
    export * from "../../desktop/src/electron/ElectronApp";
    export const server = () => import("t3");
  `,
  );
  harness("apps/mobile/src/nested/component.ts").invalid(
    "protects mobile from host imports",
    `
    export const host = () => import("node:os");
  `,
  );
  harness("packages/client-runtime/src/nested/bridge.ts").invalid(
    "protects shared client logic from app internals",
    `
    export * from "../../../../apps/desktop/src/electron/ElectronApp";
  `,
  );
  harness("packages/contracts/src/wire.ts").invalid(
    "keeps contracts independent from client implementation",
    `
    import type { State } from "@t3tools/client-runtime/state";
    export * from "../../client-runtime/src/state";
  `,
  );
  harness("apps/server/src/service.ts").valid(
    "allows Node in host services",
    `
    export * from "node:fs";
  `,
  );
  harness("apps/desktop/src/preload.ts").valid(
    "allows Electron in the desktop bridge",
    `
    export { contextBridge } from "electron";
  `,
  );
  harness("apps/web/src/component.test.ts").valid(
    "allows Node in test fixtures",
    `
    export * from "node:fs";
  `,
  );
  harness("apps/mobile/src/lib/theme.test-support.ts").valid(
    "allows host test support",
    `
    export * from "node:fs";
  `,
  );
});
