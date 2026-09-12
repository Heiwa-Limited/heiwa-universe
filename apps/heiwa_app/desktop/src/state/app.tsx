import {
  createContext,
  createMemo,
  createSignal,
  useContext,
  type Accessor,
  type ParentProps,
} from "solid-js";
import type { SurfaceId } from "../surfaces/ids";
import { createHerdState, type HerdState, type HerdStateOptions } from "./herd";
import { createOperatorState, type OperatorState, type OperatorStateOptions } from "./operator";
import { createRuntimeState, type RuntimeState, type RuntimeStateOptions } from "./runtime";
import { createSessionState, type SessionState } from "./sessions";
import type { SessionStateOptions } from "./sessions";
import type { SubApp } from "./types";

/**
 * The typed interface every surface consumes. Surfaces read this instead of
 * reaching into module globals, which is what made the pre-Solid shell one
 * file: any surface could touch any other surface's state.
 */
export type AppState = {
  operator: OperatorState;
  runtime: RuntimeState;
  herd: HerdState;
  sessions: SessionState;
  view: Accessor<SurfaceId>;
  navigate: (view: SurfaceId) => void;
  selectedProjectId: Accessor<string | undefined>;
  selectProject: (projectId: string) => void;
  /** Static capability descriptors; these never imply a live agent or pane. */
  subApps: Accessor<SubApp[]>;
};

export type AppStateOptions = {
  operator?: OperatorStateOptions;
  runtime?: RuntimeStateOptions;
  herd?: HerdStateOptions;
  sessions?: Omit<Partial<SessionStateOptions>, "start" | "dispose">;
  initialView?: SurfaceId;
  initialSelectedSessionId?: string;
};

export function createAppState(options: AppStateOptions = {}): AppState {
  const operator = createOperatorState(options.operator);
  const sessions = createSessionState({
    ...options.sessions,
    start: operator.start,
    dispose: operator.dispose,
    initialSelectedId: options.initialSelectedSessionId,
  });
  const runtime = createRuntimeState(options.runtime);
  const herd = createHerdState(options.herd);
  const [view, setView] = createSignal<SurfaceId>(options.initialView ?? "home");
  const [selectedProjectId, setSelectedProjectId] = createSignal<string | undefined>();

  // Memoized: several surfaces render <For> over slices of this, and rebuilding
  // the array on every read tears down and recreates those rows on each
  // inbox poll.
  const subApps = createMemo<SubApp[]>(() => [
    {
      id: "ai",
      title: "AI Ops",
      server: "heiwa runtime",
      state: runtimeStatusLabel(runtime),
      skills: ["route", "summarize", "delegate"],
      tools: ["provider adapters", "local models", "receipts"],
      personalization: ["cheapest acceptable route", "repo truth first"],
    },
    {
      id: "calendar",
      title: "Calendar",
      server: "calendar sub-app",
      state: `${runtime.calendarEvents().length} items`,
      skills: ["schedule", "conflict check", "draft holds"],
      tools: ["calendar.read", "calendar.draft", "life state"],
      personalization: ["recovery floor", "no-overlap days"],
    },
    {
      id: "mail",
      title: "Mail",
      server: "mail sub-app",
      state: `${runtime.inbox().length} inbox rows`,
      skills: ["triage", "draft replies", "extract asks"],
      tools: ["mail.search", "mail.draft", "approval outbox"],
      personalization: ["draft first", "no external send without approval"],
    },
    {
      id: "finance",
      title: "Finance",
      server: "finance sub-app",
      state: "read model pending",
      skills: ["cashflow", "debt plan", "receipt audit"],
      tools: ["local docs", "calculators", "approval ledger"],
      personalization: ["no money movement"],
    },
    {
      id: "social",
      title: "Social",
      server: "social sub-app",
      state: "ingress pending",
      skills: ["context read", "draft", "boundary check"],
      tools: ["message read models", "draft outbox", "receipts"],
      personalization: ["respect non-reciprocity"],
    },
    {
      id: "files",
      title: "Files",
      server: "files sub-app",
      state: "workspace",
      skills: ["search", "index", "source cite"],
      tools: ["repo.grep", "fs.read", "artifact log"],
      personalization: ["smallest source slice", "evidence before claim"],
    },
  ]);

  return { operator, runtime, herd, sessions, view, navigate: setView, selectedProjectId, selectProject: setSelectedProjectId, subApps };
}

function runtimeStatusLabel(runtime: RuntimeState): string {
  const health = runtime.health();
  if (!health) return "checking";
  if (!health.reachable) return "offline";
  return health.snapshot?.data?.status ?? "ok";
}

const AppContext = createContext<AppState>();

export function AppProvider(props: ParentProps<{ state: AppState }>) {
  return <AppContext.Provider value={props.state}>{props.children}</AppContext.Provider>;
}

export function useApp(): AppState {
  const state = useContext(AppContext);
  if (!state) throw new Error("useApp must be called inside <AppProvider>");
  return state;
}
