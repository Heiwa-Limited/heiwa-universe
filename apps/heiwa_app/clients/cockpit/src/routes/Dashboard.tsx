import type { JSX } from "solid-js";
import { For } from "solid-js";
import { providers } from "../lib/providers";

export default function Dashboard(): JSX.Element {
  const connected = () =>
    providers.providers.filter((p) => p.maturity === "stable").length;
  const totalLanes = () => Object.keys(providers.lanes).length;

  return (
    <section class="hero compact">
      <p class="eyebrow">Cockpit</p>
      <h1>Heiwa is live.</h1>
      <p class="lede">
        Your local operator runtime. Route across {connected()} stable providers, {totalLanes()} lanes,
        and any local models you host.
      </p>
      <div class="kpi-grid">
        <div class="kpi"><span>Stable providers</span><strong>{connected()}</strong></div>
        <div class="kpi"><span>Lanes</span><strong>{totalLanes()}</strong></div>
        <div class="kpi"><span>Data source</span><strong>providers.json</strong></div>
        <div class="kpi"><span>Host</span><strong>localhost</strong></div>
      </div>
      <div class="callout" style={{ "margin-top": "1rem" }}>
        <div>
          <h2>Connected lanes</h2>
          <ul>
            <For each={Object.values(providers.lanes)}>
              {(lane) => <li><strong>{lane.label}</strong> — {lane.description}</li>}
            </For>
          </ul>
        </div>
      </div>
    </section>
  );
}
