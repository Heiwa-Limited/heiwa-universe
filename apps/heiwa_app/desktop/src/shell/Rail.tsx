import { For } from "solid-js";
import { runtimeStatus } from "../runtime";
import { useApp } from "../state/app";
import { SURFACES } from "../surfaces/registry";
import type { SurfaceModule } from "../surfaces/types";

/**
 * Icon rail. Iterates the surface registry — it has no per-surface knowledge
 * beyond the SurfaceModule contract.
 */
export function Rail(props: { onNavigate: (surface: SurfaceModule) => void }) {
  const app = useApp();

  return (
    <nav class="icon-rail" aria-label="Surfaces">
      <div class="rail-logo" aria-hidden="true">
        H
      </div>

      <For each={SURFACES}>
        {(surface) => (
          <button
            class="rail-btn"
            classList={{ active: app.view() === surface.id }}
            aria-label={surface.label}
            aria-current={app.view() === surface.id ? "page" : undefined}
            title={surface.label}
            onClick={() => props.onNavigate(surface)}
          >
            <span class="rail-glyph" aria-hidden="true">
              {surface.glyph}
            </span>
            <span class="dock-preview" role="tooltip">
              <strong>{surface.preview(app).title}</strong>
              <For each={surface.preview(app).lines}>{(line) => <span>{line}</span>}</For>
            </span>
          </button>
        )}
      </For>

      <div class="rail-spacer" />
      <div
        class="rail-status"
        classList={{
          online: Boolean(app.runtime.health()?.reachable),
          offline: !app.runtime.health()?.reachable,
        }}
        title={runtimeStatus(app.runtime.health())}
      />
    </nav>
  );
}
