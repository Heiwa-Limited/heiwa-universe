import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";
import type { Receipt } from "../lib/types";
import { EmptyState, PageHero, Panel, StatusBadge } from "../lib/ui";

const preferredCountLanes = [
  "calendar",
  "automations",
  "mail",
  "promotion",
  "compress",
  "models",
];

function receiptSummary(receipt: Receipt): string {
  const data = receipt.data ?? {};
  const payload = JSON.stringify(data);
  if (payload.length <= 240) return payload;
  return `${payload.slice(0, 240)}…`;
}

function receiptTone(receipt: Receipt): "ok" | "warn" | "fail" {
  if (receipt.parse_error) return "fail";
  if (receipt.kind === "unknown") return "warn";
  return "ok";
}

export default function ReceiptsRoute(): JSX.Element {
  return (
    <section>
      <PageHero
        eyebrow="Receipts"
        title="Local evidence ledger"
        lede="Bounded, read-only summary of local receipts across calendar, automations, mail, promotion, compression, and model lanes."
      />

      <RemoteShell loader={() => v1.receipts()}>
        {(data) => (
          <>
            <div class="panels">
              <Panel
                title="Receipt index"
                status={`${data.counts.total ?? data.receipts.length} total`}
                tone={(data.counts.total ?? 0) > 0 ? "ok" : "warn"}
              >
                <p class="muted">
                  State: <code>{data.state_dir}</code>
                </p>
                <p class="muted">
                  Showing latest {data.receipts.length} of{" "}
                  {data.counts.total ?? data.receipts.length}
                  {data.truncated ? ` (capped at ${data.limit})` : ""}.
                </p>
              </Panel>

              <Panel title="Lane counts" status="local-only" tone="ok">
                <ul>
                  <For each={preferredCountLanes}>
                    {(lane) => (
                      <li>
                        <strong>{data.counts[lane] ?? 0}</strong> {lane}
                      </li>
                    )}
                  </For>
                </ul>
              </Panel>
            </div>

            <Show
              when={data.receipts.length > 0}
              fallback={
                <EmptyState title="No receipts found in known lanes.">
                  <p class="muted">
                    Create evidence through <code>heiwa calendar hold add</code>
                    , <code>heiwa auto trigger</code>,{" "}
                    <code>heiwa mail scan</code>, or{" "}
                    <code>heiwa app update --source checkout</code>.
                  </p>
                </EmptyState>
              }
            >
              <div class="data-table-wrap">
                <table class="data-table">
                  <thead>
                    <tr>
                      <th>Receipt</th>
                      <th>Lane</th>
                      <th>Kind</th>
                      <th>Created</th>
                      <th>Path</th>
                      <th>Data</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={data.receipts}>
                      {(receipt) => (
                        <tr>
                          <td>
                            <strong class="mono">{receipt.receipt_id}</strong>
                            <Show when={receipt.event}>
                              {(event) => (
                                <div class="muted mono">event: {event()}</div>
                              )}
                            </Show>
                          </td>
                          <td>
                            <StatusBadge
                              status={receipt.lane}
                              tone={receiptTone(receipt)}
                            />
                          </td>
                          <td>
                            <StatusBadge
                              status={receipt.kind}
                              tone={receiptTone(receipt)}
                            />
                            <Show when={receipt.parse_error}>
                              {(error) => (
                                <p class="repl-error">parse error: {error()}</p>
                              )}
                            </Show>
                          </td>
                          <td class="mono">{receipt.created_at}</td>
                          <td class="mono">{receipt.relative_path}</td>
                          <td>
                            <code>{receiptSummary(receipt)}</code>
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>
              </div>
            </Show>
          </>
        )}
      </RemoteShell>
    </section>
  );
}
