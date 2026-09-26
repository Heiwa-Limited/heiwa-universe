import { For, Show, createSignal } from "solid-js";
import { useApp } from "../../state/app";
import type { FinanceSummary } from "../../state/types";
import type { SurfaceModule } from "../types";
import "./finance.css";

const cad = new Intl.NumberFormat("en-CA", { style: "currency", currency: "CAD" });

function money(value: number | null | undefined): string {
  return typeof value === "number" && Number.isFinite(value) ? cad.format(value) : "—";
}

function percent(value: number | null | undefined): string {
  return typeof value === "number" && Number.isFinite(value) ? `${(value * 100).toFixed(1)}%` : "—";
}

const KIND_LABELS: Record<string, string> = {
  tfsa: "TFSA",
  rrsp: "RRSP",
  fhsa: "FHSA",
  resp: "RESP",
  rrif: "RRIF",
  lira: "LIRA",
  non_registered: "Non-registered",
  crypto: "Crypto",
  cash: "Cash",
};

/** Text with `backticked` spans rendered as commands. */
function Command(props: { text: string }) {
  return (
    <For each={props.text.split("`")}>
      {(part, index) => (index() % 2 === 1 ? <code>{part}</code> : part)}
    </For>
  );
}

function verdict(benchmark: NonNullable<FinanceSummary["benchmark"]>): string {
  const name = benchmark.benchmark;
  switch (benchmark.status) {
    case "complete": {
      const difference = benchmark.difference ?? 0;
      return difference >= 0
        ? `Ahead of ${name} by ${money(difference)}`
        : `Behind ${name} by ${money(-difference)}`;
    }
    case "partial":
      return `No verdict yet: local ${name} prices do not cover every deposit.`;
    case "no_prices":
      return `No verdict yet: no ${name} prices stored. Connect Alpha Vantage and sync.`;
    case "no_flows":
      return "No deposits to compare yet.";
    case "unsupported_currency":
      return `${name} is not CAD-listed; choose a CAD-listed benchmark.`;
    default:
      return benchmark.status;
  }
}

function roomLine(tfsa: NonNullable<FinanceSummary["tfsa"]>): string {
  const room = tfsa.room_remaining;
  if (tfsa.room_status === "unknown" || typeof room !== "number") return "Room not set";
  if (tfsa.room_status === "over") return `Over by ${money(-room)}`;
  return `${money(room)} left`;
}

function Finance() {
  const app = useApp();
  const summary = () => app.runtime.finance();
  const portfolio = () => summary()?.portfolio ?? null;
  const [synced, setSynced] = createSignal<string>();

  const sync = async () => {
    setSynced(undefined);
    const result = await app.runtime.syncFinance();
    if (result) setSynced(`Sync ${result.outcome === "ok" ? "complete" : result.outcome}.`);
  };

  const issues = () => app.runtime.financeSync()?.issues ?? summary()?.sync?.issues ?? [];
  const connected = () => Boolean(summary()?.connections?.brokerage || summary()?.connections?.market_data);

  return (
    <section class="finance-surface" aria-label="Finance">
      <header class="finance-header">
        <div>
          <h2 class="finance-title">Finance</h2>
          <p class="finance-subtitle">
            <Show when={portfolio()} fallback="Read-only accounts and prices">
              {(value) => `${money(value().total_value)} across ${value().accounts.length} account${value().accounts.length === 1 ? "" : "s"}`}
            </Show>
          </p>
        </div>
        <button
          class="finance-sync"
          disabled={app.runtime.financeSyncing() || !connected()}
          onClick={() => void sync()}
        >
          {app.runtime.financeSyncing() ? "Syncing…" : "Sync now"}
        </button>
      </header>

      <p class="finance-policy">
        Heiwa reads your accounts. It cannot trade or move money: there is no order path in Heiwa at all.
      </p>
      <Show when={app.runtime.financeError()}>
        {(message) => <p class="finance-error" role="alert">{message()}</p>}
      </Show>
      <Show when={synced()}>{(message) => <p class="finance-result" role="status">{message()}</p>}</Show>
      <Show when={issues().length > 0}>
        <ul class="finance-issues">
          <For each={issues()}>
            {(issue) => (
              <li>
                <strong>{issue.source}</strong> <Command text={issue.message} />
              </li>
            )}
          </For>
        </ul>
      </Show>

      <Show
        when={portfolio()}
        fallback={
          <div class="finance-empty">
            <p>No accounts in the local snapshot yet.</p>
            <For each={summary()?.next_actions ?? []}>
              {(action) => (
                <p class="finance-next">
                  <Command text={action} />
                </p>
              )}
            </For>
          </div>
        }
      >
        {(value) => (
          <>
            <div class="finance-tiles">
              <div class="finance-tile">
                <span>Total</span>
                <strong>{money(value().total_value)}</strong>
              </div>
              <div class="finance-tile">
                <span>Invested</span>
                <strong>{money(value().invested)}</strong>
              </div>
              <div class="finance-tile">
                <span>Cash</span>
                <strong>{money(value().cash)}</strong>
              </div>
              <div class="finance-tile">
                <span>Unrealized</span>
                <strong
                  classList={{
                    "finance-up": (value().unrealized_gain ?? 0) > 0,
                    "finance-down": (value().unrealized_gain ?? 0) < 0,
                  }}
                >
                  {money(value().unrealized_gain)}
                </strong>
              </div>
            </div>

            <div class="finance-columns">
              <div class="finance-main">
                <h3>Holdings</h3>
                <table class="finance-table">
                  <thead>
                    <tr>
                      <th scope="col">Symbol</th>
                      <th scope="col">Units</th>
                      <th scope="col">Value</th>
                      <th scope="col">Weight</th>
                      <th scope="col">Gain</th>
                    </tr>
                  </thead>
                  <tbody>
                    <For each={value().positions}>
                      {(position) => (
                        <tr>
                          <th scope="row">
                            <span class="finance-symbol">{position.symbol}</span>
                            <Show when={position.description}>
                              {(description) => <small>{description()}</small>}
                            </Show>
                          </th>
                          <td>{position.units.toLocaleString("en-CA", { maximumFractionDigits: 4 })}</td>
                          <td>{money(position.value_base)}</td>
                          <td>{percent(position.weight)}</td>
                          <td
                            classList={{
                              "finance-up": (position.unrealized_gain_base ?? 0) > 0,
                              "finance-down": (position.unrealized_gain_base ?? 0) < 0,
                            }}
                          >
                            {money(position.unrealized_gain_base)}
                          </td>
                        </tr>
                      )}
                    </For>
                  </tbody>
                </table>

                <h3>Recent activity</h3>
                <ul class="finance-activity">
                  <For each={(summary()?.activity?.recent ?? []).slice(0, 8)}>
                    {(row) => (
                      <li>
                        <span>{row.date ?? "—"}</span>
                        <span>
                          {row.kind}
                          {row.symbol ? ` ${row.symbol}` : ""}
                        </span>
                        <span>{money(row.amount)}</span>
                        <small>{row.account}</small>
                      </li>
                    )}
                  </For>
                </ul>
              </div>

              <aside class="finance-side">
                <section class="finance-card">
                  <h3>Accounts</h3>
                  <For each={value().accounts}>
                    {(account) => (
                      <div class="finance-row">
                        <span>
                          {account.name}
                          {account.number_hint ? ` …${account.number_hint}` : ""}
                          <small>{KIND_LABELS[account.kind] ?? "Account"} · {account.institution}</small>
                        </span>
                        <strong>{money(account.value)}</strong>
                      </div>
                    )}
                  </For>
                </section>

                <Show when={summary()?.tfsa}>
                  {(tfsa) => (
                    <section class="finance-card">
                      <h3>TFSA {tfsa().year}</h3>
                      <div class="finance-row">
                        <span>Contributed this year</span>
                        <strong>{money(tfsa().contributions_ytd)}</strong>
                      </div>
                      <div class="finance-row">
                        <span>Room</span>
                        <strong classList={{ "finance-down": tfsa().room_status === "over" }}>{roomLine(tfsa())}</strong>
                      </div>
                      <div class="finance-row">
                        <span>Trading pace (12 months)</span>
                        <strong
                          classList={{
                            "finance-warn": tfsa().trading_level === "watch",
                            "finance-down": tfsa().trading_level === "elevated",
                          }}
                        >
                          {tfsa().trading_level} · {tfsa().trades_365d} trades
                        </strong>
                      </div>
                      <For each={tfsa().notes}>{(note) => <p class="finance-note"><Command text={note} /></p>}</For>
                    </section>
                  )}
                </Show>

                <Show when={summary()?.benchmark}>
                  {(benchmark) => (
                    <section class="finance-card">
                      <h3>You vs {benchmark().benchmark}</h3>
                      <p class="finance-verdict">{verdict(benchmark())}</p>
                      <Show when={benchmark().status === "complete"}>
                        <div class="finance-row">
                          <span>Your accounts</span>
                          <strong>{money(benchmark().actual_value)}</strong>
                        </div>
                        <div class="finance-row">
                          <span>Same deposits in {benchmark().benchmark}</span>
                          <strong>{money(benchmark().shadow_value)}</strong>
                        </div>
                      </Show>
                      <For each={benchmark().notes}>{(note) => <p class="finance-note">{note}</p>}</For>
                    </section>
                  )}
                </Show>
              </aside>
            </div>

            <p class="finance-footer">
              <Show when={summary()?.freshness?.snapshot_synced_at}>
                {(at) => `Accounts read ${new Date(at()).toLocaleString()}`}
              </Show>
              <Show when={summary()?.freshness?.benchmark_bars_through}>
                {(through) => ` · prices through ${through()}`}
              </Show>
              <Show when={summary()?.fx?.USD}>
                {(usd) => ` · USD/CAD ${usd().rate} (Bank of Canada, ${usd().date})`}
              </Show>
            </p>
          </>
        )}
      </Show>
    </section>
  );
}

export const financeSurface: SurfaceModule = {
  id: "finance",
  label: "Finance",
  glyph: "$",
  caption: "finance window",
  Component: Finance,
  preview: (app) => {
    const summary = app.runtime.finance();
    const portfolio = summary?.portfolio;
    return {
      title: "Finance",
      lines: [
        portfolio
          ? `${money(portfolio.total_value)} · ${portfolio.accounts.length} account${portfolio.accounts.length === 1 ? "" : "s"}`
          : summary?.connections?.brokerage
            ? "connected · not synced yet"
            : "not connected",
        "read-only · never trades",
      ],
    };
  },
  // Reads the local read model only; syncing brokerages and prices stays an
  // explicit action so a visible window never spends rate-limited requests.
  refresh: (app) => app.runtime.loadFinance(),
  liveIntervalMs: 300_000,
};
