import { createSignal, type Accessor } from "solid-js";
import {
  focusHerdPane,
  herdCommandCatalog,
  herdPanes,
  readHerdPane,
  runHerdPane,
  sendHerdPane,
  splitHerdPane,
  type HerdActionResult,
  type HerdCommandSpec,
  type HerdPane,
} from "../runtime";

/**
 * Multiplexer pane state. Shared because Home pins panes and Windows drives
 * them; selection survives the jump between the two.
 */
export type HerdState = {
  panes: Accessor<HerdPane[]>;
  commands: Accessor<HerdCommandSpec[]>;
  status: Accessor<"checking" | "online" | "offline">;
  source: Accessor<string>;
  selectedPane: Accessor<string | null>;
  paneText: Accessor<string>;
  paneSource: Accessor<string>;
  paneStatus: Accessor<string>;
  mode: Accessor<"send" | "run">;
  livePaneIds: Accessor<Set<string>>;
  setMode: (mode: "send" | "run") => void;
  select: (pane: string) => void;
  load: () => Promise<void>;
  loadCommands: () => Promise<void>;
  loadPaneText: () => Promise<void>;
  act: (action: () => Promise<HerdActionResult>) => Promise<void>;
  send: (pane: string, text: string) => Promise<HerdActionResult>;
  run: (pane: string, command: string) => Promise<HerdActionResult>;
  focus: (pane: string) => Promise<HerdActionResult>;
  split: (pane: string, direction: "right" | "down", cwd?: string) => Promise<HerdActionResult>;
};

export type HerdStateOptions = {
  snapshot?: typeof herdPanes;
  catalog?: typeof herdCommandCatalog;
  read?: typeof readHerdPane;
};

const NO_LIVE_PANE =
  "No live herdr pane selected. Start or attach panes through herdr, then refresh.";

export function createHerdState(options: HerdStateOptions = {}): HerdState {
  const snapshot$ = options.snapshot ?? herdPanes;
  const catalog$ = options.catalog ?? herdCommandCatalog;
  const read$ = options.read ?? readHerdPane;

  const [panes, setPanes] = createSignal<HerdPane[]>([]);
  const [commands, setCommands] = createSignal<HerdCommandSpec[]>([]);
  const [status, setStatus] = createSignal<"checking" | "online" | "offline">("checking");
  const [source, setSource] = createSignal("none");
  const [selectedPane, setSelectedPane] = createSignal<string | null>(null);
  const [paneText, setPaneText] = createSignal("");
  const [paneSource, setPaneSource] = createSignal("none");
  const [paneStatus, setPaneStatus] = createSignal("");
  const [mode, setMode] = createSignal<"send" | "run">("send");

  const livePaneIds = (): Set<string> => new Set(panes().map((pane) => pane.pane));

  async function load(): Promise<void> {
    try {
      const snapshot = await snapshot$();
      setPanes(
        snapshot.panes.map((row) => ({
          workspace: String(row.workspace ?? "heiwa"),
          pane: String(row.pane ?? "pane"),
          agent: String(row.agent ?? "-"),
          state: String(row.state ?? "unknown"),
          cwd: String(row.cwd ?? "-"),
          message: row.message ? String(row.message) : "",
        })),
      );
      setStatus(snapshot.status === "online" ? "online" : "offline");
      setSource(snapshot.source || "none");
    } catch {
      setPanes([]);
      setStatus("offline");
      setSource("none");
    }
  }

  async function loadCommands(): Promise<void> {
    try {
      setCommands(await catalog$());
    } catch {
      setCommands([]);
    }
  }

  async function loadPaneText(): Promise<void> {
    const pane = selectedPane();
    if (!pane || !livePaneIds().has(pane)) {
      setPaneText(NO_LIVE_PANE);
      setPaneSource("none");
      setPaneStatus("offline");
      return;
    }
    setPaneStatus("reading");
    const response = await read$(pane);
    setPaneText(response.ok ? response.text : `Read failed: ${response.error || "unknown error"}`);
    setPaneSource(response.source);
    setPaneStatus(response.ok ? "ready" : "error");
  }

  async function act(action: () => Promise<HerdActionResult>): Promise<void> {
    setPaneStatus("working");
    const result = await action();
    setPaneStatus(result.ok ? result.message : `Error: ${result.error || result.message}`);
    await load();
    await loadPaneText();
  }

  return {
    panes,
    commands,
    status,
    source,
    selectedPane,
    paneText,
    paneSource,
    paneStatus,
    mode,
    livePaneIds,
    setMode,
    select: setSelectedPane,
    load,
    loadCommands,
    loadPaneText,
    act,
    send: sendHerdPane,
    run: runHerdPane,
    focus: focusHerdPane,
    split: splitHerdPane,
  };
}
