import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { type Lane, providers } from "../lib/providers";
import { RemoteShell } from "../lib/resource";
import type { Connector } from "../lib/types";
import { EmptyState, PageHero, StatusBadge } from "../lib/ui";

function providerMeta(providerId: string): {
  label: string;
  maturity: string | null;
  lanes: Lane[];
} {
  const match = providers.providers.find(
    (provider) => provider.id === providerId,
  );
  return {
    label: match?.name ?? providerId,
    maturity: match ? providers.maturity[match.maturity].label : null,
    lanes: match?.lanes ?? [],
  };
}

const KIND_ORDER: Array<{ kind: string; title: string; blurb: string }> = [
  {
    kind: "provider",
    title: "Provider runtimes",
    blurb: "CLI-auth provider lanes the routing matrix can dispatch to.",
  },
  {
    kind: "calendar",
    title: "Calendar lanes",
    blurb: "Read models first; external writes stage through approvals.",
  },
  {
    kind: "mail",
    title: "Mail lanes",
    blurb: "Metadata and read scopes only; sends are approval-gated.",
  },
];

function ConnectorCard(props: { connector: Connector }): JSX.Element {
  const meta = () =>
    props.connector.kind === "provider"
      ? providerMeta(props.connector.id)
      : null;
  return (
    <article class="panel">
      <div class="status-card-head">
        <h2>{meta()?.label ?? props.connector.display_name}</h2>
        <StatusBadge status={props.connector.status} />
      </div>
      <p class="muted">
        {props.connector.auth_kind}
        {props.connector.rate_group ? ` · ${props.connector.rate_group}` : ""}
        {meta()?.maturity ? ` · ${meta()?.maturity}` : ""}
      </p>
      <p>{props.connector.detail}</p>
      <Show when={props.connector.scopes}>
        <p class="mono muted">scope: {props.connector.scopes}</p>
      </Show>
      <Show when={meta() && (meta()?.lanes.length ?? 0) > 0}>
        <div class="operator-meta">
          <For each={meta()?.lanes ?? []}>
            {(lane) => <span class="pill">{providers.lanes[lane].short}</span>}
          </For>
        </div>
      </Show>
      <Show when={props.connector.next_action}>
        <p class="mono">
          next: <code>{props.connector.next_action}</code>
        </p>
      </Show>
    </article>
  );
}

export default function ConnectionsRoute(): JSX.Element {
  return (
    <section>
      <PageHero
        eyebrow="Connections"
        title="Connector registry"
        lede="Provider runtimes plus calendar and mail lanes in one governed registry: status, auth kind, scope, and the exact next command when action is needed."
      />

      <RemoteShell loader={() => v1.connectors()}>
        {(data) => (
          <Show
            when={data.connectors.length > 0}
            fallback={
              <EmptyState title="No connector metadata yet.">
                <p class="muted">
                  Link a provider with{" "}
                  <code>heiwa providers link &lt;id&gt;</code>.
                </p>
              </EmptyState>
            }
          >
            <For each={KIND_ORDER}>
              {(group) => {
                const rows = data.connectors.filter(
                  (connector) => connector.kind === group.kind,
                );
                return (
                  <Show when={rows.length > 0}>
                    <div class="connector-group">
                      <div class="section-header-row">
                        <span class="section-title">{group.title}</span>
                        <span class="muted">{group.blurb}</span>
                      </div>
                      <div class="panels">
                        <For each={rows}>
                          {(connector) => (
                            <ConnectorCard connector={connector} />
                          )}
                        </For>
                      </div>
                    </div>
                  </Show>
                );
              }}
            </For>
            <p class="mono muted">policy: {data.policy.join(" · ")}</p>
          </Show>
        )}
      </RemoteShell>
    </section>
  );
}
