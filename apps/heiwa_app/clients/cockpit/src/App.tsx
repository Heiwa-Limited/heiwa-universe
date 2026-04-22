import type { JSX, ParentProps } from "solid-js";
import { A } from "@solidjs/router";

export default function App(props: ParentProps): JSX.Element {
  return (
    <>
      <header class="topbar">
        <A class="brand" href="/">HEIWA</A>
        <nav class="nav">
          <A href="/" end activeClass="is-active">Dashboard</A>
          <A href="/providers" activeClass="is-active">Providers</A>
          <A href="/routes" activeClass="is-active">Routes</A>
          <A href="/repl" activeClass="is-active">REPL</A>
        </nav>
      </header>
      <main>{props.children}</main>
    </>
  );
}
