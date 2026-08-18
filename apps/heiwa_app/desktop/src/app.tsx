import { createEffect, on, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { AppProvider, useApp, type AppState } from "./state/app";
import { Composer } from "./shell/Composer";
import { FirstRun } from "./shell/FirstRun";
import { Rail } from "./shell/Rail";
import { UpdateBanner } from "./shell/UpdateBanner";
import { assertRegistryComplete, surfaceById } from "./surfaces/registry";
import type { OnboardingState } from "./state/types";
import type { UpdateOffer } from "./runtime";
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

export type AppProps = {
  state: AppState;
  /**
   * First-run state from `heiwa_identity::onboarding`, or undefined while it
   * is still being fetched. Undefined renders the shell: blocking on the
   * projection would make a slow provider probe look like a broken app, and
   * the overlay appears the moment the answer arrives.
   */
  onboarding?: OnboardingState;
  onEstablishIdentity?: (displayName: string) => void | Promise<void>;
  onRecheckOnboarding?: () => void | Promise<void>;
  /**
   * A published release newer than the running shell, or undefined when there
   * is none to offer. Undefined is also the answer outside a bundle, where
   * there is nothing an install could replace.
   */
  update?: UpdateOffer;
  onInstallUpdate?: () => void | Promise<void>;
};

export function App(props: AppProps) {
  return (
    <AppProvider state={props.state}>
      <Shell />
      {/*
        Over the shell, not instead of it — onboarding gates the application
        rather than being a place inside it, and the surfaces behind stay
        mounted so nothing reloads when the last gap closes.
      */}
      {/*
        Offered over the shell too, but not a gate — the user keeps working
        and relaunches when it suits them.
      */}
      <Show when={props.update}>
        <UpdateBanner
          offer={props.update!}
          onInstall={() => props.onInstallUpdate?.()}
        />
      </Show>
      <Show when={props.onboarding && !props.onboarding.complete}>
        <FirstRun
          state={props.onboarding!}
          onEstablishIdentity={(name) => props.onEstablishIdentity?.(name)}
          onRecheck={() => props.onRecheckOnboarding?.()}
        />
      </Show>
    </AppProvider>
  );
}
