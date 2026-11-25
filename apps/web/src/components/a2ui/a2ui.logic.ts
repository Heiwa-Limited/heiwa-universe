/**
 * A2UI v1.0 surface logic, kept pure so it is unit-tested apart from React.
 * Spec: https://a2ui.org/specification/v1.0-a2ui/ (flat adjacency list, one "root",
 * containers reference children by id, values are literals or `{"@path": "/x"}`).
 * `{"path": "/x"}` (v0.9.1) is accepted too, so older agents still render.
 */

export type A2uiNode = Readonly<Record<string, unknown>> & {
  readonly id: string;
  readonly component: string;
};

export type DataModel = Readonly<Record<string, unknown>>;

export interface A2uiActionEvent {
  readonly name: string;
  readonly surfaceId: string;
  readonly sourceComponentId: string;
  readonly context: Readonly<Record<string, unknown>>;
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);

/** Index components by id and report what would make the tree unrenderable. */
export function indexSurface(components: ReadonlyArray<Readonly<Record<string, unknown>>>): {
  readonly nodes: ReadonlyMap<string, A2uiNode>;
  readonly problems: ReadonlyArray<string>;
} {
  const nodes = new Map<string, A2uiNode>();
  const problems: string[] = [];
  for (const raw of components) {
    if (typeof raw.id !== "string" || typeof raw.component !== "string") {
      problems.push("A component is missing its id or type.");
      continue;
    }
    if (nodes.has(raw.id)) problems.push(`Duplicate component id "${raw.id}".`);
    nodes.set(raw.id, raw as A2uiNode);
  }
  if (!nodes.has("root")) problems.push('No component has the id "root".');
  for (const node of nodes.values()) {
    for (const ref of childRefs(node)) {
      if (!nodes.has(ref)) problems.push(`"${node.id}" refers to missing "${ref}".`);
    }
  }
  if (problems.length === 0 && hasCycle(nodes)) problems.push("The component tree has a cycle.");
  return { nodes, problems };
}

function childRefs(node: A2uiNode): ReadonlyArray<string> {
  const refs: string[] = [];
  if (typeof node.child === "string") refs.push(node.child);
  if (Array.isArray(node.children)) {
    for (const child of node.children) if (typeof child === "string") refs.push(child);
  }
  if (Array.isArray(node.tabs)) {
    for (const tab of node.tabs)
      if (isRecord(tab) && typeof tab.child === "string") refs.push(tab.child);
  }
  for (const key of ["trigger", "content"] as const) {
    if (typeof node[key] === "string") refs.push(node[key] as string);
  }
  return refs;
}

function hasCycle(nodes: ReadonlyMap<string, A2uiNode>): boolean {
  const state = new Map<string, "visiting" | "done">();
  const visit = (id: string): boolean => {
    const seen = state.get(id);
    if (seen === "visiting") return true;
    if (seen === "done") return false;
    state.set(id, "visiting");
    const node = nodes.get(id);
    const cyclic = node ? childRefs(node).some(visit) : false;
    state.set(id, "done");
    return cyclic;
  };
  return visit("root");
}

/** JSON Pointer (RFC 6901) read. */
export function readPointer(data: unknown, pointer: string): unknown {
  if (pointer === "" || pointer === "/") return data;
  let current: unknown = data;
  for (const raw of pointer.replace(/^\//, "").split("/")) {
    const key = raw.replace(/~1/g, "/").replace(/~0/g, "~");
    if (Array.isArray(current)) current = current[Number(key)];
    else if (isRecord(current)) current = current[key];
    else return undefined;
  }
  return current;
}

/** JSON Pointer write that returns a new object (the data model is immutable state). */
export function writePointer(data: DataModel, pointer: string, value: unknown): DataModel {
  const keys = pointer
    .replace(/^\//, "")
    .split("/")
    .map((key) => key.replace(/~1/g, "/").replace(/~0/g, "~"));
  const write = (node: unknown, index: number): unknown => {
    const key = keys[index]!;
    const base = isRecord(node) ? node : {};
    return { ...base, [key]: index === keys.length - 1 ? value : write(base[key], index + 1) };
  };
  return write(data, 0) as DataModel;
}

export function bindingPath(value: unknown): string | null {
  if (!isRecord(value)) return null;
  if (typeof value["@path"] === "string") return value["@path"];
  if (typeof value.path === "string") return value.path;
  return null;
}

/** A literal stays as is; a binding reads the data model. */
export function resolveValue(value: unknown, data: DataModel): unknown {
  const path = bindingPath(value);
  return path === null ? value : readPointer(data, path);
}

export function resolveText(value: unknown, data: DataModel): string {
  const resolved = resolveValue(value, data);
  return resolved === undefined || resolved === null ? "" : String(resolved);
}

/** Button action -> event, resolving bound context values at click time (spec behaviour). */
export function actionEvent(
  node: A2uiNode,
  surfaceId: string,
  data: DataModel,
): A2uiActionEvent | null {
  const action = node.action;
  if (!isRecord(action) || !isRecord(action.event)) return null;
  const event = action.event;
  if (typeof event.name !== "string") return null;
  const context: Record<string, unknown> = {};
  if (isRecord(event.context)) {
    for (const [key, value] of Object.entries(event.context))
      context[key] = resolveValue(value, data);
  }
  return { name: event.name, surfaceId, sourceComponentId: node.id, context };
}

/** Buttons in reading order, for the 1–9 keyboard shortcuts on a focused card. */
export function buttonOrder(nodes: ReadonlyMap<string, A2uiNode>): ReadonlyArray<string> {
  const order: string[] = [];
  const seen = new Set<string>();
  const walk = (id: string) => {
    if (seen.has(id)) return;
    seen.add(id);
    const node = nodes.get(id);
    if (!node) return;
    if (node.component === "Button") order.push(id);
    for (const ref of childRefs(node)) walk(ref);
  };
  walk("root");
  return order;
}
