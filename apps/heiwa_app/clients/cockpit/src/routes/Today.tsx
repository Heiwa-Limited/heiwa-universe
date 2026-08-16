import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";
import type { FreshnessReport, TodaySnapshot } from "../lib/types";

interface TodayBundle {
  snapshot: TodaySnapshot;
  freshness: FreshnessReport;
}

async function loadTodayBundle(): Promise<TodayBundle> {
  const [snapshot, freshness] = await Promise.all([
    v1.lifeToday(),
    v1.lifeFreshness(),
  ]);
  return { snapshot, freshness };
}

function isApprovalEvent(msg: unknown): boolean {
  if (typeof msg !== "object" || msg === null) return false;
  const event = (msg as { event?: unknown }).event;
  return (
    event === "dispatch_request_appeared" ||
    event === "dispatch_request_decided"
  );
}

function dayTypeBadge(dayType: string): "ok" | "warn" | "fail" {
  if (dayType === "off") return "ok";
  if (dayType === "work") return "warn";
  return "fail";
}

export default function TodayRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Today</p>
        <h1>Operator daily read model</h1>
        <p class="lede">
          Local snapshot built from <code>~/.heiwa/state/life/plans/</code> and{" "}
          <code>~/.heiwa/state/dispatch/</code>. No connectors, no cloud.
        </p>
      </div>

      <RemoteShell loader={loadTodayBundle} liveEventFilter={isApprovalEvent}>
        {(data) => {
          const { snapshot, freshness } = data;
          return (
            <div class="card-list">
              <article>
                <div class="status-card-head">
                  <h3>
                    {snapshot.date}{" "}
                    <span class="muted mono">({snapshot.timezone})</span>
                  </h3>
                  <span
                    class={`status-badge ${dayTypeBadge(snapshot.day_type)}`}
                  >
                    {snapshot.day_type}
                  </span>
                </div>
                <Show when={snapshot.proof_target}>
                  <p>
                    <strong>Proof target:</strong> {snapshot.proof_target}
                  </p>
                </Show>
                <Show when={snapshot.scorecard_notes}>
                  <p class="muted">{snapshot.scorecard_notes}</p>
                </Show>
                <p class="mono muted">
                  runtime evidence={snapshot.runtime.evidence_mode}
                </p>
              </article>

              <article>
                <div class="status-card-head">
                  <h3>Work shifts</h3>
                  <span class="status-badge ok">
                    {snapshot.work_shifts.length}
                  </span>
                </div>
                <Show
                  when={snapshot.work_shifts.length > 0}
                  fallback={<p class="muted">No shifts scheduled.</p>}
                >
                  <ul>
                    <For each={snapshot.work_shifts}>
                      {(shift) => <li class="mono">{shift}</li>}
                    </For>
                  </ul>
                </Show>
              </article>

              <article>
                <div class="status-card-head">
                  <h3>Appointments</h3>
                  <span class="status-badge ok">
                    {snapshot.appointments.length}
                  </span>
                </div>
                <Show
                  when={snapshot.appointments.length > 0}
                  fallback={<p class="muted">Nothing upcoming on file.</p>}
                >
                  <ul>
                    <For each={snapshot.appointments}>
                      {(appt) => (
                        <li>
                          <code>{appt.date}</code> <strong>{appt.kind}</strong>
                          {appt.note && <> — {appt.note}</>}
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </article>

              <article>
                <div class="status-card-head">
                  <h3>Pending approvals</h3>
                  <span
                    class={`status-badge ${snapshot.pending_approvals.length > 0 ? "warn" : "ok"}`}
                  >
                    {snapshot.pending_approvals.length}
                  </span>
                </div>
                <Show
                  when={snapshot.pending_approvals.length > 0}
                  fallback={
                    <p class="muted">
                      Dispatch queue is clear at{" "}
                      <code>~/.heiwa/state/dispatch/requests/</code>.
                    </p>
                  }
                >
                  <ul>
                    <For each={snapshot.pending_approvals}>
                      {(req) => (
                        <li>
                          <code>{req.id}</code> · {req.action} →{" "}
                          <code>{req.target}</code>{" "}
                          <span class="muted">risk={req.risk}</span>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </article>

              <article>
                <div class="status-card-head">
                  <h3>Source freshness</h3>
                  <span
                    class={`status-badge ${freshness.stale_sources > 0 ? "warn" : "ok"}`}
                  >
                    {freshness.stale_sources} stale
                  </span>
                </div>
                <p class="muted mono">date: {freshness.date}</p>
                <Show
                  when={freshness.sources.length > 0}
                  fallback={<p class="muted">No sources configured.</p>}
                >
                  <ul>
                    <For each={freshness.sources}>
                      {(src) => (
                        <li>
                          <code>
                            {src.group}:{src.label}
                          </code>{" "}
                          <span class="muted">
                            age=
                            {src.age_days === null ? "?" : `${src.age_days}d`}{" "}
                            sla=
                            {src.sla_days}d {src.stale ? "stale" : "fresh"}
                          </span>
                        </li>
                      )}
                    </For>
                  </ul>
                </Show>
              </article>

              <Show when={snapshot.stale_facts.length > 0}>
                <article>
                  <div class="status-card-head">
                    <h3>Stale facts</h3>
                    <span class="status-badge warn">
                      {snapshot.stale_facts.length}
                    </span>
                  </div>
                  <ul>
                    <For each={snapshot.stale_facts}>
                      {(fact) => (
                        <li>
                          <code>{fact.label}</code>{" "}
                          <span class="muted">
                            age={fact.age_days}d sla={fact.sla_days}d
                          </span>
                        </li>
                      )}
                    </For>
                  </ul>
                </article>
              </Show>
            </div>
          );
        }}
      </RemoteShell>
    </section>
  );
}
