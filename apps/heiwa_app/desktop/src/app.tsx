import { createEffect, createSignal, on, Show } from "solid-js";
import { Dynamic } from "solid-js/web";
import { AppProvider, useApp, type AppState } from "./state/app";
import { Composer } from "./shell/Composer";
import { FirstRun } from "./shell/FirstRun";
import { Rail } from "./shell/Rail";
import { Icon } from "./shell/Icon";
import { MachinePerspective } from "./surfaces/home/MachinePerspective";
import { UpdateBanner } from "./shell/UpdateBanner";
import { assertRegistryComplete, surfaceById } from "./surfaces/registry";
import type { OnboardingState } from "./state/types";
import type { UpdateOffer } from "./runtime";
import "./theme/tokens.css";
import "./theme/base.css";
import "./shell/shell.css";

assertRegistryComplete();

function Shell(props: { onResources?: () => void; blocked?: boolean }) {
  const app = useApp();
  const active = () => surfaceById(app.view());
  const [creating, setCreating] = createSignal(false);
  const [createError, setCreateError] = createSignal<string>();
  const location = () => app.view() === "ai"
    ? app.sessions.threads().find((thread) => thread.thread_id === app.sessions.selectedId())?.title || "New conversation"
    : app.view() === "projects"
      ? app.sessions.projects().find((project) => project.project_id === app.selectedProjectId())?.title || "Projects"
      : active().label;
  const newSession = async () => {
    if (creating()) return;
    setCreating(true);
    setCreateError(undefined);
    try {
      const project = app.view() === "projects"
        ? app.sessions.projects().find((item) => item.project_id === app.selectedProjectId() && !item.archived)
        : undefined;
      await app.sessions.createThread(undefined, project?.project_id);
      app.navigate("ai");
    } catch (error) {
      setCreateError(error instanceof Error ? error.message : "This session could not be created.");
    } finally { setCreating(false); }
  };

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
    <div class="app-shell" inert={props.blocked || undefined}>
      <Rail onNavigate={(surface) => app.navigate(surface.id)} onResources={props.onResources} />
      <main class="main-area">
        <header class="app-toolbar" data-tauri-drag-region>
          <span>{location()}</span>
          <button disabled={creating()} onClick={() => void newSession()}><Icon name="plus" size={16} />New session</button>
        </header>
        <Show when={createError()}><p class="toolbar-error" role="alert">{createError()}</p></Show>
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
  onVerifyProviders?: () => void | Promise<void>;
  onCompleteWorkspace?: () => void | Promise<void>;
  onOpenResourceGuide?: (id: string) => void | Promise<void>;
  onboardingError?: string;
  /**
   * A published release newer than the running shell, or undefined when there
   * is none to offer. Undefined is also the answer outside a bundle, where
   * there is nothing an install could replace.
   */
  update?: UpdateOffer;
  onInstallUpdate?: () => void | Promise<void>;
};

export function App(props: AppProps) {
  const [resourcesOpen, setResourcesOpen] = createSignal(false);
  const setupNeeded = () => props.onboarding && (props.onboarding.workspace
    ? !props.onboarding.workspace.setup_complete
    : !props.onboarding.complete);
  const finish = async () => {
    if (!props.onCompleteWorkspace) throw new Error("Workspace setup is unavailable in this host.");
    await props.onCompleteWorkspace();
    setResourcesOpen(false);
  };
  return (
    <AppProvider state={props.state}>
      <Shell blocked={Boolean(setupNeeded() || resourcesOpen())} onResources={props.onboarding?.workspace ? () => setResourcesOpen(true) : undefined} />
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
      <Show when={props.onboardingError}>
        <div class="setup-error" role="alert">
          <strong>Workspace setup could not be loaded.</strong> {props.onboardingError}
          <button onClick={() => void Promise.resolve(props.onRecheckOnboarding?.()).catch(() => undefined)}>Retry setup</button>
        </div>
      </Show>
      <Show when={props.onboarding && (setupNeeded() || resourcesOpen())}>
        <FirstRun
          deviceDetails={<MachinePerspective />}
          state={props.onboarding!}
          onEstablishIdentity={(name) => props.onEstablishIdentity?.(name)}
          onRecheck={() => props.onRecheckOnboarding?.()}
          onVerifyProviders={props.onVerifyProviders}
          onCompleteWorkspace={props.onCompleteWorkspace ? finish : undefined}
          onOpenResourceGuide={props.onOpenResourceGuide}
          onClose={!setupNeeded() ? () => setResourcesOpen(false) : undefined}
          onOpenSurface={async (surface) => {
            if (setupNeeded()) await finish();
            props.state.navigate(surface);
            setResourcesOpen(false);
          }}
        />
      </Show>
    </AppProvider>
  );
}
