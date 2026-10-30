// @effect-diagnostics nodeBuiltinImport:off -- ACP is an isolated stdio worker; the parent runtime owns permissions and durable work.
import { randomUUID } from "node:crypto";
import * as Exit from "effect/Exit";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";
import * as Scope from "effect/Scope";
import * as Acp from "effect-acp/schema-v1";
import { mcpToolSession, type McpToolSession } from "../../mcp/AcpMcpStdioBridge.ts";
import {
  LOCAL_AGENT_INSTRUCTIONS,
  listLocalModels,
  runLocalTurn,
  type LocalMessage,
  type LocalTool,
} from "./LocalAgent.ts";
import { LOCAL_WORKSPACE_TOOLS, makeLocalWorkspaceTools } from "./LocalTools.ts";

const encode = Schema.encodeSync(Schema.fromJsonString(Schema.Unknown));
const Envelope = Schema.Struct({
  id: Schema.optional(Schema.Union([Schema.String, Schema.Number])),
  method: Schema.optional(Schema.String),
  params: Schema.optional(Schema.Unknown),
  result: Schema.optional(Schema.Unknown),
  error: Schema.optional(Schema.Unknown),
});
const decodeEnvelope = Schema.decodeSync(Schema.fromJsonString(Envelope));
const decodePermission = Schema.decodeUnknownSync(Acp.RequestPermissionResponse);
const ToolList = Schema.Struct({
  tools: Schema.Array(
    Schema.Struct({
      name: Schema.String,
      description: Schema.optional(Schema.String),
      inputSchema: Schema.Unknown,
    }),
  ),
});
const ALLOWED_MCP = new Set([
  "heiwa_workflow",
  "heiwa_native_read",
  "heiwa_native_propose",
  "heiwa_native_execute",
  "t3_thread_read",
]);
interface Session {
  cwd: string;
  model: string;
  models: string[];
  history: LocalMessage[];
  tools: LocalTool[];
  execute: (
    name: string,
    args: Readonly<Record<string, unknown>>,
    signal?: AbortSignal,
  ) => Promise<unknown>;
  active?: AbortController;
}
const options = (session: Pick<Session, "model" | "models">): Acp.SessionConfigOption[] => [
  {
    id: "model",
    category: "model",
    type: "select",
    name: "Model",
    currentValue: session.model,
    options: session.models.map((value) => ({ value, name: value })),
  },
];

/** Stable ACP 1 is translated by the foundation's tested ACP compatibility layer. */
export async function runOllamaAcp() {
  const endpoint = process.env.HEIWA_LOCAL_ENDPOINT ?? "http://127.0.0.1:11434";
  const sessions = new Map<string, Session>();
  const pending = new Map<
    string,
    { resolve: (value: unknown) => void; reject: (error: unknown) => void }
  >();
  const workerScope = await Effect.runPromise(Scope.make());
  let sequence = 0;
  const send = (message: unknown) => {
    process.stdout.write(`${encode(message)}\n`);
  };
  const notify = (sessionId: string, update: unknown) =>
    send({ jsonrpc: "2.0", method: "session/update", params: { sessionId, update } });
  const requestHost = (method: string, params: unknown, signal?: AbortSignal) =>
    new Promise<unknown>((resolve, reject) => {
      const id = `heiwa-local-${sequence++}`;
      const abort = () => {
        pending.delete(id);
        reject(signal?.reason ?? new Error("Cancelled"));
      };
      signal?.throwIfAborted();
      signal?.addEventListener("abort", abort, { once: true });
      pending.set(id, {
        resolve: (value) => {
          signal?.removeEventListener("abort", abort);
          resolve(value);
        },
        reject,
      });
      send({ jsonrpc: "2.0", id, method, params });
    });
  let mcp: McpToolSession | undefined;
  let mcpTools: LocalTool[] = [];
  const endpointMcp = process.env.T3_ACP_MCP_ENDPOINT;
  const authorization = process.env.T3_ACP_MCP_AUTHORIZATION;
  if (endpointMcp && authorization) {
    mcp = await Effect.runPromise(
      mcpToolSession({
        endpoint: endpointMcp,
        authorization,
        clientName: "heiwa-local-agent",
      }).pipe(Scope.provide(workerScope)),
    );
    if (mcp.listTools) {
      const listed = Schema.decodeUnknownSync(ToolList)(await Effect.runPromise(mcp.listTools()));
      mcpTools = listed.tools
        .filter((tool) => ALLOWED_MCP.has(tool.name))
        .map((tool) => ({
          name: tool.name,
          description: tool.description ?? tool.name,
          parameters: tool.inputSchema,
        }));
    }
  }
  const findSession = (input: unknown) => {
    const value = Schema.decodeUnknownSync(Schema.Struct({ sessionId: Schema.String }))(input);
    const session = sessions.get(value.sessionId);
    if (!session) throw new Error("No such local session.");
    return { id: value.sessionId, session };
  };
  const handle = async (line: string) => {
    let envelope: typeof Envelope.Type;
    try {
      envelope = decodeEnvelope(line);
    } catch {
      send({ jsonrpc: "2.0", id: null, error: { code: -32700, message: "Invalid ACP JSON." } });
      return;
    }
    if (!envelope.method) {
      const deferred = pending.get(String(envelope.id));
      if (deferred) {
        pending.delete(String(envelope.id));
        envelope.error
          ? deferred.reject(new Error("The host refused the request."))
          : deferred.resolve(envelope.result);
      }
      return;
    }
    const { method, params, id } = envelope;
    const reply = (result: unknown) => {
      if (id !== undefined) send({ jsonrpc: "2.0", id, result });
    };
    try {
      if (method === "initialize") {
        reply({
          protocolVersion: 1,
          agentInfo: { name: "Heiwa local", version: "0.4.0" },
          agentCapabilities: { loadSession: false, promptCapabilities: { embeddedContext: true } },
          authMethods: [],
        } satisfies Acp.InitializeResponse);
        return;
      }
      if (method === "session/new") {
        const input = Schema.decodeUnknownSync(Acp.NewSessionRequest)(params);
        const models = await listLocalModels(endpoint);
        const model =
          process.env.HEIWA_LOCAL_MODEL ??
          models.find((value) => value.startsWith("gemma4:")) ??
          models[0];
        if (!model || !models.includes(model))
          throw new Error("No installed local model declares completion and tool support.");
        const sessionId = randomUUID();
        const session: Session = {
          cwd: input.cwd,
          model,
          models,
          history: [{ role: "system", content: LOCAL_AGENT_INSTRUCTIONS }],
          tools: [...LOCAL_WORKSPACE_TOOLS, ...mcpTools],
          execute: async () => undefined,
        };
        const workspace = await makeLocalWorkspaceTools({
          cwd: input.cwd,
          ...(process.env.HEIWA_APPLE_HELPER ? { helper: process.env.HEIWA_APPLE_HELPER } : {}),
          approve: async (name, args) => {
            const result = decodePermission(
              await requestHost(
                "session/request_permission",
                {
                  sessionId,
                  toolCall: {
                    toolCallId: `permission-${sequence++}`,
                    title:
                      name === "process_run"
                        ? `Run ${String(args.executable)}`
                        : `Create ${String(args.path)}`,
                    name,
                    kind: name === "process_run" ? "execute" : "edit",
                    rawInput: args,
                    locations: typeof args.path === "string" ? [{ path: args.path }] : [],
                  },
                  options: [
                    { optionId: "allow", name: "Allow once", kind: "allow_once" },
                    { optionId: "reject", name: "Reject", kind: "reject_once" },
                  ],
                },
                session.active?.signal,
              ),
            );
            return result.outcome.outcome === "selected" && result.outcome.optionId === "allow";
          },
        });
        session.execute = async (name, args, signal) => {
          if (ALLOWED_MCP.has(name) && mcp)
            return Effect.runPromise(mcp.call(name, args), signal ? { signal } : undefined);
          return workspace(name, args, signal);
        };
        sessions.set(sessionId, session);
        reply({ sessionId, configOptions: options(session) } satisfies Acp.NewSessionResponse);
        return;
      }
      if (method === "session/cancel") {
        findSession(params).session.active?.abort();
        return;
      }
      if (method === "session/set_config_option") {
        const input = Schema.decodeUnknownSync(Acp.SetSessionConfigOptionRequest)(params);
        const { session } = findSession(input);
        if (
          session.active ||
          input.configId !== "model" ||
          typeof input.value !== "string" ||
          !session.models.includes(input.value)
        )
          throw new Error("This model option is not available.");
        session.model = input.value;
        reply({ configOptions: options(session) });
        return;
      }
      if (method === "session/set_model") {
        const input = Schema.decodeUnknownSync(
          Schema.Struct({ sessionId: Schema.String, modelId: Schema.String }),
        )(params);
        const { session } = findSession(input);
        if (session.active || !session.models.includes(input.modelId))
          throw new Error("This model is not available.");
        session.model = input.modelId;
        reply({});
        return;
      }
      if (method === "session/set_mode") {
        findSession(params);
        reply({});
        return;
      }
      if (method === "session/list") {
        reply({ sessions: [] });
        return;
      }
      if (method === "session/prompt") {
        const input = Schema.decodeUnknownSync(Acp.PromptRequest)(params);
        const { session } = findSession(input);
        if (session.active) throw new Error("This local session is already working.");
        const pieces = input.prompt.map((block) => {
          if (block.type === "text") return block.text;
          if (block.type === "resource_link") return `Referenced resource: ${block.uri}`;
          if (block.type === "resource" && "text" in block.resource)
            return `Attached resource ${block.resource.uri}:\n${block.resource.text}`;
          throw new Error(
            "This local model lane currently accepts text and text resources. Choose another provider for media.",
          );
        });
        const controller = new AbortController();
        session.active = controller;
        session.history.push({ role: "user", content: pieces.join("\n\n") });
        let currentTool = "";
        try {
          const result = await runLocalTurn({
            endpoint,
            model: session.model,
            messages: session.history,
            tools: session.tools,
            execute: session.execute,
            signal: controller.signal,
            onHistory: (history) => {
              session.history = [...history];
            },
            onTool: async (phase, name, args, output) => {
              if (phase === "start") {
                currentTool = randomUUID();
                notify(input.sessionId, {
                  sessionUpdate: "tool_call",
                  toolCallId: currentTool,
                  name,
                  title:
                    name === "fs_read"
                      ? `Read ${String(args.path)}`
                      : name === "fs_create"
                        ? `Create ${String(args.path)}`
                        : name === "process_run"
                          ? `Run ${String(args.executable)}`
                          : name,
                  kind:
                    name === "fs_read"
                      ? "read"
                      : name === "fs_create"
                        ? "edit"
                        : name === "process_run"
                          ? "execute"
                          : "other",
                  status: "in_progress",
                  rawInput: args,
                });
              } else {
                const failed =
                  typeof output === "object" &&
                  output !== null &&
                  ("error" in output ||
                    ("isError" in output && output.isError === true) ||
                    ("exitCode" in output && output.exitCode !== 0) ||
                    ("timedOut" in output && output.timedOut === true));
                notify(input.sessionId, {
                  sessionUpdate: "tool_call_update",
                  toolCallId: currentTool,
                  status: failed ? "failed" : "completed",
                  rawOutput: output,
                  content: [{ type: "content", content: { type: "text", text: encode(output) } }],
                });
              }
            },
          });
          session.history = result.messages;
          notify(input.sessionId, {
            sessionUpdate: "agent_message_chunk",
            content: { type: "text", text: result.text },
          });
          reply({ stopReason: "end_turn" } satisfies Acp.PromptResponse);
        } catch (error) {
          if (controller.signal.aborted) reply({ stopReason: "cancelled" });
          else throw error;
        } finally {
          delete session.active;
        }
        return;
      }
      if (id !== undefined)
        send({
          jsonrpc: "2.0",
          id,
          error: { code: -32601, message: "This ACP method is not supported by the local worker." },
        });
    } catch (error) {
      if (id !== undefined)
        send({
          jsonrpc: "2.0",
          id,
          error: {
            code: -32603,
            message: error instanceof Error ? error.message : "The local worker failed.",
          },
        });
    }
  };
  let buffer = "";
  process.stdin.setEncoding("utf8");
  process.stdin.on("data", (chunk: string) => {
    buffer += chunk;
    if (Buffer.byteLength(buffer) > 4 * 1024 * 1024) {
      process.stdin.destroy();
      process.exitCode = 1;
      return;
    }
    let newline: number;
    while ((newline = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, newline);
      buffer = buffer.slice(newline + 1);
      if (line.trim()) void handle(line);
    }
  });
  await new Promise<void>((resolve) => process.stdin.once("close", resolve).once("end", resolve));
  for (const session of sessions.values()) session.active?.abort();
  for (const deferred of pending.values()) deferred.reject(new Error("The local worker closed."));
  await Effect.runPromise(Scope.close(workerScope, Exit.void));
}
