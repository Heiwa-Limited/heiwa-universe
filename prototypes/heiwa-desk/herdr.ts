// Thin client over the herdr CLI (which speaks to the server's socket API).

export type HerdRow = {
  workspace: string;
  pane: string;
  agent: string;
  state: string;
  cwd: string;
};

async function exec(args: string[], expectJson = true): Promise<unknown> {
  const cmd = new Deno.Command("herdr", { args, stdout: "piped", stderr: "piped" });
  const { code, stdout, stderr } = await cmd.output();
  const out = new TextDecoder().decode(stdout);
  if (code !== 0) {
    throw new Error(`herdr ${args.join(" ")}: ${new TextDecoder().decode(stderr).trim()}`);
  }
  return expectJson ? JSON.parse(out) : out;
}

function result<T>(res: unknown, key: string): T[] {
  return ((res as Record<string, Record<string, T[]>>).result?.[key]) ?? [];
}

export async function getHerd(): Promise<HerdRow[]> {
  const [wsRes, paneRes, agentRes] = await Promise.all([
    exec(["workspace", "list"]),
    exec(["pane", "list"]),
    exec(["agent", "list"]),
  ]);
  type P = {
    pane_id: string;
    workspace_id: string;
    agent_status: string;
    cwd?: string;
    foreground_cwd?: string;
  };
  type W = { workspace_id: string; label: string };
  type A = { pane_id?: string; agent?: string; name?: string; state?: string };
  const wsLabel = new Map(result<W>(wsRes, "workspaces").map((w) => [w.workspace_id, w.label]));
  const agentByPane = new Map(
    result<A>(agentRes, "agents").filter((a) => a.pane_id).map((a) => [a.pane_id!, a]),
  );
  return result<P>(paneRes, "panes").map((p) => {
    const a = agentByPane.get(p.pane_id);
    return {
      workspace: wsLabel.get(p.workspace_id) ?? p.workspace_id,
      pane: p.pane_id,
      agent: a?.agent ?? a?.name ?? "-",
      state: a?.state ?? p.agent_status ?? "unknown",
      cwd: p.foreground_cwd ?? p.cwd ?? "-",
    };
  });
}

export function readPane(paneId: string): Promise<string> {
  return exec(
    ["pane", "read", paneId, "--source", "visible", "--format", "ansi"],
    false,
  ) as Promise<string>;
}

export async function sendToPane(paneId: string, text: string): Promise<void> {
  await exec(["pane", "send-text", paneId, text], false);
  await exec(["pane", "send-keys", paneId, "enter"], false);
}

export async function runInPane(paneId: string, command: string): Promise<void> {
  await exec(["pane", "run", paneId, command], false);
}

export async function focusPane(paneId: string): Promise<void> {
  await exec(["agent", "focus", paneId], false);
}

export async function splitPane(
  paneId: string,
  direction: "right" | "down",
  cwd?: string,
): Promise<void> {
  const args = ["pane", "split", paneId, "--direction", direction, "--focus"];
  if (cwd && cwd !== "-") args.push("--cwd", cwd);
  await exec(args, false);
}
