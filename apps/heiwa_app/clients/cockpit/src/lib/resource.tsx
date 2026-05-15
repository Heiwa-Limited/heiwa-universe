import type { JSX, Resource } from "solid-js";
import { createResource, Show } from "solid-js";

export interface RemoteShellProps<T> {
  loader: () => Promise<T>;
  fallback?: JSX.Element;
  children: (data: T, refetch: () => void) => JSX.Element;
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
