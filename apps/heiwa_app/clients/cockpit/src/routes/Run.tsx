import type { JSX } from "solid-js";
import { For, Show, createMemo, createResource, createSignal } from "solid-js";
import { v1 } from "../lib/endpoints";
import type { FileTreeEntry } from "../lib/types";

function formatBytes(bytes: number | null): string {
  if (bytes === null) return "—";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function entryTone(entry: FileTreeEntry): string {
  if (entry.kind === "directory") return "directory";
  if (["rs", "ts", "tsx", "js", "json", "md", "toml"].some((ext) => entry.name.endsWith(`.${ext}`))) {
    return "source";
  }
  return "file";
}

export default function RunRoute(): JSX.Element {
  const [treePath, setTreePath] = createSignal<string | undefined>("~/heiwa-universe");
  const [selectedPath, setSelectedPath] = createSignal<string | null>(null);
  const [draftUrl, setDraftUrl] = createSignal("https://example.com");
  const [browserUrl, setBrowserUrl] = createSignal("https://example.com");

  const [tree, { refetch: refetchTree }] = createResource(treePath, (path) => v1.filesTree(path));
  const [preview] = createResource(selectedPath, async (path) => (path ? v1.filePreview(path) : null));
  const [probe] = createResource(browserUrl, (url) => v1.browserProbe(url));

  const directories = createMemo(() => tree()?.entries.filter((entry) => entry.kind === "directory") ?? []);
  const files = createMemo(() => tree()?.entries.filter((entry) => entry.kind !== "directory") ?? []);

  const openEntry = (entry: FileTreeEntry) => {
    setSelectedPath(entry.path);
    if (entry.kind === "directory") {
      setTreePath(entry.path);
      void refetchTree();
    }
  };

  return (
    <section class="run-shell">
      <div class="run-hero">
        <div>
          <p class="eyebrow">Main app view</p>
          <h1>Run Heiwa with browser, files, and evidence in one workspace.</h1>
          <p class="lede">
            Dashboard/debug stays advanced. This is the daily operating surface: web context,
            local filesystem context, preview, and one command lane.
          </p>
        </div>
        <div class="run-mode-stack">
          <span class="status-pill online">browser built in</span>
          <span class="status-pill secure">read-only files</span>
          <span class="status-pill active">Tauri-ready</span>
        </div>
      </div>

      <div class="run-grid">
        <article class="browser-workbench">
          <div class="surface-title-row">
            <div>
              <span class="widget-label">Browser</span>
              <h3>{probe()?.host ?? "Embedded web context"}</h3>
            </div>
            <span class="status-pill online">webview</span>
          </div>
          <form
            class="browser-bar"
            onSubmit={(event) => {
              event.preventDefault();
              setBrowserUrl(draftUrl());
            }}
          >
            <input
              value={draftUrl()}
              onInput={(event) => setDraftUrl(event.currentTarget.value)}
              spellcheck={false}
              aria-label="Browser URL"
            />
            <button type="submit">Go</button>
          </form>
          <div class="browser-frame-shell">
            <Show
              when={probe()?.url}
              fallback={<p class="muted">Enter a URL to open a browser view.</p>}
            >
              {(url) => <iframe title="Heiwa browser" src={url()} sandbox="allow-forms allow-same-origin allow-scripts allow-popups" />}
            </Show>
          </div>
          <p class="muted browser-note">
            Some sites block embedding; in packaged Heiwa.app this is still the first-class browser
            lane and can graduate to native Tauri WebView windows.
          </p>
        </article>

        <article class="files-workbench">
          <div class="surface-title-row">
            <div>
              <span class="widget-label">Filesystem tree</span>
              <h3>{tree()?.path ?? "Loading workspace…"}</h3>
            </div>
            <button class="btn-workspace-action" type="button" onClick={() => void refetchTree()}>
              Refresh
            </button>
          </div>
          <div class="file-tree-toolbar">
            <button
              type="button"
              disabled={!tree()?.parent}
              onClick={() => tree()?.parent && setTreePath(tree()?.parent ?? undefined)}
            >
              Parent
            </button>
            <span>{tree()?.entries.length ?? 0} items</span>
            <Show when={tree()?.truncated}><span>truncated</span></Show>
          </div>
          <div class="file-tree-list">
            <For each={directories()}>
              {(entry) => (
                <button type="button" class={`file-tree-row ${entryTone(entry)}`} onClick={() => openEntry(entry)}>
                  <span>▸ {entry.name}</span>
                  <small>directory</small>
                </button>
              )}
            </For>
            <For each={files()}>
              {(entry) => (
                <button type="button" class={`file-tree-row ${entryTone(entry)}`} onClick={() => openEntry(entry)}>
                  <span>{entry.name}</span>
                  <small>{formatBytes(entry.size_bytes)}</small>
                </button>
              )}
            </For>
          </div>
        </article>

        <article class="preview-workbench">
          <div class="surface-title-row">
            <div>
              <span class="widget-label">File preview</span>
              <h3>{preview()?.name ?? selectedPath() ?? "Select a file"}</h3>
            </div>
            <Show when={preview()}>
              {(item) => <span class="status-pill secure">{item().kind}</span>}
            </Show>
          </div>
          <Show
            when={preview()}
            fallback={<p class="muted">Choose a file from the tree to preview it without editing.</p>}
          >
            {(item) => (
              <div class="preview-panel">
                <p class="muted mono">{item().path}</p>
                <Show when={item().message}><p>{item().message}</p></Show>
                <Show when={item().binary}><p class="muted">Binary file. Text preview withheld.</p></Show>
                <Show when={item().content}>
                  {(content) => <pre>{content()}</pre>}
                </Show>
                <Show when={item().truncated}>
                  <p class="muted">Preview truncated at {formatBytes(item().limit ?? null)}.</p>
                </Show>
              </div>
            )}
          </Show>
        </article>
      </div>
    </section>
  );
}
