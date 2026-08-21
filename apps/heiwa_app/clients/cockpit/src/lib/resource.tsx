import type { JSX, Resource } from "solid-js";
import { createResource, onCleanup, onMount, Show } from "solid-js";
import { openWs } from "./api";

export interface RemoteShellProps<T> {
  loader: () => Promise<T>;
  fallback?: JSX.Element;
  children: (data: T, refetch: () => void) => JSX.Element;
  liveEventFilter?: (event: unknown) => boolean;
}

export function useRemote<T>(
  loader: () => Promise<T>,
): [Resource<T>, { refetch: () => void }] {
  const [data, { refetch }] = createResource(loader);
  return [
    data,
    {
      refetch: () => {
        refetch();
      },
    },
  ];
}

export function RemoteShell<T>(props: RemoteShellProps<T>): JSX.Element {
  const [data, ctl] = useRemote(props.loader);

  onMount(() => {
    if (!props.liveEventFilter) return;
    const ws = openWs("/ws/v1/events");
    const onMessage = (event: MessageEvent) => {
      try {
        const msg: unknown = JSON.parse(event.data);
        if (props.liveEventFilter?.(msg)) {
          ctl.refetch();
        }
      } catch {
        // ignore malformed frames
      }
    };
    ws.addEventListener("message", onMessage);
    onCleanup(() => {
      ws.removeEventListener("message", onMessage);
      ws.close();
    });
  });

  return (
    <Show
      when={!data.loading && !data.error && data() !== undefined}
      fallback={
        <Show when={data.error} fallback={<p class="muted">Loading…</p>}>
          <div class="empty-state">
            <strong>Runtime unreachable.</strong>
            <p class="muted">
              The cockpit couldn't reach <code>heiwa_core</code>. Start the
              runtime with <code>heiwa app</code>, then retry.
            </p>
            <button type="button" class="btn btn-outline" onClick={ctl.refetch}>
              Retry
            </button>
          </div>
        </Show>
      }
    >
      {props.children(data() as T, ctl.refetch)}
    </Show>
  );
}
