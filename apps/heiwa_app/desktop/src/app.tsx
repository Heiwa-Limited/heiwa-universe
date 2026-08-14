import { createEffect, on } from "solid-js";
import { Dynamic } from "solid-js/web";
import { AppProvider, useApp, type AppState } from "./state/app";
import { Composer } from "./shell/Composer";
import { Rail } from "./shell/Rail";
import { assertRegistryComplete, surfaceById } from "./surfaces/registry";
import "./theme/tokens.css";
import "./theme/base.css";
import "./shell/shell.css";

assertRegistryComplete();

function Shell() {
  const app = useApp();
  const active = () => surfaceById(app.view());

  // Refresh on every arrival, wherever navigation came from — the rail, a
  // Home tile, or the composer. Hanging this off the view signal rather off
  // the rail's click handler is what keeps the other entry points from
  // showing stale data.
  //
  // `on` pins the dependency to the view id: refresh reads runtime signals,
  // and a tracked read of those inside the effect would re-run it on every
  // data change.
  createEffect(
    on(app.view, (id) => {
      void surfaceById(id).refresh?.(app);
    }),
  );

  return (
    <div class="app-shell">
      <Rail onNavigate={(surface) => app.navigate(surface.id)} />
      <main class="main-area">
        {/*
          Dynamic mounts exactly the active surface. The shell holds no
          per-surface branch, so adding a surface never edits this file.
        */}
        <Dynamic component={active().Component} />
        <Composer caption={active().caption} />
      </main>
    </div>
  );
}

export function App(props: { state: AppState }) {
  return (
    <AppProvider state={props.state}>
      <Shell />
    </AppProvider>
  );
}
