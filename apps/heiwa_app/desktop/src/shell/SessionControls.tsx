import { For, Show, createSignal, onCleanup, onMount, type JSX } from "solid-js";
import type { OperatorProject, OperatorThreadSummary } from "../operator/types";
import { useApp } from "../state/app";
import { Icon } from "./Icon";

type DialogState =
  | { kind: "create-project" }
  | { kind: "rename-project"; project: OperatorProject }
  | { kind: "rename-session"; thread: OperatorThreadSummary }
  | { kind: "move-session"; thread: OperatorThreadSummary };

type MenuAction = {
  label: string;
  run: () => void | Promise<unknown>;
};

export function CreateProjectControl(props: { children?: JSX.Element; ariaLabel?: string; class?: string }) {
  const [dialog, setDialog] = createSignal<DialogState>();
  let trigger!: HTMLButtonElement;
  const close = () => {
    setDialog(undefined);
    queueMicrotask(() => trigger.focus());
  };

  return <>
    <button ref={trigger} class={props.class} aria-label={props.ariaLabel} aria-haspopup="dialog" onClick={() => setDialog({ kind: "create-project" })}>
      {props.children ?? <><Icon name="plus" size={16} /> New project</>}
    </button>
    <Show when={dialog()}>{(current) => <ResourceDialog state={current()} onClose={close} />}</Show>
  </>;
}

export function ProjectActions(props: { project: OperatorProject }) {
  const app = useApp();
  const [dialog, setDialog] = createSignal<DialogState>();
  const actions = (): MenuAction[] => props.project.archived
    ? [{ label: "Restore", run: () => app.sessions.restoreProject(props.project.project_id) }]
    : [
        { label: "Rename", run: () => { setDialog({ kind: "rename-project", project: props.project }); } },
        { label: "Archive", run: () => app.sessions.archiveProject(props.project.project_id) },
      ];

  return <ResourceMenu
    label={`Project actions for ${props.project.title}`}
    focusKey={`project:${props.project.project_id}`}
    actions={actions()}
    dialog={dialog()}
    onDialogClose={() => setDialog(undefined)}
  />;
}

export function SessionActions(props: { thread: OperatorThreadSummary }) {
  const app = useApp();
  const title = () => props.thread.title?.trim() || "Untitled session";
  const [dialog, setDialog] = createSignal<DialogState>();
  const actions = (): MenuAction[] => props.thread.archived
    ? [{ label: "Restore", run: () => app.sessions.restoreThread(props.thread.thread_id) }]
    : [
        { label: "Rename", run: () => { setDialog({ kind: "rename-session", thread: props.thread }); } },
        { label: "Move", run: () => { setDialog({ kind: "move-session", thread: props.thread }); } },
        { label: "Archive", run: () => app.sessions.archiveThread(props.thread.thread_id) },
      ];

  return <ResourceMenu
    label={`Session actions for ${title()}`}
    focusKey={`session:${props.thread.thread_id}`}
    actions={actions()}
    dialog={dialog()}
    onDialogClose={() => setDialog(undefined)}
  />;
}

function ResourceMenu(props: {
  label: string;
  focusKey: string;
  actions: MenuAction[];
  dialog?: DialogState;
  onDialogClose: () => void;
}) {
  const [open, setOpen] = createSignal(false);
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal<string>();
  let root!: HTMLDivElement;
  let trigger!: HTMLButtonElement;
  let menu!: HTMLDivElement;

  const focusTrigger = () => queueMicrotask(() => {
    const current = [...document.querySelectorAll<HTMLButtonElement>("[data-resource-actions]")]
      .find((button) => button.dataset.resourceActions === props.focusKey);
    (current ?? trigger).focus();
  });
  const close = (restoreFocus = true) => {
    setOpen(false);
    if (restoreFocus) focusTrigger();
  };
  const openMenu = () => {
    if (pending()) return;
    setError(undefined);
    setOpen(true);
    queueMicrotask(() => menu.querySelector<HTMLButtonElement>('[role="menuitem"]')?.focus());
  };
  const run = async (action: MenuAction) => {
    if (pending()) return;
    setPending(true);
    setError(undefined);
    try {
      await action.run();
      setOpen(false);
      if (!props.dialog) focusTrigger();
    } catch (cause) {
      setOpen(false);
      setError(cause instanceof Error ? cause.message : "Could not save this change.");
      focusTrigger();
    } finally {
      setPending(false);
    }
  };
  const onDocumentPointer = (event: MouseEvent) => {
    if (open() && !root.contains(event.target as Node)) setOpen(false);
  };
  onMount(() => document.addEventListener("mousedown", onDocumentPointer));
  onCleanup(() => document.removeEventListener("mousedown", onDocumentPointer));

  const moveFocus = (event: KeyboardEvent) => {
    const items = [...menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)')];
    const current = items.indexOf(document.activeElement as HTMLButtonElement);
    const target = event.key === "Home" ? 0
      : event.key === "End" ? items.length - 1
      : event.key === "ArrowDown" ? (current + 1) % items.length
      : event.key === "ArrowUp" ? (current - 1 + items.length) % items.length
      : -1;
    if (target >= 0) {
      event.preventDefault();
      items[target]?.focus();
    }
  };

  return <div class="resource-actions" ref={root}>
    <button
      ref={trigger}
      class="resource-actions-trigger"
      data-resource-actions={props.focusKey}
      aria-label={props.label}
      aria-haspopup="menu"
      aria-expanded={open()}
      disabled={pending()}
      onClick={() => open() ? close() : openMenu()}
      onKeyDown={(event) => {
        if (event.key === "ArrowDown" || event.key === "ArrowUp") {
          event.preventDefault();
          openMenu();
        }
      }}
    ><Icon name="more" size={15} /></button>
    <Show when={open()}>
      <div
        ref={menu}
        class="resource-actions-menu"
        role="menu"
        aria-label={props.label}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            close();
          } else if (event.key === "Tab") {
            setOpen(false);
          } else moveFocus(event);
        }}
      >
        <For each={props.actions}>{(action) =>
          <button role="menuitem" disabled={pending()} onClick={() => void run(action)}>{action.label}</button>
        }</For>
      </div>
    </Show>
    <Show when={error()}><p class="resource-actions-error" role="alert">{error()}</p></Show>
    <Show when={props.dialog}>{(dialog) =>
      <ResourceDialog state={dialog()} onClose={() => { props.onDialogClose(); focusTrigger(); }} />
    }</Show>
  </div>;
}

function ResourceDialog(props: { state: DialogState; onClose: () => void }) {
  const app = useApp();
  const [pending, setPending] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [validation, setValidation] = createSignal<string>();
  let dialog!: HTMLDivElement;
  let titleInput!: HTMLInputElement;
  let projectSelect!: HTMLSelectElement;

  const isMove = () => props.state.kind === "move-session";
  const heading = () => {
    if (props.state.kind === "create-project") return "New project";
    if (props.state.kind === "rename-project") return "Rename project";
    if (props.state.kind === "rename-session") return "Rename session";
    return "Move session";
  };
  const initialTitle = () => {
    if (props.state.kind === "rename-project") return props.state.project.title;
    if (props.state.kind === "rename-session") return props.state.thread.title ?? "";
    return "";
  };
  const save = async () => {
    if (pending()) return;
    let action: Promise<unknown>;
    if (props.state.kind === "move-session") {
      action = app.sessions.moveThread(props.state.thread.thread_id, projectSelect.value || null);
    } else {
      const title = titleInput.value.trim();
      if (!title) {
        setValidation("Enter a name.");
        titleInput.focus();
        return;
      }
      if (title.length > 120) {
        setValidation("Use 120 characters or fewer.");
        titleInput.focus();
        return;
      }
      setValidation(undefined);
      if (props.state.kind === "create-project") action = app.sessions.createProject(title);
      else if (props.state.kind === "rename-project") action = app.sessions.renameProject(props.state.project.project_id, title);
      else action = app.sessions.renameThread(props.state.thread.thread_id, title);
    }
    setPending(true);
    setError(undefined);
    try {
      await action;
      props.onClose();
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Could not save this change.");
    } finally {
      setPending(false);
    }
  };
  const focusable = () => [...dialog.querySelectorAll<HTMLElement>('input, select, button:not(:disabled), [tabindex]:not([tabindex="-1"])')];
  const onKeyDown = (event: KeyboardEvent) => {
    if (event.key === "Escape" && !pending()) {
      event.preventDefault();
      props.onClose();
      return;
    }
    if (event.key !== "Tab") return;
    const items = focusable();
    if (!items.length) return;
    const first = items[0];
    const last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };
  onMount(() => (isMove() ? projectSelect : titleInput).focus());

  return <div ref={dialog} class="resource-dialog" role="dialog" aria-modal="true" aria-labelledby="resource-dialog-title" onKeyDown={onKeyDown}>
    <form onSubmit={(event) => { event.preventDefault(); void save(); }}>
      <h2 id="resource-dialog-title">{heading()}</h2>
      <Show when={isMove()} fallback={
        <label>Name<input ref={titleInput} value={initialTitle()} maxlength={120} aria-describedby={validation() ? "resource-dialog-validation" : undefined} /></label>
      }>
        <label>Project<select ref={projectSelect} value={props.state.kind === "move-session" ? props.state.thread.project_id ?? "" : ""}>
          <option value="">Standalone</option>
          <For each={app.sessions.projects().filter((project) => !project.archived)}>{(project) =>
            <option value={project.project_id}>{project.title}</option>
          }</For>
        </select></label>
      </Show>
      <Show when={validation()}><p id="resource-dialog-validation" class="resource-dialog-error" role="alert">{validation()}</p></Show>
      <Show when={error()}><p class="resource-dialog-error" role="alert">{error()}</p></Show>
      <div class="resource-dialog-actions">
        <button type="button" disabled={pending()} onClick={props.onClose}>Cancel</button>
        <button type="submit" disabled={pending()}>{pending() ? "Saving…" : isMove() ? "Move" : "Save"}</button>
      </div>
    </form>
  </div>;
}
