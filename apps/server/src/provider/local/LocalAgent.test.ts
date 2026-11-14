import { assert, expect, it } from "@effect/vitest";
import { localOllamaEndpoint, runLocalTurn } from "./LocalAgent.ts";
const tool = {
  name: "fixture_read",
  description: "Read the fixture",
  parameters: { type: "object", properties: {} },
};
const setup = (responses: unknown[]) => {
  const requests: Record<string, unknown>[] = [];
  return {
    requests,
    fetchImplementation: (async (_url: unknown, init: RequestInit) => {
      if (String(_url).endsWith("/api/show"))
        return Response.json({ capabilities: ["completion", "tools"] });
      requests.push(JSON.parse(init.body as string));
      return Response.json(responses.shift());
    }) as typeof fetch,
  };
};
it("executes an offered tool and sends its result back before returning the model answer", async () => {
  const f = setup([
    {
      done: true,
      done_reason: "stop",
      message: {
        role: "assistant",
        content: "",
        tool_calls: [{ function: { name: "fixture_read", arguments: {} } }],
      },
    },
    { done: true, message: { role: "assistant", content: "Verified fixture." } },
  ]);
  let executions = 0;
  const result = await runLocalTurn({
    ...f,
    endpoint: "http://127.0.0.1:11434",
    model: "fixture",
    messages: [{ role: "user", content: "Read it" }],
    tools: [tool],
    execute: async () => {
      executions++;
      return { content: "proof" };
    },
  });
  assert.equal(executions, 1);
  assert.equal(result.toolCalls, 1);
  assert.equal(result.text, "Verified fixture.");
  assert.include(JSON.stringify(f.requests[1]), '"tool_name":"fixture_read"');
  assert.include(JSON.stringify(f.requests[1]), "proof");
});
it("never executes a tool the host did not offer", async () => {
  const f = setup([
    {
      done: true,
      message: {
        role: "assistant",
        content: "",
        tool_calls: [{ function: { name: "heiwa_native_approve", arguments: { id: "forged" } } }],
      },
    },
    { done: true, message: { role: "assistant", content: "Approval is unavailable." } },
  ]);
  let executions = 0;
  await runLocalTurn({
    ...f,
    endpoint: "http://localhost:11434",
    model: "fixture",
    messages: [],
    tools: [tool],
    execute: async () => {
      executions++;
    },
  });
  assert.equal(executions, 0);
});
it("does not execute partial tool calls after an incomplete generation", async () => {
  const f = setup([
    {
      done: false,
      message: {
        role: "assistant",
        content: "",
        tool_calls: [{ function: { name: tool.name, arguments: {} } }],
      },
    },
  ]);
  let executions = 0;
  await expect(
    runLocalTurn({
      ...f,
      endpoint: "http://localhost:11434",
      model: "fixture",
      messages: [],
      tools: [tool],
      execute: async () => {
        executions++;
      },
    }),
  ).rejects.toThrow();
  assert.equal(executions, 0);
});
it("rejects nonlocal destinations, credentials and cloud model aliases", async () => {
  for (const url of [
    "https://localhost:11434",
    "http://example.com",
    "http://127.0.0.1.attacker.test",
    "http://user:secret@localhost:11434",
    "http://localhost:11434/private",
  ])
    assert.throws(() => localOllamaEndpoint(url));
  await expect(
    runLocalTurn({
      endpoint: "http://localhost:11434",
      model: "fixture:cloud",
      messages: [],
      tools: [],
      execute: async () => undefined,
    }),
  ).rejects.toThrow();
});
it("cancellation prevents additional model calls and tool execution", async () => {
  const c = new AbortController();
  c.abort();
  const f = setup([]);
  await expect(
    runLocalTurn({
      ...f,
      endpoint: "http://localhost:11434",
      model: "fixture",
      messages: [],
      tools: [tool],
      signal: c.signal,
      execute: async () => undefined,
    }),
  ).rejects.toThrow();
  assert.equal(f.requests.length, 0);
});

it("refuses context overflow before making a chat request and retains completed tool history", async () => {
  const huge = setup([]);
  await expect(
    runLocalTurn({
      ...huge,
      endpoint: "http://localhost:11434",
      model: "fixture",
      messages: [{ role: "user", content: "x".repeat(25000) }],
      tools: [],
      execute: async () => undefined,
    }),
  ).rejects.toThrow("context limit");
  assert.equal(huge.requests.length, 0);
  const f = setup([
    {
      done: true,
      message: {
        role: "assistant",
        content: "",
        tool_calls: [{ function: { name: tool.name, arguments: {} } }],
      },
    },
    { done: false, message: { role: "assistant", content: "" } },
  ]);
  let saved: readonly unknown[] = [];
  await expect(
    runLocalTurn({
      ...f,
      endpoint: "http://localhost:11434",
      model: "fixture",
      messages: [],
      tools: [tool],
      execute: async () => ({ saved: true }),
      onHistory: (messages) => {
        saved = [...messages];
      },
    }),
  ).rejects.toThrow();
  assert.deepStrictEqual(JSON.parse((saved[1] as { content: string }).content), { saved: true });
});
