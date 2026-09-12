// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
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
