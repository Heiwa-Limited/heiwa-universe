import { For, Show, createMemo } from "solid-js";
import type { CatalogState, SnapshotState } from "../../state/work";
import {
  describeRun,
  formatRecordedTime,
  runsFrom,
  statusLabel,
  type WorkStatus,
  type WorkSurfaceView,
} from "../../state/work-model";
import "./work-views.css";

/**
 * Presentation shared by Home, Work, and Workers. Everything here selects
 * from `app.work`; nothing fetches, folds, or keeps its own copy of a Work.
 */

export function WorkStatusChip(props: { status: WorkStatus; raw?: string }) {
  return <span class={`work-status work-status-${props.status}`}>{statusLabel(props.status, props.raw)}</span>;
}

/** Loading, failure, and staleness of the catalog, with a retry. */
export function CatalogNotice(props: { state: CatalogState; onRetry: () => void }) {
  const catalog = () => props.state.catalog;
  return <>
    <Show when={props.state.status === "loading"}><p class="work-notice" role="status">Loading Work…</p></Show>
    <Show when={props.state.error}>{(error) => (
      <p class="work-notice work-notice-problem" role="alert">
        {catalog() ? `Showing Work from an earlier load. ${error().message}` : error().message}
        {" "}<button onClick={() => props.onRetry()}>Retry</button>
      </p>
    )}</Show>
    <Show when={(catalog()?.truncated ?? 0) > 0}>
      <p class="work-notice">Showing the {catalog()!.rows.length} most recently updated of {catalog()!.total} Work.</p>
    </Show>
    <Show when={(catalog()?.skippedEvents ?? 0) > 0}>
      <p class="work-notice work-notice-problem">{catalog()!.skippedEvents} Work event(s) could not be read; some Work may be missing or out of date.</p>
    </Show>
    <Show when={(catalog()?.unreadableRows ?? 0) > 0}>
      <p class="work-notice work-notice-problem">{catalog()!.unreadableRows} Work row(s) could not be read by this app.</p>
    </Show>
  </>;
}

/** Loading, failure, and staleness of the selected Work, with a retry. */
export function DetailNotice(props: { state: SnapshotState; onRetry: () => void }) {
  return <>
    <Show when={props.state.loading && !props.state.surfaces}><p class="work-notice" role="status">Loading Work…</p></Show>
    <Show when={props.state.loading && props.state.surfaces}><p class="work-notice" role="status">Updating…</p></Show>
    <Show when={props.state.error}>{(error) => (
      <p class="work-notice work-notice-problem" role="alert">
        {props.state.stale ? `Showing an earlier snapshot. ${error().message}` : error().message}
        <Show when={error().kind !== "not_found"}>{" "}<button onClick={() => props.onRetry()} disabled={props.state.loading}>Retry</button></Show>
      </p>
    )}</Show>
  </>;
}

/** A bounded collection's omitted-row label, or nothing when it is complete. */
export function BoundNote(props: { view: WorkSurfaceView; collection: string; noun: string }) {
  const omitted = () => props.view.truncated[props.collection] ?? 0;
  return <Show when={omitted() > 0}>
    <p class="work-notice">{omitted()} more {props.noun} not shown; the runtime bounds this list.</p>
  </Show>;
}

export function WorkRunList(props: { view: WorkSurfaceView }) {
  const parsed = createMemo(() => runsFrom(props.view));
  return <div class="work-runs">
    <Show when={parsed().runs.length === 0 && parsed().unreadable === 0}>
      <p class="work-empty">No runs recorded for this Work.</p>
    </Show>
    <ul class="work-run-list">
      <For each={parsed().runs}>{(run) => {
        const description = () => describeRun(run);
        return <li class={`work-run work-tone-${description().tone}`}>
          <div class="work-run-head">
            <span class="work-run-dot" aria-hidden="true" />
            <strong>{description().label}</strong>
            <Show when={run.provider}><span class="work-run-meta">{run.provider}</span></Show>
            <small>Started {formatRecordedTime(run.startedAt)}</small>
          </div>
          <Show when={description().detail}><p class="work-run-detail">{description().detail}</p></Show>
          <Show when={description().supervision}>{(loss) => (
            <p class="work-supervision" role="note"><strong>{loss().headline}.</strong> {loss().observation}</p>
          )}</Show>
          <details class="work-diagnostics">
            <summary>Run details</summary>
            <dl>
              <dt>Run</dt><dd>{run.runId}</dd>
              <dt>Worker</dt><dd>{run.workerId}</dd>
              <dt>Thread</dt><dd>{run.threadId || "not recorded"}</dd>
              <dt>Recorded state</dt><dd>{run.rawState}</dd>
              <dt>Process ID</dt><dd>{run.pid ?? "not recorded"}</dd>
              <dt>Ended</dt><dd>{run.endedAt ? formatRecordedTime(run.endedAt) : "no ending recorded"}</dd>
              <Show when={run.exitCode !== null}><dt>Exit code</dt><dd>{run.exitCode}</dd></Show>
              <Show when={run.paneState}><dt>Pane</dt><dd>{run.paneState}</dd></Show>
            </dl>
          </details>
        </li>;
      }}</For>
    </ul>
    <BoundNote view={props.view} collection="runs" noun="runs" />
    <Show when={parsed().unreadable > 0}>
      <p class="work-notice work-notice-problem">{parsed().unreadable} run record(s) could not be read by this app.</p>
    </Show>
  </div>;
}
