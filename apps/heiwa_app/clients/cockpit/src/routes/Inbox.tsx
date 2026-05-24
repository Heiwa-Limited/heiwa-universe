import type { JSX } from "solid-js";
import { For, Show } from "solid-js";
import { v1 } from "../lib/endpoints";
import { RemoteShell } from "../lib/resource";

function priorityClass(priority: string): string {
  if (priority === "high") return "fail";
  if (priority === "normal") return "warn";
  return "ok";
}

export default function InboxRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Inbox</p>
        <h1>Local work queue</h1>
        <p class="lede">
          Typed events and receipts from <code>~/.heiwa/state</code>.
        </p>
      </div>

      <RemoteShell loader={() => v1.inbox()}>
        {(data) => (
          <Show
            when={data.items.length > 0}
            fallback={
              <div class="empty-state">
                <strong>No local items.</strong>
                <p class="muted">The local read model has no event rows yet.</p>
              </div>
            }
          >
            <div class="card-list">
              <For each={data.items}>
                {(item) => (
                  <article>
                    <div class="status-card-head">
                      <h3>{item.title}</h3>
                      <span class={`status-badge ${priorityClass(item.priority)}`}>
                        {item.plane}
                      </span>
                    </div>
                    <p class="muted">
                      <code>{item.item_id}</code> · {item.kind} · {item.status}
                    </p>
                    <p>{item.summary}</p>
                    <p class="mono muted">
                      {item.occurred_at} · {item.source.source_type} ·{" "}
                      {item.source.label}
                    </p>
                    <Show when={item.receipt_refs.length > 0}>
                      <ul>
                        <For each={item.receipt_refs}>
                          {(receipt) => (
                            <li>
                              <span class="muted">{receipt.kind}</span> ·{" "}
                              <code>{receipt.ref}</code>
                            </li>
                          )}
                        </For>
                      </ul>
                    </Show>
                  </article>
                )}
              </For>
            </div>
          </Show>
        )}
      </RemoteShell>
    </section>
  );
}
