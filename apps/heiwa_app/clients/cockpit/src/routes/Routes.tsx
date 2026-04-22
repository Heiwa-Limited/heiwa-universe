import type { JSX } from "solid-js";

export default function RoutesRoute(): JSX.Element {
  return (
    <section class="hero compact">
      <p class="eyebrow">Routes</p>
      <h1>Routing config</h1>
      <p class="lede">Placeholder. Fetches live routes from <code>/api/routes</code> when wired to heiwa_core.</p>
    </section>
  );
}
