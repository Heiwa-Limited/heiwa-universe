// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { OperatorProject, OperatorThreadSummary } from "../operator/types";
import { AppProvider, createAppState } from "../state/app";
import { CreateProjectControl } from "./SessionControls";
import { Rail } from "./Rail";

afterEach(cleanup);

const threads: OperatorThreadSummary[] = [
  { thread_id: "standalone", title: "Standalone work", archived: false, project_id: null },
  { thread_id: "project-session", title: "Project work", archived: false, project_id: "project-1" },
  { thread_id: "archived-session", title: "Old work", archived: true, project_id: null },
];
const projects: OperatorProject[] = [
  { project_id: "project-1", title: "Alpha", archived: false },
  { project_id: "archived-project", title: "Old project", archived: true },
];

function setup(options: { rejectCreate?: boolean } = {}) {
  const post = vi.fn(async (path: string, body: unknown): Promise<{ ok: boolean; data: { project?: OperatorProject; thread?: OperatorThreadSummary } }> => {
    const metadata = body as { title?: string; project_id?: string | null; archived?: boolean };
    if (path === "/api/v1/operator/projects") {
      if (options.rejectCreate) throw new Error("Project name is already in use.");
      return { ok: true, data: { project: { project_id: "project-new", title: metadata.title!, archived: false } } };
    }
    const projectMatch = path.match(/^\/api\/v1\/operator\/projects\/([^/]+)\/metadata$/);
    if (projectMatch) {
      const previous = projects.find((project) => project.project_id === projectMatch[1])!;
      return { ok: true, data: { project: { ...previous, title: metadata.title ?? previous.title, archived: metadata.archived ?? previous.archived } } };
    }
    const threadMatch = path.match(/^\/api\/v1\/operator\/threads\/([^/]+)\/metadata$/);
    if (threadMatch) {
      const previous = threads.find((thread) => thread.thread_id === threadMatch[1])!;
      return { ok: true, data: { thread: { ...previous, ...metadata } } };
    }
    throw new Error(`Unexpected POST ${path}`);
  });
  const state = createAppState({
    operator: {
      get: async () => ({ ok: true, data: { events: [], next_cursor: null, skipped_lines: 0 } }),
      post: vi.fn() as never,
      subscribe: () => Promise.resolve(),
      schedule: (task) => task(),
    },
    sessions: {
      get: async () => ({ ok: true, data: { threads, projects } }),
      post,
    },
  });
  return { state, post };
}

async function openMenu(label: string) {
  fireEvent.click(screen.getByRole("button", { name: label }));
  return screen.getByRole("menu", { name: label });
}

describe("session and project controls", () => {
  it("calls the real session service for every lifecycle action", async () => {
    const { state, post } = setup();
    await state.sessions.load();
    render(() => <AppProvider state={state}><Rail onNavigate={() => undefined}/></AppProvider>);

    fireEvent.click(screen.getByRole("button", { name: "New project" }));
    fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Gamma" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/operator/projects", { title: "Gamma" }));

    await openMenu("Session actions for Standalone work");
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Renamed work" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/operator/threads/standalone/metadata", { title: "Renamed work" }));

    await openMenu("Session actions for Renamed work");
    fireEvent.click(screen.getByRole("menuitem", { name: "Move" }));
    fireEvent.change(screen.getByLabelText("Project"), { target: { value: "project-1" } });
    fireEvent.click(screen.getByRole("button", { name: "Move" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/operator/threads/standalone/metadata", { project_id: "project-1" }));

    await openMenu("Session actions for Project work");
    fireEvent.click(screen.getByRole("menuitem", { name: "Archive" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/operator/threads/project-session/metadata", { archived: true }));

    await openMenu("Session actions for Old work");
    fireEvent.click(screen.getByRole("menuitem", { name: "Restore" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/operator/threads/archived-session/metadata", { archived: false }));

    await openMenu("Project actions for Alpha");
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Beta" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/operator/projects/project-1/metadata", { title: "Beta" }));

    await openMenu("Project actions for Beta");
    fireEvent.click(screen.getByRole("menuitem", { name: "Archive" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/operator/projects/project-1/metadata", { archived: true }));

    await openMenu("Project actions for Old project");
    fireEvent.click(screen.getByRole("menuitem", { name: "Restore" }));
    await waitFor(() => expect(post).toHaveBeenCalledWith("/api/v1/operator/projects/archived-project/metadata", { archived: false }));
  });

  it("validates titles, traps dialog focus, closes on Escape, and restores focus", async () => {
    const { state, post } = setup();
    render(() => <AppProvider state={state}><CreateProjectControl/></AppProvider>);
    const trigger = screen.getByRole("button", { name: "New project" });
    fireEvent.click(trigger);
    const input = screen.getByLabelText("Name");
    const save = screen.getByRole("button", { name: "Save" });

    fireEvent.input(input, { target: { value: "   " } });
    fireEvent.click(save);
    expect(screen.getByRole("alert").textContent).toContain("Enter a name.");
    expect(post).not.toHaveBeenCalled();

    save.focus();
    fireEvent.keyDown(save, { key: "Tab" });
    expect(document.activeElement).toBe(input);
    fireEvent.keyDown(input, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(save);

    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(document.activeElement).toBe(trigger);
  });

  it("closes a keyboard-opened menu on Escape and restores its trigger", async () => {
    const { state } = setup();
    await state.sessions.load();
    render(() => <AppProvider state={state}><Rail onNavigate={() => undefined}/></AppProvider>);
    const trigger = screen.getByRole("button", { name: "Session actions for Standalone work" });
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    const menu = screen.getByRole("menu", { name: "Session actions for Standalone work" });
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("menuitem", { name: "Rename" })));
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    await waitFor(() => expect(document.activeElement).toBe(trigger));
  });

  it("keeps the menu mounted through a targetless focus change for assistive activation", async () => {
    const { state } = setup();
    await state.sessions.load();
    render(() => <AppProvider state={state}><Rail onNavigate={() => undefined}/></AppProvider>);
    const label = "Session actions for Standalone work";
    const menu = await openMenu(label);
    fireEvent.focusOut(menu, { relatedTarget: null });
    expect(screen.getByRole("menu", { name: label })).toBeTruthy();
    fireEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    expect(screen.getByRole("dialog", { name: "Rename session" })).toBeTruthy();
  });

  it("keeps runtime mutation errors visible in the dialog", async () => {
    const { state } = setup({ rejectCreate: true });
    render(() => <AppProvider state={state}><CreateProjectControl/></AppProvider>);
    fireEvent.click(screen.getByRole("button", { name: "New project" }));
    fireEvent.input(screen.getByLabelText("Name"), { target: { value: "Alpha" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));
    expect((await screen.findByRole("alert")).textContent).toContain("Project name is already in use.");
    expect(screen.getByRole("dialog")).toBeTruthy();
  });
});
