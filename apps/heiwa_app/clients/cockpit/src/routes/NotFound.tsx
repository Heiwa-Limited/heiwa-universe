import type { JSX } from "solid-js";
import { A } from "@solidjs/router";

export default function NotFound(): JSX.Element {
  return (
    <section class="hero compact">
      <p class="eyebrow">404</p>
      <h1>Not found.</h1>
      <p class="lede">This cockpit route doesn't exist.</p>
      <div class="hero-actions">
        <A class="btn btn-outline" href="/">Back to dashboard</A>
      </div>
    </section>
  );
}
