import {
  ProviderSetupError,
  type ProviderAuthMethod,
  type ProviderInstanceId,
} from "@t3tools/contracts";
import * as ProviderAuthFlow from "@t3tools/provider-core/server/providerAuthFlow";
import * as PtyAdapter from "@t3tools/shared/PtyAdapter";
import * as Deferred from "effect/Deferred";
import * as Effect from "effect/Effect";
import * as Option from "effect/Option";
import * as Queue from "effect/Queue";
import * as Schema from "effect/Schema";

/**
 * Claude Code owns its OAuth client and token storage (macOS Keychain or
 * CLAUDE_CONFIG_DIR). Heiwa only drives `claude auth login` in a terminal the
 * user can see, so the browser link, pasted code and any prompts stay Claude's.
 */
export const CLAUDE_AUTH_METHODS: ReadonlyArray<ProviderAuthMethod> = [
  {
    id: "claudeai",
    name: "Claude subscription",
    description: "Use your Claude Pro or Max plan.",
    type: "terminal",
  },
  {
    id: "console",
    name: "Anthropic Console",
    description: "Bill usage to an Anthropic API account.",
    type: "terminal",
  },
];

const AuthStatus = Schema.Struct({ loggedIn: Schema.Boolean });
const decodeStatus = Schema.decodeUnknownOption(Schema.fromJsonString(AuthStatus));

/** `claude auth status` prints JSON; anything else is treated as signed out. */
export const parseClaudeAuthStatus = (stdout: string) =>
  Option.match(decodeStatus(stdout.trim()), {
    onNone: () => false,
    onSome: (status) => status.loggedIn,
  });

export interface ClaudeAuthOptions {
  readonly instanceId: ProviderInstanceId;
  /** Accounts are per config directory, so instances sharing one share a login. */
  readonly configDir: string;
  readonly cwd: string;
  readonly command: (args: ReadonlyArray<string>) => Effect.Effect<{
    readonly command: string;
    readonly args: ReadonlyArray<string>;
    readonly env: NodeJS.ProcessEnv;
  }>;
  readonly run: (
    args: ReadonlyArray<string>,
  ) => Effect.Effect<{ readonly stdout: string; readonly code: number }, ProviderSetupError>;
  readonly onChanged: (signedIn: boolean) => Effect.Effect<void, ProviderSetupError>;
}

export const makeClaudeAuth = Effect.fn("makeClaudeAuth")(function* (options: ClaudeAuthOptions) {
  const pty = yield* Effect.serviceOption(PtyAdapter.PtyAdapter);
  const failure = (operation: string, detail: string, cause?: unknown) =>
    new ProviderSetupError({ instanceId: options.instanceId, operation, detail, cause });

  const signedIn = options
    .run(["auth", "status"])
    .pipe(Effect.map((result) => result.code === 0 && parseClaudeAuthStatus(result.stdout)));

  const runLogin = Effect.fnUntraced(function* (
    methodId: string,
    context: ProviderAuthFlow.ProviderAuthFlowContext,
  ) {
    if (Option.isNone(pty))
      return yield* failure(
        "start",
        "Interactive sign-in is unavailable here. Run `claude auth login` on this Mac.",
      );
    const resolved = yield* options.command([
      "auth",
      "login",
      methodId === "console" ? "--console" : "--claudeai",
    ]);
    const exited = yield* Deferred.make<number>();
    const output = yield* Queue.sliding<string>(64);
    const process = yield* pty.value
      .spawn({
        shell: resolved.command,
        args: [...resolved.args],
        cwd: options.cwd,
        cols: 100,
        rows: 24,
        env: resolved.env,
      })
      .pipe(
        Effect.mapError((cause) =>
          failure("start", "Could not open the Claude sign-in terminal.", cause),
        ),
      );
    const detachData = process.onData((data) => {
      Queue.offerUnsafe(output, data.slice(-16_384));
    });
    const detachExit = process.onExit(({ exitCode }) => {
      Deferred.doneUnsafe(exited, Effect.succeed(exitCode));
    });
    yield* Effect.addFinalizer(() =>
      Effect.sync(() => {
        detachData();
        detachExit();
        try {
          process.kill();
        } catch {}
      }),
    );
    let transcript = "";
    let outputOffset = 0;
    const update = () =>
      context.setInteraction(
        { type: "terminal", id: "terminal", output: transcript, outputOffset },
        (response) =>
          response.type === "terminal"
            ? Effect.try({
                try: () => {
                  if (response.size) process.resize(response.size.cols, response.size.rows);
                  if (response.data) process.write(response.data);
                },
                catch: () =>
                  failure("respond", "The Claude sign-in terminal is no longer available."),
              })
            : Effect.void,
      );
    yield* update();
    yield* Queue.take(output).pipe(
      Effect.flatMap((data) => {
        outputOffset += data.length;
        transcript = (transcript + data).slice(-16_384);
        return update();
      }),
      Effect.forever,
      Effect.forkScoped,
    );
    if ((yield* Deferred.await(exited)) !== 0)
      return yield* failure("start", "Claude sign-in did not finish. Start sign-in again.");
  });

  return yield* ProviderAuthFlow.make({
    instanceId: options.instanceId,
    credentialBinding: { owner: "provider", key: `claude:${options.configDir}` },
    methods: Effect.succeed(CLAUDE_AUTH_METHODS),
    defaultMethodId: "claudeai",
    authenticate: Effect.fn("ClaudeAuth.authenticate")(function* (methodId, context) {
      yield* runLogin(methodId, context);
      yield* context.verifying;
      if (!(yield* signedIn))
        return yield* failure("verify", "Claude still reports no signed-in account. Try again.");
      yield* options.onChanged(true);
    }),
    logout: Effect.gen(function* () {
      const result = yield* options.run(["auth", "logout"]);
      if (result.code !== 0)
        return yield* failure("logout", "Claude could not sign out. Try again.");
      yield* options.onChanged(false);
    }),
  });
});
