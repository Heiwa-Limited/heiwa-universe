import { createMemo, Index, Show } from "solid-js";
import { cssToken, timeFmt } from "../../lib/format";
import type { OperatorProjection, OperatorSnapshot, OperatorTurn } from "../../operator/types";
import { useApp } from "../../state/app";
import type { SurfaceModule } from "../types";
import "./workers.css";

function routeForTurn(
  turn: OperatorTurn,
  routes: Record<string, OperatorProjection>,
): OperatorProjection | undefined {
  return Object.values(routes)
    .filter((route) => route.turnId === turn.turnId)
    .sort((left, right) => left.occurredAt.localeCompare(right.occurredAt))
    .at(-1);
}

function routeLabel(route?: OperatorProjection): string {
  if (!route) return "auto";
  const provider = typeof route.payload.provider === "string" ? route.payload.provider : "auto";
  const model = typeof route.payload.model === "string" ? route.payload.model : "";
  return [provider, model].filter(Boolean).join("/");
}

function liveWorkerCount(snapshot: OperatorSnapshot, panes: number, reported: number): number {
  const active = snapshot.turns.filter(
    (turn) => turn.status === "open" || turn.status === "running" || turn.status === "cancelling",
  ).length;
  return active + panes + reported;
}

function WorkersSurface() {
  const app = useApp();
  let dispatchInput: HTMLTextAreaElement | undefined;

  // Newest first, with the route and completed output each turn resolved once.
  const rows = createMemo(() => {
    const snapshot = app.operator.snapshot();
    return [...snapshot.turns].reverse().map((turn) => ({
      turn,
      route: routeLabel(routeForTurn(turn, snapshot.routesByCall)),
      output: snapshot.messages.find(
        (message) => message.role === "assistant" && message.turnId === turn.turnId,
      )?.body,
    }));
  });

  const dispatch = async () => {
    const task = dispatchInput?.value.trim() ?? "";
    if (!task || !app.operator.ready()) return;
    if (dispatchInput) dispatchInput.value = "";
    await app.operator.submit(task).catch(() => undefined);
  };

  return (
    <div class="view workers-view">
      <div class="view-header">
        <h2>Operator turns</h2>
        <p class="muted">
          Durable tasks routed per call to the cheapest provider above the required quality floor.
        </p>
      </div>

      <div class="panel dispatch-quick">
        <textarea
          ref={dispatchInput}
          rows="2"
          placeholder="Describe a task for Heiwa to route to the right local/provider-owned executor…"
        />
        <div class="dispatch-controls">
          <div class="route-note">
            <strong>Auto-routed</strong>
            <span class="quiet">
              Heiwa chooses the smallest sufficient route by task, privacy, quota, device state, and
              evidence quality.
            </span>
          </div>
          <button class="btn-primary" disabled={!app.operator.ready()} onClick={() => void dispatch()}>
            Dispatch
          </button>
        </div>
      </div>

      <div class="subagent-list">
        <Show
          when={rows().length > 0}
          fallback={<p class="muted">No operator turns yet.</p>}
        >
          <Index each={rows()}>
            {(row) => (
              <div class={`panel subagent-card ${cssToken(row().turn.status)}`}>
                <div class="sa-header">
                  <span class={`sa-status-dot ${cssToken(row().turn.status)}`} />
                  <span class="sa-task">{row().turn.prompt || "Operator turn"}</span>
                  <span class="sa-meta quiet">{row().route}</span>
                  <span class="sa-time quiet">
                    {timeFmt(Date.parse(row().turn.updatedAt || row().turn.startedAt || ""))}
                  </span>
                </div>
                <Show when={row().output}>
                  <div class="sa-output">{row().output}</div>
                </Show>
              </div>
            )}
          </Index>
        </Show>
      </div>
    </div>
  );
}

export const workersSurface: SurfaceModule = {
  id: "workers",
  label: "Workers",
  glyph: "◇",
  caption: "workers window",
  Component: WorkersSurface,
  preview: (app) => {
    const snapshot = app.operator.snapshot();
    return {
      title: "Workers",
      lines: [
        `${liveWorkerCount(snapshot, app.herd.panes().length, app.runtime.health()?.snapshot?.data?.workers?.live ?? 0)} live`,
        `${snapshot.turns.length} known tasks`,
      ],
    };
  },
};
