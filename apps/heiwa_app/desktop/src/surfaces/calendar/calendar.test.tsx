// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { afterEach, expect, it, vi } from "vitest";
import { createRoot } from "solid-js";
import { AppProvider, createAppState } from "../../state/app";
import { calendarSurface } from "./index";

let dispose: (() => void) | undefined;
afterEach(() => { cleanup(); dispose?.(); vi.useRealTimers(); });

it("keeps month results chronological and excludes events from other months", async () => {
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(new Date(2026, 8, 11, 12));
  const state = createRoot((cleanupRoot) => {
    dispose = cleanupRoot;
    return createAppState({
    runtime: {
      get: async <T,>(path: string): Promise<T> => ({ data: path.endsWith("/summary") ? {
        events: [
          { id: "next", date: "2026-10-02", title: "October review" },
          { id: "later", date: "2026-09-28", title: "September wrap" },
          { id: "earlier", date: "2026-09-12", title: "September planning" },
        ],
      } : {} }) as T,
    },
    });
  });
  await state.runtime.loadCalendar();
  const Calendar = calendarSurface.Component;
  const { container } = render(() => <AppProvider state={state}><Calendar /></AppProvider>);
  expect(screen.queryByText("October review")).toBeNull();
  expect(Array.from(container.querySelectorAll(".cal-event-title"), (node) => node.textContent))
    .toEqual(["September planning", "September wrap"]);
  fireEvent.click(screen.getByRole("button", { name: "Next month" }));
  expect(screen.getByText("October review")).toBeTruthy();
  expect(screen.queryByText("September wrap")).toBeNull();
});


it("imports only explicitly selected calendar identities and shows the loaded events", async () => {
  const now = new Date();
  const date = `${now.getFullYear()}-${String(now.getMonth()+1).padStart(2, "0")}-12`;
  let imported = false;
  const post = vi.fn(async <T,>(_path: string, _body: unknown): Promise<T> => {
    imported = true;
    return { data: { fetched: 1, truncated: false } } as T;
  });
  const state = createRoot((cleanupRoot) => {
    dispose = cleanupRoot;
    return createAppState({ runtime: {
      post: async <T,>(path: string, body: unknown): Promise<T> => await post(path, body) as T,
      get: async <T,>(path: string): Promise<T> => ({ data: path.endsWith("/resources") ? {
        source: "apple_calendar", status: "ready", reader_available: true, selected_ids: imported ? ["work-id"] : [],
        calendars: [{ id: "work-id", name: "Work", source: "iCloud", writable: true }, { id: "private-id", name: "Private", writable: true }],
      } : path.endsWith("/summary") ? { events: imported ? [{ id: "actual-event", date, title: "Imported meeting" }] : [] } : {} }) as T,
    } });
  });
  await Promise.all([state.runtime.loadCalendarResources(), state.runtime.loadCalendar()]);
  const Calendar = calendarSurface.Component;
  render(() => <AppProvider state={state}><Calendar /></AppProvider>);
  const read = screen.getByRole("button", { name: "Read selected calendars" });
  expect((read as HTMLButtonElement).disabled).toBe(true);
  expect(post).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("checkbox", { name: "Work · iCloud" }));
  fireEvent.click(read);
  await waitFor(() => expect(screen.getByText("Imported meeting")).toBeTruthy());
  expect(post).toHaveBeenCalledExactlyOnceWith("/api/v1/calendar/read", { calendar_ids: ["work-id"] });
  expect((screen.getByRole("checkbox", { name: "Private" }) as HTMLInputElement).checked).toBe(false);
});
