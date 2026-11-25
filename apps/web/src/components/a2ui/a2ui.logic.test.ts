import { describe, expect, it } from "vite-plus/test";

import {
  actionEvent,
  buttonOrder,
  indexSurface,
  readPointer,
  resolveText,
  writePointer,
} from "./a2ui.logic";

const ask = [
  { id: "root", component: "Card", child: "body" },
  { id: "body", component: "Column", children: ["title", "choices"] },
  { id: "title", component: "Text", text: "Overdue: Pay rent" },
  { id: "choices", component: "Row", children: ["b_done", "b_later"] },
  { id: "l_done", component: "Text", text: "Done" },
  { id: "l_later", component: "Text", text: "Tomorrow" },
  {
    id: "b_done",
    component: "Button",
    child: "l_done",
    action: { event: { name: "answer", context: { askId: "abcd1234", choice: "done" } } },
  },
  {
    id: "b_later",
    component: "Button",
    child: "l_later",
    action: {
      event: { name: "answer", context: { askId: "abcd1234", note: { "@path": "/note" } } },
    },
  },
];

describe("indexSurface", () => {
  it("accepts a well-formed surface", () => {
    expect(indexSurface(ask).problems).toEqual([]);
  });
  it("reports a missing root, dangling refs and cycles", () => {
    expect(indexSurface(ask.slice(1)).problems).toContain('No component has the id "root".');
    expect(indexSurface([{ id: "root", component: "Card", child: "ghost" }]).problems).toEqual([
      '"root" refers to missing "ghost".',
    ]);
    expect(
      indexSurface([
        { id: "root", component: "Column", children: ["a"] },
        { id: "a", component: "Column", children: ["root"] },
      ]).problems,
    ).toEqual(["The component tree has a cycle."]);
  });
});

describe("data binding", () => {
  it("reads and writes JSON Pointers immutably", () => {
    const data = { user: { name: "Devon" } };
    expect(readPointer(data, "/user/name")).toBe("Devon");
    const next = writePointer(data, "/user/mood", "good");
    expect(next).toEqual({ user: { name: "Devon", mood: "good" } });
    expect(data).toEqual({ user: { name: "Devon" } });
  });
  it("resolves v1.0 @path and v0.9 path bindings and literals", () => {
    expect(resolveText({ "@path": "/a" }, { a: 1 })).toBe("1");
    expect(resolveText({ path: "/a" }, { a: "x" })).toBe("x");
    expect(resolveText("plain", {})).toBe("plain");
    expect(resolveText({ "@path": "/missing" }, {})).toBe("");
  });
});

describe("actions", () => {
  it("builds the A2UI event and resolves bound context at click time", () => {
    const { nodes } = indexSurface(ask);
    expect(actionEvent(nodes.get("b_later")!, "ask-abcd1234", { note: "after payday" })).toEqual({
      name: "answer",
      surfaceId: "ask-abcd1234",
      sourceComponentId: "b_later",
      context: { askId: "abcd1234", note: "after payday" },
    });
    expect(actionEvent(nodes.get("title")!, "ask-abcd1234", {})).toBeNull();
  });
  it("orders buttons for the 1-9 shortcuts in reading order", () => {
    expect(buttonOrder(indexSurface(ask).nodes)).toEqual(["b_done", "b_later"]);
  });
});
