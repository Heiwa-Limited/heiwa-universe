import type { JSX, ParentProps } from "solid-js";
import { Show } from "solid-js";

/**
 * One tone vocabulary for every status string the runtime emits, so all
 * surfaces color connector/lane/route states identically.
 */
export type Tone = "ok" | "warn" | "fail";

const TONE_BY_STATUS: Record<string, Tone> = {
  // healthy / live
  ready: "ok",
  connected: "ok",
  healthy: "ok",
  metadata: "ok",
  ok: "ok",
  local_model: "ok",
  deterministic: "ok",
  // pending operator action / partial
  staged: "warn",
  needs_auth: "warn",
  gated: "warn",
  watch: "warn",
  draft: "warn",
  soft: "warn",
  report: "warn",
  remote_model: "warn",
  // not available
  planned: "fail",
  offline: "fail",
  error: "fail",
  unavailable: "fail",
  fixed: "fail",
  approval: "fail",
};

export function statusTone(status: string | null | undefined): Tone {
  if (!status) return "warn";
  return TONE_BY_STATUS[status] ?? "warn";
}

export function StatusBadge(props: {
  status: string;
  tone?: Tone | undefined;
}): JSX.Element {
  return (
    <span class={`status-badge ${props.tone ?? statusTone(props.status)}`}>
      {props.status}
    </span>
  );
}

export function PageHero(props: {
  eyebrow: string;
  title: string;
  lede: string;
}): JSX.Element {
  return (
    <div class="hero compact">
      <p class="eyebrow">{props.eyebrow}</p>
      <h1>{props.title}</h1>
      <p class="lede">{props.lede}</p>
    </div>
  );
}

export function PanelHead(props: {
  title: string;
  status?: string | undefined;
  tone?: Tone | undefined;
}): JSX.Element {
  return (
    <div class="status-card-head">
      <h2>{props.title}</h2>
      <Show when={props.status}>
        {(status) => <StatusBadge status={status()} tone={props.tone} />}
      </Show>
    </div>
  );
}

export function Panel(
  props: ParentProps<{
    title: string;
    status?: string | undefined;
    tone?: Tone | undefined;
  }>,
): JSX.Element {
  return (
    <article class="panel">
      <PanelHead title={props.title} status={props.status} tone={props.tone} />
      {props.children}
    </article>
  );
}

export function EmptyState(
  props: ParentProps<{ title: string }>,
): JSX.Element {
  return (
    <div class="empty-state">
      <strong>{props.title}</strong>
      {props.children}
    </div>
  );
}
