import type { JSX } from "solid-js";

export default function GovernanceRoute(): JSX.Element {
  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Governance</p>
        <h1>Human-governed execution with a narrow public boundary</h1>
        <p class="lede">
          Governance in the cockpit mirrors the same self-hosted product truth
          as the public docs: local runtime for operator work, narrow public
          edge for delivery and status.
        </p>
      </div>

      <div class="panels">
        <article class="panel">
          <h2>Change control</h2>
          <ul>
            <li>Public claims must be backed by docs and CI.</li>
            <li>Humans review, merge, and deploy.</li>
            <li>Runtime changes should match the CLI and API specs.</li>
          </ul>
        </article>
        <article class="panel">
          <h2>Secrets and data</h2>
          <ul>
            <li>
              Operator secrets stay local under <code>~/.heiwa/</code>.
            </li>
            <li>
              <code>heiwa.ltd</code> does not store operator runtime state.
            </li>
            <li>Provider auth remains provider-owned.</li>
          </ul>
        </article>
        <article class="panel">
          <h2>Execution boundaries</h2>
          <ul>
            <li>The operator hosts the runtime, app, and REPL locally.</li>
            <li>
              Public edge paths exist for install, docs, identity exchange, and
              read-only status.
            </li>
            <li>
              Approvals, missions, traces, and memory belong to the local
              runtime.
            </li>
          </ul>
        </article>
      </div>

      <section class="callout">
        <div>
          <p class="eyebrow">Supported v1 surface</p>
          <h2>CLI, cockpit, local API, and docs</h2>
          <p>
            This cockpit is a local operator surface over the installed runtime.
            It should not grow hidden hosted dependencies as backend work lands.
          </p>
        </div>
        <div class="callout-metrics">
          <div>
            <span>Public docs</span>
            <strong>Cloudflare Pages</strong>
          </div>
          <div>
            <span>Runtime host</span>
            <strong>Installed local runtime</strong>
          </div>
          <div>
            <span>Contract</span>
            <strong>CLI.md + API.md</strong>
          </div>
        </div>
      </section>
    </section>
  );
}
