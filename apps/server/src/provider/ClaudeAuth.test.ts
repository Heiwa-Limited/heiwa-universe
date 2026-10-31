import * as NodeServices from "@effect/platform-node/NodeServices";
import { assert, it } from "@effect/vitest";
import { ProviderInstanceId, type ProviderAuthState } from "@t3tools/contracts";
import * as PtyAdapter from "@t3tools/shared/PtyAdapter";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Stream from "effect/Stream";

import { makeClaudeAuth, parseClaudeAuthStatus } from "./ClaudeAuth.ts";

const instanceId = ProviderInstanceId.make("claude-auth-test");

const makeHarness = (statusAfterLogin: string) =>
  Effect.gen(function* () {
    const runs: string[][] = [];
    const changed: boolean[] = [];
    const written: string[] = [];
    let spawned: PtyAdapter.PtySpawnInput | undefined;
    let loggedIn = false;
    let data: ((data: string) => void) | undefined;
    let exit: ((event: PtyAdapter.PtyExitEvent) => void) | undefined;
    const controller = yield* makeClaudeAuth({
      instanceId,
      configDir: "/home/.claude",
      cwd: "/state",
      command: (args) => Effect.succeed({ command: "/bin/claude", args, env: { HOME: "/home" } }),
      run: (args) =>
        Effect.sync(() => {
          runs.push([...args]);
          return {
            code: 0,
            stdout:
              args[1] === "status" ? (loggedIn ? statusAfterLogin : '{"loggedIn":false}') : "",
          };
        }),
      onChanged: (value) => Effect.sync(() => void changed.push(value)),
    }).pipe(
      Effect.provideService(PtyAdapter.PtyAdapter, {
        spawn: (input) =>
          Effect.sync(() => {
            spawned = input;
            return {
              pid: 1,
              write: (value) => void written.push(value),
              resize: () => {},
              kill: () => {},
              onData: (callback) => {
                data = callback;
                return () => {};
              },
              onExit: (callback) => {
                exit = callback;
                return () => {};
              },
            };
          }),
      }),
    );
    const state = (predicate: (state: ProviderAuthState) => boolean) =>
      controller
        .subscribe("owner")
        .pipe(Stream.filter(predicate), Stream.runHead, Effect.map(Option.getOrThrow));
    return {
      controller,
      state,
      runs,
      changed,
      written,
      spawned: () => spawned,
      output: (value: string) => data?.(value),
      finish: (code: number) => {
        loggedIn = true;
        exit?.({ exitCode: code, signal: null });
      },
    };
  });

it.effect("drives claude auth login in a terminal and verifies the account before succeeding", () =>
  Effect.gen(function* () {
    const h = yield* makeHarness('{"loggedIn":true,"authMethod":"claude.ai"}');
    const flow = yield* h.controller.start("owner", Effect.void, "claudeai");
    yield* h.state((value) => value.interaction?.type === "terminal");
    assert.deepEqual(h.spawned()?.args, ["auth", "login", "--claudeai"]);
    assert.equal(h.spawned()?.shell, "/bin/claude");
    h.output("Paste code here if prompted > ");
    yield* h.state(
      (value) =>
        value.interaction?.type === "terminal" &&
        value.interaction.output.includes("Paste code here"),
    );
    yield* h.controller.respond!("owner", {
      instanceId,
      flowId: flow.flowId!,
      interactionId: "terminal",
      response: { type: "terminal", data: "code#state\r" },
    });
    assert.deepEqual(h.written, ["code#state\r"]);
    h.finish(0);
    yield* h.state((value) => value.phase === "succeeded");
    assert.deepEqual(h.runs, [["auth", "status"]]);
    assert.deepEqual(h.changed, [true]);
  }).pipe(Effect.scoped, Effect.provide(NodeServices.layer)),
);

it.effect("fails instead of claiming success when Claude still reports no account", () =>
  Effect.gen(function* () {
    const h = yield* makeHarness('{"loggedIn":false}');
    yield* h.controller.start("owner", Effect.void, "console");
    yield* h.state((value) => value.interaction?.type === "terminal");
    assert.deepEqual(h.spawned()?.args, ["auth", "login", "--console"]);
    h.finish(0);
    yield* h.state((value) => value.phase === "failed");
    assert.deepEqual(h.changed, []);
  }).pipe(Effect.scoped, Effect.provide(NodeServices.layer)),
);

it("reads only an explicit loggedIn flag from claude auth status", () => {
  assert.isTrue(parseClaudeAuthStatus('{"loggedIn":true}\n'));
  assert.isFalse(parseClaudeAuthStatus('{"loggedIn":false}'));
  assert.isFalse(parseClaudeAuthStatus("Not logged in"));
});
