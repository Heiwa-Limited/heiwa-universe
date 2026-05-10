import { A } from "@solidjs/router";
import type { JSX, ParentProps } from "solid-js";

export default function App(props: ParentProps): JSX.Element {
  return (
    <>
      <header class="topbar">
        <A class="brand" href="/">
          HEIWA
        </A>
        <nav class="nav">
          <A href="/" end activeClass="is-active">
            Dashboard
          </A>
          <A href="/providers" activeClass="is-active">
            Providers
          </A>
          <A href="/connections" activeClass="is-active">
            Connections
          </A>
          <A href="/routes" activeClass="is-active">
            Routes
          </A>
          <A href="/live" activeClass="is-active">
            Live
          </A>
          <A href="/missions" activeClass="is-active">
            Missions
          </A>
          <A href="/approvals" activeClass="is-active">
            Approvals
          </A>
          <A href="/history" activeClass="is-active">
            History
          </A>
          <A href="/traces" activeClass="is-active">
            Traces
          </A>
          <A href="/memory" activeClass="is-active">
            Memory
          </A>
          <A href="/agents" activeClass="is-active">
            Agents
          </A>
          <A href="/crons" activeClass="is-active">
            Crons
          </A>
          <A href="/rate-groups" activeClass="is-active">
            Rate groups
          </A>
          <A href="/cells" activeClass="is-active">
            Cells
          </A>
          <A href="/status" activeClass="is-active">
            Status
          </A>
          <A href="/domains" activeClass="is-active">
            Domains
          </A>
          <A href="/governance" activeClass="is-active">
            Governance
          </A>
          <A href="/repl" activeClass="is-active">
            REPL
          </A>
        </nav>
      </header>
      <main>{props.children}</main>
    </>
  );
}
