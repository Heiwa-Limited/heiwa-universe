// herd.ts — one view of the agent herd for humans and agents.
//
// Human:  deno task herd          (compact table)
// Agent:  deno task herd --json   (normalized JSON, stable shape)
//
// Reads herdr's socket API via the herdr CLI. Least-privilege on purpose:
// runs with --allow-run=herdr only (see deno.json task).

type HerdrPane = {
  pane_id: string;
  workspace_id: string;
  tab_id: string;
  agent_status: string;
  cwd?: string;
  foreground_cwd?: string;
};

type HerdrWorkspace = {
  workspace_id: string;
  label: string;
  pane_count: number;
  tab_count: number;
  agent_status: string;
};

type HerdrAgent = {
  terminal_id?: string;
  pane_id?: string;
  agent?: string;
  name?: string;
  state?: string;
  agent_status?: string;
  message?: string;
};

async function herdr(...args: string[]): Promise<Record<string, unknown>> {
  const cmd = new Deno.Command("herdr", { args, stdout: "piped", stderr: "piped" });
  const { code, stdout, stderr } = await cmd.output();
  const err = new TextDecoder().decode(stderr).trim();
  if (code !== 0) {
    throw new Error(`herdr ${args.join(" ")} failed (${code}): ${err}`);
  }
  return JSON.parse(new TextDecoder().decode(stdout));
}

function rows<T>(res: Record<string, unknown>, key: string): T[] {
  const result = res.result as Record<string, unknown> | undefined;
  return (result?.[key] as T[]) ?? [];
}

const [wsRes, paneRes, agentRes] = await Promise.all([
  herdr("workspace", "list"),
  herdr("pane", "list"),
  herdr("agent", "list"),
]);

const workspaces = rows<HerdrWorkspace>(wsRes, "workspaces");
const panes = rows<HerdrPane>(paneRes, "panes");
const agents = rows<HerdrAgent>(agentRes, "agents");

const agentByPane = new Map<string, HerdrAgent>();
for (const a of agents) {
  if (a.pane_id) agentByPane.set(a.pane_id, a);
}

const wsLabel = new Map(workspaces.map((w) => [w.workspace_id, w.label]));

const herd = panes.map((p) => {
  const agent = agentByPane.get(p.pane_id);
  return {
    workspace: wsLabel.get(p.workspace_id) ?? p.workspace_id,
    pane: p.pane_id,
    agent: agent?.agent ?? agent?.name ?? "-",
    state: agent?.state ?? agent?.agent_status ?? p.agent_status ?? "unknown",
    cwd: p.foreground_cwd ?? p.cwd ?? "-",
    message: agent?.message ?? "",
  };
});

if (Deno.args.includes("--json")) {
  console.log(JSON.stringify(
    {
      generated_at: new Date().toISOString(),
      workspaces: workspaces.map((w) => ({
        id: w.workspace_id,
        label: w.label,
        panes: w.pane_count,
        agent_status: w.agent_status,
      })),
      herd,
    },
    null,
    2,
  ));
} else {
  if (herd.length === 0) {
    console.log(
      "herd empty — no panes. start one: herdr agent start <name> --cwd <path> -- <argv>",
    );
    Deno.exit(0);
  }
  const home = Deno.env.get("HOME") ?? "";
  const shortCwd = (c: string) => home && c.startsWith(home) ? "~" + c.slice(home.length) : c;
  const widths = [12, 8, 10, 9];
  const line = (cols: string[]) =>
    cols.map((c, i) => (widths[i] ? c.padEnd(widths[i]) : c)).join("  ");
  console.log(line(["WORKSPACE", "PANE", "AGENT", "STATE"]) + "  CWD");
  for (const h of herd) {
    console.log(line([h.workspace, h.pane, h.agent, h.state]) + "  " + shortCwd(h.cwd));
    if (h.message) console.log(" ".repeat(45) + "· " + h.message);
  }
}
