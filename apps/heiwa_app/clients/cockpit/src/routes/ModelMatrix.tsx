import type { JSX } from "solid-js";
import { For } from "solid-js";

type MatrixLane = {
  lane: string;
  status: "healthy" | "watch" | "offline";
  bestFor: string;
  userCopy: string;
};

type EvalAxis = {
  task: string;
  decisionInputs: string;
  visibleRationale: string;
};

const lanes: MatrixLane[] = [
  {
    lane: "Local/private",
    status: "healthy",
    bestFor: "calendar extraction, mail triage, short summaries, private context",
    userCopy: "Used local/private lane; no model choice needed.",
  },
  {
    lane: "Subscription CLI",
    status: "watch",
    bestFor: "coding agents, long reasoning, provider-owned specialist loops",
    userCopy: "Escalated to specialist lane because the task needed stronger reasoning.",
  },
  {
    lane: "Metered API",
    status: "watch",
    bestFor: "structured output, high-reliability drafting, overflow when subscriptions throttle",
    userCopy: "Used metered lane only because cheaper sufficient lanes were unavailable or lower quality.",
  },
  {
    lane: "Offline fallback",
    status: "offline",
    bestFor: "deterministic reads, receipts, cached state, no-network mode",
    userCopy: "Answered from local state; sync is stale or unavailable.",
  },
];

const axes: EvalAxis[] = [
  {
    task: "Priority mail scan",
    decisionInputs: "sensitivity, thread length, urgency, local model quality, send risk",
    visibleRationale: "Private triage lane; replies staged for approval.",
  },
  {
    task: "Calendar conflict resolution",
    decisionInputs: "event certainty, attendees, write risk, source freshness, undo posture",
    visibleRationale: "Read model sufficient; external edits require approval.",
  },
  {
    task: "Code/runtime work",
    decisionInputs: "repo state, tests, provider CLI auth, context size, eval history",
    visibleRationale: "Specialist code lane with receipts and verification output.",
  },
  {
    task: "Life brief",
    decisionInputs: "calendar/mail freshness, local facts, privacy, cost, latency",
    visibleRationale: "Local state first; cite stale sources before advising.",
  },
];

function statusClass(status: MatrixLane["status"]): string {
  if (status === "healthy") return "ok";
  if (status === "watch") return "warn";
  return "fail";
}

export default function ModelMatrixRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Model Matrix</p>
        <h1>Routing intelligence, not model picking</h1>
        <p class="lede">
          Heiwa chooses provider/model/device lanes through capability probes,
          evals, quota, privacy, cost, and evidence. Users see the reason and
          receipt, not a model dropdown.
        </p>
      </div>

      <div class="card-list">
        <For each={lanes}>
          {(lane) => (
            <article>
              <div class="status-card-head">
                <h3>{lane.lane}</h3>
                <span class={`status-badge ${statusClass(lane.status)}`}>
                  {lane.status}
                </span>
              </div>
              <p><strong>Best for:</strong> {lane.bestFor}</p>
              <p class="muted"><strong>User copy:</strong> {lane.userCopy}</p>
            </article>
          )}
        </For>
      </div>

      <div class="data-table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>Task class</th>
              <th>Matrix inputs</th>
              <th>What the user sees</th>
            </tr>
          </thead>
          <tbody>
            <For each={axes}>
              {(axis) => (
                <tr>
                  <td>{axis.task}</td>
                  <td>{axis.decisionInputs}</td>
                  <td>{axis.visibleRationale}</td>
                </tr>
              )}
            </For>
          </tbody>
        </table>
      </div>
    </section>
  );
}
