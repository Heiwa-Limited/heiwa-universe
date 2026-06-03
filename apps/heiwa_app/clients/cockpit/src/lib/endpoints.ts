import { api } from "./api";
import type {
  Agent,
  Approval,
  ApprovalsSummary,
  CellsCatalogEntry,
  Cron,
  Envelope,
  FreshnessReport,
  Health,
  HistorySummary,
  HookProvider,
  HookSummary,
  InboxItem,
  MemoryEntry,
  Mission,
  ProviderLive,
  RateGroup,
  Route,
  Session,
  TodaySnapshot,
  Trace,
} from "./types";

async function unwrap<T>(promise: Promise<Envelope<T>>): Promise<T> {
  const envelope = await promise;
  return envelope.data;
}

export const v1 = {
  session: () => unwrap(api.get<Envelope<Session>>("/api/v1/session")),
  providers: () =>
    unwrap(
      api.get<Envelope<{ providers: ProviderLive[] }>>("/api/v1/providers"),
    ),
  routes: () =>
    unwrap(api.get<Envelope<{ routes: Route[] }>>("/api/v1/routes")),
  missions: (params?: { status?: string; limit?: number }) => {
    const qs = new URLSearchParams();
    if (params?.status) qs.set("status", params.status);
    if (params?.limit) qs.set("limit", String(params.limit));
    const suffix = qs.toString() ? `?${qs.toString()}` : "";
    return unwrap(
      api.get<Envelope<{ missions: Mission[]; cursor: string | null }>>(
        `/api/v1/missions${suffix}`,
      ),
    );
  },
  approvals: () =>
    unwrap(api.get<Envelope<{ approvals: Approval[] }>>("/api/v1/approvals")),
  approvalsSummary: () =>
    unwrap(api.get<Envelope<ApprovalsSummary>>("/api/v1/approvals/summary")),
  lifeToday: () =>
    unwrap(api.get<Envelope<TodaySnapshot>>("/api/v1/life/today")),
  lifeFreshness: () =>
    unwrap(api.get<Envelope<FreshnessReport>>("/api/v1/life/freshness")),
  inbox: () =>
    unwrap(
      api.get<Envelope<{ items: InboxItem[]; cursor: string | null }>>(
        "/api/v1/inbox",
      ),
    ),
  rateGroups: () =>
    unwrap(
      api.get<Envelope<{ rate_groups: RateGroup[] }>>("/api/v1/rate-groups"),
    ),
  history: () => unwrap(api.get<Envelope<HistorySummary>>("/api/v1/history")),
  traces: () =>
    unwrap(api.get<Envelope<{ traces: Trace[] }>>("/api/v1/traces")),
  memory: () =>
    unwrap(api.get<Envelope<{ entries: MemoryEntry[] }>>("/api/v1/memory")),
  agents: () =>
    unwrap(api.get<Envelope<{ agents: Agent[] }>>("/api/v1/agents")),
  crons: () => unwrap(api.get<Envelope<{ crons: Cron[] }>>("/api/v1/crons")),
  hooks: () =>
    unwrap(
      api.get<Envelope<{ providers: HookProvider[]; summary: HookSummary }>>(
        "/api/v1/hooks",
      ),
    ),
  cells: () =>
    unwrap(
      api.get<Envelope<{ cells: CellsCatalogEntry[] }>>(
        "/api/v1/cells/catalog",
      ),
    ),
  health: () => unwrap(api.get<Envelope<Health>>("/status/health")),
};
