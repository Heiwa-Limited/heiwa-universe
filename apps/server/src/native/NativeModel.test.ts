import * as NodeServices from "@effect/platform-node/NodeServices";
import { assert, describe, it } from "@effect/vitest";
import * as HostProcess from "@t3tools/shared/HostProcess";
import * as Deferred from "effect/Deferred";
import * as Effect from "effect/Effect";
import * as Fiber from "effect/Fiber";
import * as ProcessRunner from "../processRunner.ts";
import * as NativeModel from "./NativeModel.ts";

const output = (stdout: string, code = 0): ProcessRunner.ProcessRunOutput => ({
  stdout,
  stderr: "",
  code: code as ProcessRunner.ProcessRunOutput["code"],
  timedOut: false,
  stdoutTruncated: false,
  stderrTruncated: false,
  stdoutInvalidUtf8: false,
  stderrInvalidUtf8: false,
});
const excerpt = "Synthetic: team rehearsal at 3 pm on Friday.";

const service = (
  run: ProcessRunner.ProcessRunner["Service"]["run"],
  options: { platform?: NodeJS.Platform; architecture?: NodeJS.Architecture; helper?: string } = {},
) =>
  NativeModel.make.pipe(
    Effect.provideService(ProcessRunner.ProcessRunner, ProcessRunner.ProcessRunner.of({ run })),
    Effect.provideService(HostProcess.Platform, options.platform ?? "darwin"),
    Effect.provideService(HostProcess.Architecture, options.architecture ?? "arm64"),
    // Any existing file stands in for the compiled helper.
    Effect.provideService(HostProcess.Environment, {
      HEIWA_MODEL_HELPER: options.helper ?? process.execPath,
    }),
    Effect.provide(NodeServices.layer),
  );

describe("NativeModel.triage", () => {
  it.effect("sends only the excerpt over stdin with a deadline and output bound", () =>
    Effect.gen(function* () {
      const inputs: Array<ProcessRunner.ProcessRunInput> = [];
      const model = yield* service((input) => {
        inputs.push(input);
        return Effect.succeed(output('{"kind":"needs-clarification"}'));
      });
      assert.deepEqual(yield* model.triage({ excerpt }), { kind: "needs-clarification" });
      assert.deepEqual(inputs[0]?.args, []);
      assert.deepEqual(JSON.parse(inputs[0]?.stdin ?? ""), { task: "meeting-triage", excerpt });
      assert.equal(inputs[0]?.timeout, `${NativeModel.TRIAGE_TIMEOUT_SECONDS} seconds`);
      assert.equal(inputs[0]?.maxOutputBytes, 4096);
    }),
  );

  it.effect.each([
    { name: "off a Mac", platform: "linux" as const, code: "unavailable", detail: "runs on a Mac" },
    {
      name: "without a helper",
      helper: "/nonexistent/heiwa-model",
      code: "unavailable",
      detail: "missing",
    },
  ])("reports unavailable $name without launching", ({ platform, helper, code, detail }) =>
    Effect.gen(function* () {
      let launched = false;
      const model = yield* service(
        () => Effect.sync(() => ((launched = true), output('{"kind":"meeting"}'))),
        { ...(platform ? { platform } : {}), ...(helper ? { helper } : {}) },
      );
      const error = yield* model.triage({ excerpt }).pipe(Effect.flip);
      assert.equal(error.code, code);
      assert.include(error.detail, detail);
      assert.isFalse(launched);
    }),
  );

  it.effect.each([
    {
      stdout: '{"error":"unavailable","reason":"disabled"}',
      code: "unavailable",
      detail: "Apple Intelligence",
    },
    {
      stdout: '{"error":"unavailable","reason":"device"}',
      code: "unavailable",
      detail: "does not support",
    },
    {
      stdout: '{"error":"unavailable","reason":"not-ready"}',
      code: "unavailable",
      detail: "not ready",
    },
    { stdout: '{"error":"unavailable","reason":"os"}', code: "unavailable", detail: "macOS 26" },
    { stdout: '{"error":"invalid"}', code: "invalid", detail: "short excerpt" },
    { stdout: '{"error":"refused"}', code: "failed", detail: "declined" },
    { stdout: "", code: "failed", detail: "incomplete" },
    // A classification is trusted only with a clean exit.
    { stdout: '{"kind":"meeting"}', exit: 1, code: "failed", detail: "could not classify" },
    { stdout: '{"kind":"maybe"}', code: "failed", detail: "incomplete" },
  ])("maps helper reply $stdout to $code", ({ stdout, exit, code, detail }) =>
    Effect.gen(function* () {
      const model = yield* service(() =>
        Effect.succeed(output(stdout, exit ?? (stdout.includes("error") ? 1 : 0))),
      );
      const error = yield* model.triage({ excerpt }).pipe(Effect.flip);
      assert.equal(error.code, code);
      assert.include(error.detail, detail);
    }),
  );

  it.effect.each([
    { architecture: "x64" as const, code: "unavailable", detail: "Apple silicon" },
    { architecture: "arm64" as const, code: "failed", detail: "did not return" },
  ])("maps an unlaunchable helper on $architecture to $code", ({ architecture, code, detail }) =>
    Effect.gen(function* () {
      const model = yield* service(
        (input) =>
          Effect.fail(
            new ProcessRunner.ProcessSpawnError({
              command: input.command,
              argumentCount: 0,
              cause: new Error("Bad CPU type in executable"),
            }),
          ),
        { architecture },
      );
      const error = yield* model.triage({ excerpt }).pipe(Effect.flip);
      assert.equal(error.code, code);
      assert.include(error.detail, detail);
    }),
  );

  it.effect("reports a timed-out helper as failed, not as a classification", () =>
    Effect.gen(function* () {
      const model = yield* service((input) =>
        Effect.fail(
          new ProcessRunner.ProcessTimeoutError({
            command: input.command,
            argumentCount: 0,
            timeoutMs: NativeModel.TRIAGE_TIMEOUT_SECONDS * 1000,
          }),
        ),
      );
      const error = yield* model.triage({ excerpt }).pipe(Effect.flip);
      assert.equal(error.code, "failed");
      assert.include(error.detail, `within ${NativeModel.TRIAGE_TIMEOUT_SECONDS} seconds`);
    }),
  );

  it.effect("refuses a second launch while one is running, then accepts the next", () =>
    Effect.gen(function* () {
      const started = yield* Deferred.make<void>();
      const release = yield* Deferred.make<void>();
      let launches = 0;
      const model = yield* service(() =>
        Effect.gen(function* () {
          launches += 1;
          if (launches === 1) {
            yield* Deferred.succeed(started, undefined);
            yield* Deferred.await(release);
          }
          return output('{"kind":"other"}');
        }),
      );
      const first = yield* model.triage({ excerpt }).pipe(Effect.forkChild);
      yield* Deferred.await(started);
      const busy = yield* model.triage({ excerpt }).pipe(Effect.flip);
      assert.equal(busy.code, "failed");
      assert.include(busy.detail, "busy");
      yield* Deferred.succeed(release, undefined);
      assert.deepEqual(yield* Fiber.join(first), { kind: "other" });
      assert.deepEqual(yield* model.triage({ excerpt }), { kind: "other" });
      assert.equal(launches, 2);
    }),
  );
});
