import { createEffect, createSignal, onCleanup, For, Show } from "solid-js";
import { apiGet, chooseWorkspace } from "../../runtime";
import { useApp } from "../../state/app";
import type { SurfaceModule } from "../types";

type Workspace = { workspace: string; entries: Array<{ name: string; kind: string }>; truncated: boolean };
function FilesSurface() {
  const app = useApp();
  const [choosing, setChoosing] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const endpoint = () => app.sessions.selectedId() ? `/api/v1/operator/threads/${encodeURIComponent(app.sessions.selectedId()!)}/workspace` : undefined;
  const [workspace, setWorkspace] = createSignal<Workspace>();
  const [loading, setLoading] = createSignal(false);
  let generation = 0;
  const load = async (path = endpoint()) => {
    const current = ++generation;
    setWorkspace(undefined); setError(undefined);
    if (!path) { setLoading(false); return; }
    setLoading(true);
    try {
      const response = await apiGet<{ ok: boolean; data: Workspace }>(path);
      if (!response.ok || typeof response.data?.workspace !== "string" || !Array.isArray(response.data.entries)) throw new Error("invalid workspace response");
      if (current === generation) setWorkspace(response.data);
    } catch { if (current === generation) setError("The workspace is unavailable. Choose its folder again."); }
    finally { if (current === generation) setLoading(false); }
  };
  createEffect(() => { void load(endpoint()); });
  onCleanup(() => { generation += 1; });
  const choose = async () => {
    const threadId = app.sessions.selectedId(); if (!threadId || choosing()) return;
    setChoosing(true); setError(undefined);
    try { await chooseWorkspace(threadId); await load(); }
    catch { setError("The folder could not be selected. Check that Heiwa setup is complete and try again."); }
    finally { setChoosing(false); }
  };
  return <div class="view files-view">
    <div class="view-header"><h2>Files</h2><p class="muted">Files available to this conversation.</p></div>
    <div class="panel">
      <Show when={app.sessions.selectedWork()}><p class="quiet">Work uses its registered repository. This folder selection applies to ordinary chat.</p></Show>
      <header><strong>Conversation workspace</strong><button class="btn-secondary" disabled={!endpoint() || choosing() || !!app.sessions.selectedWork()} onClick={() => void choose()}>{choosing() ? "Choosing…" : "Choose folder…"}</button><button class="btn-secondary" disabled={loading()} onClick={() => void load()}>Refresh</button></header>
      <Show when={loading()}><p role="status">Loading workspace…</p></Show>
      <Show when={error()}><p role="alert">{error() ?? "The workspace is unavailable. Choose its folder again."}</p></Show>
      <Show when={workspace()}>{(data) => <>
        <p class="quiet">{data().workspace}</p>
        <p class="quiet">Your next message can read these files and create new artifacts here. Process actions require approval.</p>
        <ul class="workspace-files"><For each={data().entries}>{(entry) => <li><span>{entry.kind === "directory" ? "▸" : "·"}</span> {entry.name}</li>}</For></ul>
        <Show when={!data().entries.length}><p>This workspace is empty.</p></Show>
        <Show when={data().truncated}><p class="quiet">Showing the first 100 entries. Ask Heiwa to inspect a specific directory.</p></Show>
      </>}</Show>
      <Show when={!endpoint()}><p>Open a conversation to choose its workspace.</p></Show>
    </div>
  </div>;
}
export const filesSurface: SurfaceModule = {
  id: "files", label: "Files", glyph: "□", caption: "files window", Component: FilesSurface,
  preview: () => ({ title: "Files", lines: ["conversation workspace", "native folder selection"] }),
};
