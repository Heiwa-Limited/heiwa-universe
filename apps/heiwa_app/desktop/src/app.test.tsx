// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { App } from "./app";
import { localIsoDate } from "./lib/format";
import { AppProvider, createAppState, type AppState } from "./state/app";
import { SURFACES } from "./surfaces/registry";
import type { OperatorFrame } from "./operator/types";
import { MachinePerspective } from "./surfaces/home/MachinePerspective";

afterEach(cleanup);

const EMPTY_HISTORY = {
  ok: true,
  data: { events: [], next_cursor: null, skipped_lines: 0 },
};

type Harness = {
  state: AppState;
  post: ReturnType<typeof vi.fn>;
  runtimePost: ReturnType<typeof vi.fn>;
  emit: (frame: OperatorFrame) => void;
};

/**
 * Builds app state with every runtime dependency faked, so the shell is
 * exercised without a live runtime. `schedule` runs inline: the production
 * scheduler defers publishes to an animation frame, which a test would have
 * to wait on.
 */
function harness(
  overrides: {
    subscribeNever?: boolean;
    get?: (path: string) => Promise<unknown>;
    machineOs?: string;
    machineName?: string;
    machineRecognitionError?: { code: string; message: string };
    machineSyncStatus?: string;
    emptyHerd?: boolean;
    readAppleMail?: () => Promise<{ fetched: number; appended: number; deduplicated: number }>;
  } = {},
): Harness {
  const post = vi.fn().mockResolvedValue({
    ok: true,
    data: {
      thread_id: "default",
      turn_id: "turn-1",
      cursor: "1",
      duplicate: false,
      stream_url: "/stream",
    },
  });
  const runtimePost = vi.fn().mockResolvedValue({ ok: true, data: {} });
  let emit: (frame: OperatorFrame) => void = () => {};

  const state = createAppState({
    operator: {
      get: vi.fn().mockResolvedValue(EMPTY_HISTORY),
      post,
      subscribe: (_thread, _after, onFrame) => {
        emit = onFrame;
        return overrides.subscribeNever ? new Promise<void>(() => {}) : Promise.resolve();
      },
      randomUUID: () => "req-1",
      schedule: (task) => task(),
    },
    runtime: {
      get: overrides.get
        ? (vi.fn(overrides.get) as never)
        : vi.fn().mockResolvedValue({ data: { items: [], holds: [], events: [] } }),
      health: vi.fn().mockResolvedValue({
        reachable: true,
        error: null,
        snapshot: {
          ok: true,
          data: {
            // The shape the runtime actually returns. The old fixture put
            // the version at the top level, where nothing sends it, so the
            // reader could look in the wrong place and still pass.
            runtime: { version: "0.1.0-test", status: "ok" },
            machine: {
              schema_version: "heiwa_machine_v1",
              device_id: "mac-test-device",
              display_name: overrides.machineName ?? "dmac.local",
              hostname: overrides.machineName ?? "dmac.local",
              os: overrides.machineOs ?? "macos",
              arch: "aarch64",
              device_class: "full_node",
              hardware: {
                logical_cpu_count: 12,
                memory_total_bytes: 25_769_803_776,
                cpu_model: "Apple M4 Pro",
                hardware_model: "Mac16,8",
              },
              perspective: {
                locality: "local",
                execution_scope: "this_device",
                data_scope: "shared_user",
                sync_status: overrides.machineSyncStatus ?? "local_only",
              },
              recognition_error: overrides.machineRecognitionError,
            },
            resource: {
              snapshot: {
                cpu_count: 12,
                free_memory_bytes: 12_884_901_888,
                load_1m: 1.2,
                battery_percent: 100,
                on_battery: false,
                thermal_pressure: "nominal",
              },
            },
            workers: {
              total: 1,
              live: 1,
              stale: 0,
              runtime_live: 1,
              task_live: 0,
            },
            providers: [
              {
                provider_id: "ollama",
                display_name: "Ollama",
                status: "connected",
                auth_kind: "local_runtime",
                default_model: null,
                supported_lanes: [],
                last_error: null,
                last_validated_at: null,
              },
            ],
          },
        },
      }),
      post: runtimePost,
      readAppleMail: overrides.readAppleMail ?? vi.fn().mockRejectedValue(new Error("Apple Mail read not configured in this test.")),
    },
    herd: {
      snapshot: vi.fn().mockResolvedValue({
        status: "online",
        source: "test",
        panes: overrides.emptyHerd
          ? []
          : [
              {
                workspace: "heiwa",
                pane: "heiwa:ops",
                agent: "multiplexer",
                state: "running",
                cwd: "/work",
              },
            ],
        error: null,
      }),
      catalog: vi.fn().mockResolvedValue([]),
      read: vi.fn().mockResolvedValue({
        ok: true,
        pane: "heiwa:ops",
        text: "pane output",
        source: "test",
        error: null,
      }),
    },
  });

  return { state, post, runtimePost, emit: (frame) => emit(frame) };
}

/**
 * A distinctive string each surface must put on screen when mounted.
 *
 * These must not collide with rail content: the rail renders every surface's
 * `preview().title` regardless of which surface is active, so keying Mail on
 * "Mail" passed even with the component gutted. Each marker below is body
 * copy only that surface renders.
 */
const SURFACE_MARKERS: Record<string, string | RegExp> = {
  home: "What’s on your mind?",
  sessions: "Every conversation",
  projects: "Select a project from the sidebar.",
  ai: "No messages yet.",
  windows: "Terminal panes",
  calendar: "Upcoming",
  approvals: "Pending decisions",
  // Mail now renders the local snapshot rather than an L3 placeholder.
  mail: /metadata only, read\s+from this machine/,
  finance: /Read model arrives with the L3 connector plane/,
  social: /Ingress arrives with the L3 connector plane/,
  workers: "Operator turns",
  browser: "Go",
  files: "Workspace tree",
};

describe("shell", () => {
  it("registers the primary surfaces including in-app approvals", () => {
    expect(SURFACES.map((surface) => surface.id)).toEqual([
      "home",
      "sessions",
      "projects",
      "ai",
      "windows",
      "calendar",
      "approvals",
      "mail",
      "finance",
      "social",
      "workers",
      "browser",
      "files",
    ]);
  });

  it("decides a pending dispatch approval through the runtime service", async () => {
    const { state, runtimePost } = harness({
      get: async (path) => {
        if (path === "/api/v1/approvals/summary") {
          return {
            data: {
              pending_count: 1,
              pending: [
                {
                  id: "req_calendar_1",
                  action: "calendar-event-create",
                  target: "calendar:hold-1",
                  risk: "T2",
                  requested_at: "2026-08-21T00:00:00Z",
                },
              ],
              requests_dir: "/state/dispatch/requests",
              decisions_dir: "/state/dispatch/approvals/decisions",
            },
          };
        }
        return { data: { items: [], holds: [], events: [] } };
      },
    });
    state.navigate("approvals");
    render(() => <App state={state} />);

    await Promise.resolve();
    await Promise.resolve();
    screen.getByRole("button", { name: /approve req_calendar_1/i }).click();
    await Promise.resolve();

    expect(runtimePost).toHaveBeenCalledWith(
      "/api/v1/approvals/req_calendar_1/approve",
      {},
    );
  });

  it("keeps Apple Calendar disconnected until the user connects it", async () => {
    const { state, runtimePost } = harness({
      get: async (path) => {
        if (path === "/api/v1/calendar/resources") {
          return {
            data: {
              source: "apple_calendar",
              status: "disconnected",
              calendars: [],
              detail: "Detected on this Mac, but not connected to this Heiwa profile.",
              next_action: "heiwa connect apple-calendar --authorize",
            },
          };
        }
        return { data: { items: [], holds: [], events: [] } };
      },
    });
    state.navigate("calendar");
    render(() => <App state={state} />);

    await Promise.resolve();
    await Promise.resolve();
    screen.getByRole("button", { name: /connect apple calendar/i }).click();
    await Promise.resolve();

    expect(runtimePost).toHaveBeenCalledWith(
      "/api/v1/connectors/apple_calendar/connect",
      {},
    );
  });

  it("stages an Apple event for approval from the native Calendar surface", async () => {
    const { state, runtimePost } = harness({
      get: async (path) => {
        if (path === "/api/v1/calendar/resources") {
          return {
            data: {
              source: "apple_calendar",
              status: "ready",
              calendars: [{ name: "Calendar", writable: true }],
              detail: "Connected to this Heiwa profile on this device.",
            },
          };
        }
        return { data: { items: [], holds: [], events: [] } };
      },
    });
    state.navigate("calendar");
    render(() => <App state={state} />);
    await Promise.resolve();
    await Promise.resolve();

    fireEvent.input(screen.getByLabelText("Event title"), {
      target: { value: "Call mom" },
    });
    fireEvent.input(screen.getByLabelText("Event date"), {
      target: { value: "2026-08-22" },
    });
    fireEvent.input(screen.getByLabelText("Event start"), {
      target: { value: "15:00" },
    });
    fireEvent.input(screen.getByLabelText("Event end"), {
      target: { value: "15:30" },
    });
    screen.getByRole("button", { name: /stage apple event/i }).click();
    await Promise.resolve();

    expect(runtimePost).toHaveBeenCalledWith("/api/v1/calendar/holds", {
      title: "Call mom",
      date: "2026-08-22",
      start: "15:00",
      end: "15:30",
      kind: "focus",
      promotion: { connector: "apple_calendar", calendar: "Calendar" },
    });
    fireEvent.click(await screen.findByRole("button", { name: "Review pending changes" }));
    expect(state.view()).toBe("approvals");
  });

  it("does not present the live runtime host as a task worker", async () => {
    const { state } = harness();
    await state.runtime.loadHealth();
    const preview = SURFACES.find((surface) => surface.id === "workers")!.preview(state);
    expect(preview.lines).toContain("0 task workers");
  });

  it("does not invent panes or active agents for a fresh profile", async () => {
    const { state } = harness({ emptyHerd: true });
    render(() => <App state={state} />);
    await Promise.resolve();
    await Promise.resolve();

    expect(screen.getByText("A place for your next idea.")).toBeTruthy();
    expect(screen.queryByText(/6 sub-app agents/)).toBeNull();
    expect(screen.queryByText("planned")).toBeNull();

    state.navigate("windows");
    expect(screen.queryByText("Sub-app agents")).toBeNull();
  });

  it("renders the primary rail navigation", () => {
    const { state } = harness();
    render(() => <App state={state} />);
    const labels = [...document.querySelectorAll<HTMLButtonElement>(".sidebar-navlist button")].map((button) => button.textContent);
    expect(labels).toEqual(["Home", "All sessions", "Calendar", "Mail"]);
  });

  it.each(SURFACES.map((surface) => surface.id))("mounts the %s surface", (id) => {
    const { state } = harness();
    state.navigate(id);
    const { container } = render(() => <App state={state} />);
    // Scope to the main area: the rail carries every surface's preview title,
    // so an unscoped query can pass on a surface that rendered nothing.
    const main = container.querySelector(".main-area");
    expect(main).toBeTruthy();
    const marker = SURFACE_MARKERS[id];
    const matcher =
      typeof marker === "string"
        ? new RegExp(marker.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"))
        : marker;
    expect(matcher.test(main!.textContent ?? "")).toBe(true);
  });

  it("shows the active surface caption in the composer", () => {
    const { state } = harness();
    state.navigate("windows");
    render(() => <App state={state} />);
    expect(screen.getByText("Viewing windows")).toBeTruthy();
  });

  it("focuses the composer with Command-L without stealing focus from a dialog", async () => {
    const { state } = harness();
    render(() => <App state={state} />);
    const composer = screen.getByLabelText("Message Heiwa");
    fireEvent.keyDown(document, { key: "l", metaKey: true });
    expect(document.activeElement).toBe(composer);

    fireEvent.click(screen.getByRole("button", { name: "New project" }));
    const name = screen.getByLabelText("Name");
    await waitFor(() => expect(document.activeElement).toBe(name));
    fireEvent.keyDown(document, { key: "l", metaKey: true });
    expect(document.activeElement).toBe(name);
  });

  it("renders runtime health in the composer hint", async () => {
    const { state } = harness();
    render(() => <App state={state} />);
    await state.runtime.loadHealth();
    expect(screen.getByText(/0\.1\.0-test · 1 providers/)).toBeTruthy();
  });
});

describe("operator seam", () => {
  it("keeps session B's same-text draft when session A's submission settles late", async () => {
    let settleA!: () => void;
    const post = vi.fn((path: string) => {
      if (path.endsWith("/turns")) return new Promise((resolve) => { settleA = () => resolve({ ok: true, data: { thread_id: "a", turn_id: "turn-a", cursor: "1", duplicate: false, stream_url: "/stream" } }); });
      return Promise.resolve({ ok: true, data: {} });
    });
    const state = createAppState({
      operator: {
        get: (async () => EMPTY_HISTORY) as never,
        post: post as never,
        subscribe: () => new Promise<void>(() => {}),
        randomUUID: () => "req-a",
        schedule: (task) => task(),
      },
      sessions: {
        get: async () => ({ ok: true, data: { threads: [{ thread_id: "a", title: "A", archived: false }, { thread_id: "b", title: "B", archived: false }], projects: [] } }),
        post: post as never,
      },
    });
    await state.sessions.load();
    render(() => <App state={state} />);
    await Promise.resolve();
    const input = screen.getByLabelText("Message Heiwa") as HTMLTextAreaElement;
    fireEvent.input(input, { target: { value: "same text" } });
    fireEvent.click(screen.getByLabelText("Send"));
    await state.sessions.select("b");
    fireEvent.input(input, { target: { value: "same text" } });
    settleA();
    await Promise.resolve();
    await Promise.resolve();
    expect(input.value).toBe("same text");
    expect(state.sessions.draft()).toBe("same text");
  });

  it("submits through OperatorClient and expands the same conversation on request", async () => {
    const { state, post } = harness();
    render(() => <App state={state} />);
    await state.operator.start("default");
    expect(state.operator.ready()).toBe(true);

    const input = screen.getByLabelText("Message Heiwa") as HTMLTextAreaElement;
    fireEvent.input(input, { target: { value: "hello heiwa" } });
    screen.getByLabelText("Send").click();
    await Promise.resolve();

    expect(post).toHaveBeenCalledWith("/api/v1/operator/threads/default/turns", {
      client_request_id: "req-1",
      prompt: "hello heiwa",
      route_policy: { mode: "auto" },
    });
    expect(state.view()).toBe("home");
    expect(screen.getByRole("region", { name: "Heiwa response" })).toBeTruthy();
    fireEvent.click(screen.getByText("Open conversation"));
    expect(state.view()).toBe("ai");
    expect(post).toHaveBeenCalledTimes(1);
  });

  it("keeps Calendar open while responses stream, close, and reopen", async () => {
    const { state, emit, post } = harness({ subscribeNever: true });
    state.navigate("calendar");
    render(() => <App state={state} />);
    await state.operator.start("default");
    fireEvent.input(screen.getByLabelText("Message Heiwa"), { target: { value: "Plan a focus block" } });
    fireEvent.click(screen.getByLabelText("Send"));
    await waitFor(() => expect(post).toHaveBeenCalledOnce());
    emit({ type: "assistant_delta", thread_id: "default", turn_id: "turn-1", text: "A quiet afternoon" });
    expect(screen.getByText("A quiet afternoon")).toBeTruthy();
    expect(state.view()).toBe("calendar");
    fireEvent.click(screen.getByLabelText("Close response"));
    expect(screen.queryByText("A quiet afternoon")).toBeNull();
    fireEvent.click(screen.getByText("Show conversation"));
    expect(screen.getByText("A quiet afternoon")).toBeTruthy();
    expect(post).toHaveBeenCalledOnce();
  });

  it("retains an unacknowledged draft and shows the submission failure", async () => {
    const { state, post } = harness();
    post.mockRejectedValueOnce(new Error("transport offline"));
    render(() => <App state={state} />);
    await state.operator.start("default");
    const input = screen.getByLabelText("Message Heiwa") as HTMLTextAreaElement;
    fireEvent.input(input, { target: { value: "Keep this draft" } });
    fireEvent.click(screen.getByLabelText("Send"));
    await waitFor(() => expect(screen.getByRole("alert").textContent).toContain("draft is retained"));
    expect(input.value).toBe("Keep this draft");
  });

  it("does not erase the next draft when the previous submission finishes", async () => {
    const { state, post } = harness();
    let accept!: (value: unknown) => void;
    post.mockImplementationOnce(() => new Promise((resolve) => { accept = resolve; }));
    render(() => <App state={state} />);
    await state.operator.start("default");
    const input = screen.getByLabelText("Message Heiwa") as HTMLTextAreaElement;
    fireEvent.input(input, { target: { value: "First message" } });
    fireEvent.click(screen.getByLabelText("Send"));
    fireEvent.input(input, { target: { value: "Next message" } });
    accept({ ok: true, data: { thread_id: "default", turn_id: "turn-1", cursor: "1", duplicate: false, stream_url: "/stream" } });
    await waitFor(() => expect((screen.getByLabelText("Send") as HTMLButtonElement).disabled).toBe(false));
    expect(input.value).toBe("Next message");
  });

  it("persists local entry and allows resource setup to be reopened without a provider", async () => {
    const { state } = harness();
    const [onboarding, setOnboarding] = createSignal({
      complete: false, display_name: "Ada", gaps: [],
      workspace: { can_enter: true, setup_complete: false, resources: [] },
    });
    const finish = vi.fn(async () => setOnboarding((s) => ({ ...s, workspace: { ...s.workspace, setup_complete: true } })));
    render(() => <App state={state} onboarding={onboarding()} onCompleteWorkspace={async () => { await finish(); }} />);
    fireEvent.click(screen.getByText("Enter workspace"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(onboarding().complete).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Your resources" }));
    expect(screen.getByRole("dialog")).toBeTruthy();
    fireEvent.click(screen.getByLabelText("Close resources"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(finish).toHaveBeenCalledOnce();
  });

  it("renders durable events and streaming deltas from the store projection", async () => {
    const { state, emit } = harness({ subscribeNever: true });
    state.navigate("ai");
    render(() => <App state={state} />);
    await state.operator.start("default");

    emit({
      type: "event",
      cursor: "1",
      event: {
        schema_version: 1,
        event_id: "evt-1",
        thread_id: "default",
        turn_id: "turn-1",
        run_id: null,
        call_id: null,
        event_type: "user_message",
        occurred_at: "2026-08-14T00:00:00Z",
        actor: { kind: "user", id: "local" },
        risk_class: "none",
        sensitivity: "normal",
        parent_event_id: null,
        correlation_id: null,
        source_refs: [],
        evidence_refs: [],
        payload: { text: "what changed today?" },
      },
    });
    expect(screen.getByText("what changed today?")).toBeTruthy();

    emit({
      type: "assistant_delta",
      thread_id: "default",
      turn_id: "turn-1",
      text: "three receipts",
    });
    expect(screen.getByText("three receipts")).toBeTruthy();
  });

  it("disables the composer until the stream is ready", () => {
    const { state } = harness();
    render(() => <App state={state} />);
    expect((screen.getByLabelText("Send") as HTMLButtonElement).disabled).toBe(true);
  });

  it("covers the shell with first run until onboarding is complete", () => {
    // Onboarding gates the whole application, so it is an overlay rather
    // than an eleventh surface — there is nothing useful to navigate to
    // before a provider exists.
    const { state } = harness();
    render(() => (
      <App
        state={state}
        onboarding={{
          complete: false,
          display_name: null,
          gaps: [
            {
              step: "provider",
              detail: "no provider account is connected",
              remedy: "add a key with `heiwa auth add-key <provider> <key>`",
            },
          ],
        }}
      />
    ));

    expect(screen.getByText("Set up Heiwa")).toBeTruthy();
    expect(screen.getByText("no provider account is connected")).toBeTruthy();
  });

  it("shows every gap with the action that closes it", () => {
    const { state } = harness();
    render(() => (
      <App
        state={state}
        onboarding={{
          complete: false,
          display_name: null,
          gaps: [
            { step: "identity", detail: "no local identity yet", remedy: "run heiwa setup" },
            { step: "provider", detail: "no provider connected", remedy: "add a key" },
          ],
        }}
      />
    ));

    for (const text of ["no local identity yet", "run heiwa setup", "no provider connected", "add a key"]) {
      expect(screen.getByText(text)).toBeTruthy();
    }
  });

  it("gets out of the way once onboarding is complete", () => {
    const { state } = harness();
    render(() => (
      <App state={state} onboarding={{ complete: true, display_name: "Ada", gaps: [] }} />
    ));

    expect(screen.queryByText("Set up Heiwa")).toBeNull();
    expect(screen.getByLabelText("Send")).toBeTruthy();
  });

  it("renders the shell when onboarding state has not arrived yet", () => {
    // The projection is fetched asynchronously. Blocking the shell on it
    // would make a slow provider probe look like a broken application.
    const { state } = harness();
    render(() => <App state={state} />);

    expect(screen.queryByText("Set up Heiwa")).toBeNull();
    expect(screen.getByLabelText("Send")).toBeTruthy();
  });

  it("shows the messages the local mail snapshot actually holds", () => {
    // The pipeline existed and the surface ignored it: `heiwa mail scan`
    // writes a metadata snapshot from the user's own Mail.app, the runtime
    // serves it at /api/v1/mail/summary, and the surface was rendering a
    // "reads land on L3" placeholder while real data sat one call away.
    const { state } = harness({
      get: async (path: string) =>
        path === "/api/v1/mail/summary"
          ? {
              data: {
                priority: [
                  { sender: "ada@example.com", subject: "Re: launch", unread: true, account: "Work" },
                  { sender: "grace@example.com", subject: "Invoice", unread: false, account: "Work" },
                ],
              },
            }
          : { data: {} },
    });

    render(() => <App state={state} />);
    state.navigate("mail");

    return Promise.resolve().then(() => {
      expect(screen.getByText("Re: launch")).toBeTruthy();
      expect(screen.getByText("ada@example.com")).toBeTruthy();
    });
  });

  it("offers a native Apple Mail read for an empty snapshot", () => {
    const { state } = harness({ get: async () => ({ data: { priority: [] } }) });

    render(() => <App state={state} />);
    state.navigate("mail");

    return Promise.resolve().then(() => {
      expect(screen.getByRole("button", { name: "Read Apple Mail" })).toBeTruthy();
      expect(screen.queryByText(/heiwa mail scan/)).toBeNull();
    });
  });

  it("reads Apple Mail only after the button is clicked and refreshes the snapshot", async () => {
    let finishRead!: (result: { fetched: number; appended: number; deduplicated: number }) => void;
    const readAppleMail = vi.fn(() => new Promise<{ fetched: number; appended: number; deduplicated: number }>((resolve) => {
      finishRead = resolve;
    }));
    const { state } = harness({
      readAppleMail,
      get: async (path) => path === "/api/v1/mail/summary"
        ? { data: { priority: [{ sender: "ada@example.com", subject: "Fresh mail", unread: true, date: "2026-09-11T12:00:00Z" }] } }
        : { data: {} },
    });
    state.navigate("mail");
    render(() => <App state={state} />);
    expect(readAppleMail).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "Read Apple Mail" }));
    const pending = screen.getByRole("button", { name: "Reading…" }) as HTMLButtonElement;
    expect(pending.disabled).toBe(true);
    fireEvent.click(pending);
    expect(readAppleMail).toHaveBeenCalledOnce();
    finishRead({ fetched: 2, appended: 1, deduplicated: 1 });
    expect(await screen.findByText("Fresh mail")).toBeTruthy();
    expect((await screen.findByRole("status")).textContent).toContain("Read 2 headers; 1 added");
  });

  it("keeps existing mail visible and allows retry after a native read failure", async () => {
    // Tauri serializes a Rust `Result::Err(String)` as a rejected string.
    const readAppleMail = vi.fn().mockRejectedValue("Automation access is required.");
    const { state } = harness({
      readAppleMail,
      get: async (path) => path === "/api/v1/mail/summary"
        ? { data: { priority: [{ sender: "ada@example.com", subject: "Existing mail", unread: false, date: "2026-09-11T12:00:00Z" }] } }
        : { data: {} },
    });
    state.navigate("mail");
    render(() => <App state={state} />);
    expect(await screen.findByText("Existing mail")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Read Apple Mail" }));

    expect((await screen.findByRole("alert")).textContent).toContain("Automation access is required.");
    expect(screen.getByText("Existing mail")).toBeTruthy();
    expect((screen.getByRole("button", { name: "Read Apple Mail" }) as HTMLButtonElement).disabled).toBe(false);
  });

  it("opens on a briefing of what today actually holds", () => {
    // The point of having the calendar and the mail locally is that the app
    // can answer "what do I need to know right now" the moment it opens,
    // without the user going and looking in two places.
    // Local, matching what a machine's calendar hands back.
    const today = localIsoDate();
    const { state } = harness({
      get: async (path: string) => {
        if (path === "/api/v1/calendar/summary") {
          return {
            data: {
              events: [
                { id: "e1", title: "Standup", date: today, start: "09:30" },
                { id: "e2", title: "Design review", date: today, start: "14:00" },
                { id: "e3", title: "Next week thing", date: "2099-01-01", start: "10:00" },
              ],
            },
          };
        }
        if (path === "/api/v1/mail/summary") {
          return {
            data: {
              priority: [
                { sender: "ada@example.com", subject: "Re: launch", unread: true },
                { sender: "grace@example.com", subject: "Invoice", unread: false },
              ],
            },
          };
        }
        return { data: {} };
      },
    });

    render(() => <App state={state} />);

    return Promise.resolve()
      .then(() => Promise.resolve())
      .then(() => {
        const briefing = document.querySelector(".today-briefing");
        expect(briefing).toBeTruthy();
        const text = briefing!.textContent ?? "";
        // Today's events only — a briefing that includes next week is a list,
        // not a briefing.
        expect(text).toContain("Standup");
        expect(text).not.toContain("Next week thing");
        expect(text).toContain("1 unread");
      });
  });

  it("renders this Mac perspective without pretending peer sync exists", async () => {
    const { state } = harness();
    await state.runtime.loadHealth();
    render(() => <AppProvider state={state}><MachinePerspective /></AppProvider>);
    const text = document.querySelector(".machine-perspective")!.textContent ?? "";
    expect(text).toContain("This Mac");
    expect(text).toContain("dmac.local");
    expect(text).toContain("Apple M4 Pro");
    expect(text).toContain("12 cores");
    expect(text).toContain("Shared data");
    expect(text).toContain("sync local only");
  });

  it("does not claim peer sync when the mesh state could not be read", async () => {
    const { state } = harness({ machineSyncStatus: "unknown" });
    await state.runtime.loadHealth();
    render(() => <AppProvider state={state}><MachinePerspective /></AppProvider>);
    const text = document.querySelector(".machine-perspective")?.textContent ?? "";
    expect(text).not.toContain("peer enrolled");
    expect(text).not.toContain("sync local only");
    expect(text).toContain("sync state unavailable");
  });

  it("renders the same shared-data client from a Windows-local perspective", async () => {
    const { state } = harness({ machineOs: "windows", machineName: "devon-windows" });
    await state.runtime.loadHealth();
    render(() => <AppProvider state={state}><MachinePerspective /></AppProvider>);
    const text = document.querySelector(".machine-perspective")?.textContent ?? "";
    expect(text).toContain("This Windows PC");
    expect(text).toContain("devon-windows");
    expect(text).toContain("Shared data");
    expect(text).toContain("sync local only");
  });

  it("explains an incompatible machine manifest without exposing its contents", async () => {
    const { state } = harness({
      machineRecognitionError: {
        code: "unsupported_schema",
        message: "Machine identity was written by a newer or incompatible Heiwa build.",
      },
    });

    await state.runtime.loadHealth();
    render(() => <AppProvider state={state}><MachinePerspective /></AppProvider>);
    const perspective = document.querySelector(".machine-perspective");
    expect(perspective?.textContent).toContain("Device recognition needs attention");
    expect(perspective?.textContent).toContain(
      "Machine identity was written by a newer or incompatible Heiwa build.",
    );
  });

  it("does not show an empty briefing dashboard", () => {
    const { state } = harness({ get: async () => ({ data: { events: [], priority: [] } }) });

    render(() => <App state={state} />);

    return Promise.resolve()
      .then(() => Promise.resolve())
      .then(() => {
        expect(document.querySelector(".today-briefing")).toBeNull();
      });
  });

  it("offers a published update and installs it on the user's word", async () => {
    // Registering the updater plugin only makes a release fetchable. This is
    // the reachable path: without a rendered offer wired to the install
    // command, a signed release sits on GitHub and every shell stays stale.
    const { state } = harness();
    const onInstallUpdate = vi.fn().mockResolvedValue(undefined);

    render(() => (
      <App
        state={state}
        update={{ version: "0.2.0", current_version: "0.1.0" }}
        onInstallUpdate={onInstallUpdate}
      />
    ));

    const banner = document.querySelector(".update-banner");
    expect(banner?.textContent ?? "").toContain("0.2.0");

    const action = screen.getByRole("button", { name: /install and relaunch/i });
    action.click();
    await Promise.resolve();

    expect(onInstallUpdate).toHaveBeenCalledTimes(1);
  });

  it("stays quiet when no update is published", () => {
    const { state } = harness();

    render(() => <App state={state} />);

    expect(document.querySelector(".update-banner")).toBeNull();
  });

  it("keeps the offer on screen with the reason when installing fails", async () => {
    // A banner that disappears on failure leaves the user believing they
    // updated when they did not.
    const { state } = harness();
    const onInstallUpdate = vi.fn().mockRejectedValue(new Error("signature rejected"));

    render(() => (
      <App
        state={state}
        update={{ version: "0.2.0", current_version: "0.1.0" }}
        onInstallUpdate={onInstallUpdate}
      />
    ));

    screen.getByRole("button", { name: /install and relaunch/i }).click();
    await Promise.resolve();
    await Promise.resolve();

    const banner = document.querySelector(".update-banner");
    expect(banner?.textContent ?? "").toContain("signature rejected");
  });
});
