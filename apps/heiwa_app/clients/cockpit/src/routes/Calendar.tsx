import type { JSX } from "solid-js";
import { For } from "solid-js";

type CalendarLane = {
  name: string;
  status: "ready" | "planned" | "gated";
  sync: string;
  write: string;
  evidence: string;
};

type CalendarMoment = {
  time: string;
  title: string;
  source: string;
  pressure: "fixed" | "soft" | "draft";
  detail: string;
};

const lanes: CalendarLane[] = [
  {
    name: "Apple Calendar",
    status: "planned",
    sync: "Device-local EventKit bridge with Calendar permission and change notifications.",
    write: "Create/update events only after an approval lease is granted.",
    evidence: "Local receipt for permission, sync, source calendar, and before/after writes.",
  },
  {
    name: "Google Calendar",
    status: "gated",
    sync: "OAuth full sync + per-calendar incremental sync tokens; 410 recovery forces scoped resync.",
    write: "Draft focus holds, scheduling blocks, and event edits before external writes.",
    evidence: "Sync token, external event id, etag/version, approval id, and undo posture.",
  },
  {
    name: "Heiwa Holds",
    status: "ready",
    sync: "Local runtime state for focus blocks, travel buffers, and tentative commitments.",
    write: "Promote holds to Apple or Google only after user approval.",
    evidence: "Local hold receipt linked to the mission, mail thread, or planning request.",
  },
];

const moments: CalendarMoment[] = [
  {
    time: "08:30",
    title: "Daily operating brief",
    source: "Heiwa Hold",
    pressure: "soft",
    detail: "Summarize schedule, stale facts, priority mail, and pending approvals.",
  },
  {
    time: "10:00",
    title: "External appointment placeholder",
    source: "Apple/Google pending",
    pressure: "fixed",
    detail: "Connector disabled; reserved slot shows where synced obligations land.",
  },
  {
    time: "14:00",
    title: "Focus block draft",
    source: "Heiwa Hold",
    pressure: "draft",
    detail: "Can be promoted to an external calendar after approval.",
  },
];

function statusClass(status: CalendarLane["status"]): string {
  if (status === "ready") return "ok";
  if (status === "gated") return "warn";
  return "fail";
}

function pressureClass(pressure: CalendarMoment["pressure"]): string {
  if (pressure === "fixed") return "fail";
  if (pressure === "draft") return "warn";
  return "ok";
}

export default function CalendarRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Calendar</p>
        <h1>Heiwa Calendar</h1>
        <p class="lede">
          One local commitment map for Apple Calendar, Google Calendar, and
          Heiwa-owned holds. Syncs become read models first; external writes are
          staged through approvals and receipts.
        </p>
      </div>

      <div class="panels">
        <article class="panel">
          <div class="status-card-head">
            <h2>Today pressure</h2>
            <span class="status-badge warn">design slice</span>
          </div>
          <p class="muted">
            Calendar connectors are not enabled in this UI yet. This surface now
            reserves the product shape for schedule pressure, conflicts, and
            external write approvals.
          </p>
          <ul>
            <For each={moments}>
              {(moment) => (
                <li>
                  <code>{moment.time}</code> <strong>{moment.title}</strong>{" "}
                  <span class={`status-badge ${pressureClass(moment.pressure)}`}>
                    {moment.pressure}
                  </span>
                  <p class="muted">
                    {moment.source} · {moment.detail}
                  </p>
                </li>
              )}
            </For>
          </ul>
        </article>

        <article class="panel">
          <div class="status-card-head">
            <h2>Calendar UX contract</h2>
            <span class="status-badge ok">local-first</span>
          </div>
          <ul>
            <li>Show fixed obligations, soft holds, conflicts, and stale inputs in one timeline.</li>
            <li>Draft schedule changes before writing to Apple or Google.</li>
            <li>Attach receipts to every sync and external calendar mutation.</li>
            <li>Explain source freshness without exposing raw account secrets.</li>
          </ul>
        </article>
      </div>

      <div class="card-list">
        <For each={lanes}>
          {(lane) => (
            <article>
              <div class="status-card-head">
                <h3>{lane.name}</h3>
                <span class={`status-badge ${statusClass(lane.status)}`}>
                  {lane.status}
                </span>
              </div>
              <p><strong>Sync:</strong> {lane.sync}</p>
              <p><strong>Writes:</strong> {lane.write}</p>
              <p class="muted"><strong>Evidence:</strong> {lane.evidence}</p>
            </article>
          )}
        </For>
      </div>
    </section>
  );
}
