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

  return (
    <div class="app-shell">
      <Rail
        onNavigate={(surface) => {
          app.navigate(surface.id);
          void surface.refresh?.(app);
        }}
      />
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
