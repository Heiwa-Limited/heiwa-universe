import type { JSX } from "solid-js";

export default function ReplRoute(): JSX.Element {
  return (
    <section class="hero compact">
      <p class="eyebrow">REPL</p>
      <h1>Operator REPL</h1>
      <p class="lede">Placeholder. Streams from <code>/ws/repl</code> when wired to heiwa_core.</p>
    </section>
  );
}
