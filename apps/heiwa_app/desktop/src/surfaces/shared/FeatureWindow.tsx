import { For, Show } from "solid-js";
import { useApp } from "../../state/app";
import type { SurfaceId } from "../ids";
import "./feature-window.css";

/**
 * Shared shape for surfaces whose read model is not wired yet (Mail, Finance,
 * Social). It states what the surface will do, what it is gated on, and what
 * it is honestly not doing today — rather than rendering an empty screen.
 *
 * These become real surfaces on the L3 connector plane; the descriptor comes
 * from app state so the copy has one source.
 */
export function FeatureWindow(props: { id: SurfaceId; pending: string }) {
  const app = useApp();
  const descriptor = () => app.subApps().find((sub) => sub.id === props.id);

  return (
    <div class="view feature-window">
      <Show when={descriptor()}>
        {(sub) => (
          <section class="panel feature-panel">
            <header>
              <span>{sub().title}</span>
              <strong>{sub().state}</strong>
            </header>
            <For
              each={[
                ["availability", sub().state],
                ["runtime boundary", sub().server],
                ["capabilities", sub().skills.join(" · ")],
                ["tool policy", sub().tools.join(" · ")],
                ["local defaults", sub().personalization.join(" · ")],
              ]}
            >
              {([label, value]) => (
                <div class="feature-row">
                  <span>{label}</span>
                  <strong>{value}</strong>
                </div>
              )}
            </For>
            <p class="feature-pending quiet">{props.pending}</p>
          </section>
        )}
      </Show>
    </div>
  );
}
