// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { createRoot } from "solid-js";
import { AppProvider, createAppState } from "../../state/app";
import type { FinanceSummary } from "../../state/types";
import { financeSurface } from "./index";

let dispose: (() => void) | undefined;
afterEach(() => {
  cleanup();
  dispose?.();
});

const connected: FinanceSummary = {
  schema_version: "heiwa_finance_summary_v1",
  policy: "read_only",
  policy_note: "Heiwa reads balances, positions, and transactions. It has no way to place a trade or move money.",
  connections: { brokerage: true, market_data: true },
  settings: { base_currency: "CAD", benchmark: "XIC.TO", tfsa_room: { year: 2026, room_at_start: 7000 } },
  sync: { last_attempt_at: "2026-09-25T20:00:00Z", last_success_at: "2026-09-25T20:00:00Z", outcome: "ok", issues: [] },
  fx: { USD: { rate: 1.4145, date: "2026-09-25", source: "Bank of Canada" } },
  portfolio: {
    base_currency: "CAD",
    total_value: 560,
    cash: 20,
    invested: 540,
    unrealized_gain: 45,
    accounts: [{ id: "tfsa", name: "TFSA", institution: "Wealthsimple", kind: "tfsa", number_hint: "4567", value: 560, cash: 20, positions: 1 }],
    positions: [{
      symbol: "XEQT.TO",
      description: "iShares Core Equity ETF Portfolio",
      kind: "etf",
      currency: "CAD",
      units: 15,
      price: 36,
      value_base: 540,
      weight: 540 / 560,
      average_cost: 33,
      unrealized_gain_base: 45,
      unrealized_pct: 45 / 495,
      accounts: 1,
    }],
    unconverted: [],
  },
  tfsa: {
    year: 2026,
    contributions_ytd: 500,
    withdrawals_ytd: 0,
    room_remaining: 6500,
    room_status: "ok",
    trades_365d: 2,
    quick_sells_365d: 0,
    trading_level: "quiet",
    us_listed_value: 0,
    notes: [],
  },
  benchmark: {
    benchmark: "XIC.TO",
    status: "complete",
    flows: 2,
    flows_priced: 2,
    net_contributed: 500,
    shadow_value: 520,
    actual_value: 560,
    difference: 40,
    as_of: "2026-09-25",
    notes: ["Assumes each deposit bought XIC.TO at the next close and each withdrawal sold it, with no commission, spread, or tax."],
  },
  activity: {
    recent: [{ date: "2026-09-01", kind: "contribution", amount: 250, currency: "CAD", account: "TFSA …4567" }],
    total: 2,
  },
  freshness: { snapshot_synced_at: "2026-09-25T20:00:00Z", benchmark_bars_through: "2026-09-25" },
  next_actions: [],
};

function mount(summary: FinanceSummary | null, post?: ReturnType<typeof vi.fn>) {
  const get = vi.fn(async (path: string) => (path === "/api/v1/finance/summary" ? { data: summary } : { data: {} }));
  const send = post ?? vi.fn(async () => ({ data: { outcome: "ok", counts: {}, issues: [] } }));
  const state = createRoot((cleanupRoot) => {
    dispose = cleanupRoot;
    return createAppState({ runtime: { get: get as never, post: send as never } });
  });
  void state.runtime.loadFinance();
  const Finance = financeSurface.Component;
  render(() => (
    <AppProvider state={state}>
      <Finance />
    </AppProvider>
  ));
  return { state, get, post: send };
}

it("tells a new user how to connect, read-only, before anything is synced", async () => {
  mount({
    schema_version: "heiwa_finance_summary_v1",
    policy: "read_only",
    connections: { brokerage: false, market_data: false },
    portfolio: null,
    next_actions: ["Connect your brokerage read-only, then run `heiwa connect snaptrade`."],
  });
  expect(await screen.findByText("heiwa connect snaptrade")).toBeTruthy();
  expect(screen.getByText(/cannot trade or move money/)).toBeTruthy();
});

it("shows the total, holdings, TFSA room, and the benchmark verdict", async () => {
  mount(connected);
  expect(await screen.findByText("XEQT.TO")).toBeTruthy();
  expect(screen.getAllByText("$560.00").length).toBeGreaterThan(0);
  expect(screen.getByText("$6,500.00 left")).toBeTruthy();
  expect(screen.getByText("Ahead of XIC.TO by $40.00")).toBeTruthy();
  expect(screen.getAllByText(/TFSA …4567/).length).toBe(2);
});

it("withholds a verdict while price history is incomplete", async () => {
  mount({
    ...connected,
    benchmark: { ...connected.benchmark!, status: "partial", difference: null, flows_priced: 1, notes: ["1 of 2 deposits/withdrawals have no XIC.TO close."] },
  });
  expect(await screen.findByText(/No verdict yet/)).toBeTruthy();
  expect(screen.getByText(/1 of 2 deposits/)).toBeTruthy();
});

it("offers no action that could trade or move money", async () => {
  mount(connected);
  await screen.findByText("XEQT.TO");
  const labels = screen.getAllByRole("button").map((button) => button.textContent ?? "");
  expect(labels.filter((label) => /buy|sell|trade|order|transfer|withdraw|deposit/i.test(label))).toEqual([]);
});

it("syncs on request and reports what the sources said", async () => {
  const post = vi.fn(async () => ({
    data: { outcome: "partial", counts: {}, issues: [{ source: "Alpha Vantage", message: "rate limited; retry tomorrow" }] },
  }));
  mount(connected, post);
  fireEvent.click(await screen.findByRole("button", { name: "Sync now" }));
  await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/finance/sync", {}));
  expect(await screen.findByText(/rate limited; retry tomorrow/)).toBeTruthy();
});
