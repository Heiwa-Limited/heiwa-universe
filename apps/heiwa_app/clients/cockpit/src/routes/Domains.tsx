import type { JSX } from "solid-js";
import { For } from "solid-js";
import domainManifest from "../../../web/assets/domains.bootstrap.json";

type DomainManifest = typeof domainManifest;

function humanLabel(value: string): string {
  return value.replaceAll("_", " ");
}

export default function DomainsRoute(): JSX.Element {
  const manifest: DomainManifest = domainManifest;

  return (
    <section>
      <div class="hero compact">
        <p class="eyebrow">Domains</p>
        <h1>Public topology and delivery boundaries</h1>
        <p class="lede">
          Static domain manifest mirrored into the cockpit for operator
          reference. Public pages live on the edge; runtime control stays local.
        </p>
      </div>

      <section class="callout">
        <div>
          <p class="eyebrow">Current view</p>
          <h2>{manifest.root_domain}</h2>
          <p class="section-copy">
            DNS on {humanLabel(manifest.platform.dns)}, public web on{" "}
            {humanLabel(manifest.platform.public_web)}, and control plane on{" "}
            {humanLabel(manifest.platform.control_plane)}.
          </p>
        </div>
        <div class="callout-metrics">
          <div>
            <span>State ledger</span>
            <strong>{manifest.platform.state_ledger}</strong>
          </div>
          <div>
            <span>State endpoint</span>
            <strong>{manifest.platform.state_endpoint}</strong>
          </div>
          <div>
            <span>Generated from</span>
            <strong>{manifest.generated_from}</strong>
          </div>
        </div>
      </section>

      <div class="panels">
        <For each={manifest.domains}>
          {(domain) => (
            <article class="panel">
              <div class="status-card-head">
                <h2 class="mono">{domain.host}</h2>
                <span
                  class={`status-badge ${domain.state === "active" ? "ok" : "warn"}`}
                >
                  {domain.state}
                </span>
              </div>
              <p>{domain.purpose}</p>
              <p class="muted">{domain.target}</p>
              <p class="mono">health {domain.health_path}</p>
            </article>
          )}
        </For>
        <article class="panel panel-full">
          <h2>Bootstrap steps</h2>
          <ol class="steps-list">
            <For each={manifest.bootstrap_steps}>
              {(step) => <li>{step}</li>}
            </For>
          </ol>
        </article>
      </div>
    </section>
  );
}
