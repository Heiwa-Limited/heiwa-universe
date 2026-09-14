import { createMemo, createSignal, type Accessor } from "solid-js";
import { apiGet } from "../runtime";
import {
  IncompatibleWorkPayload,
  catalogEvidence,
  homeOrder,
  isUnfinished,
  parseWorkCatalog,
  parseWorkSurfaces,
  workRecord,
  type CatalogEvidence,
  type WorkCatalog,
  type WorkCatalogRow,
  type WorkSurfaces,
} from "./work-model";

/**
 * The desktop's one authority for Work read models.
 *
 * Home, Work, and Workers select from this service; none of them fetches or
 * folds on its own. The catalog discovers Work; each Work's validated
 * snapshot is kept per Work with its own request generation, so a slow reply
 * lands only on the Work it was for and an older request for that Work can
 * never replace a newer one. Snapshots replace whole.
 *
 * Discovery and detail agree by construction: a catalog row is shown with its
 * snapshot's values whenever that snapshot's durable Work revision is at
 * least the row's. Work revisions are the aggregate's own monotonic counter,
 * so this never compares projection epochs or cursors, and an older catalog
 * cannot downgrade a newer snapshot.
 */

export type WorkLoadErrorKind = "offline" | "not_found" | "incompatible" | "unavailable";

export type WorkLoadError = { kind: WorkLoadErrorKind; message: string };

export type CatalogState = {
  status: "idle" | "loading" | "ready" | "error";
  catalog?: WorkCatalog;
  error?: WorkLoadError;
  /** The catalog shown is from an earlier successful load. */
  stale: boolean;
  refreshing: boolean;
};

export type SnapshotState = {
  workId?: string;
  surfaces?: WorkSurfaces;
  loading: boolean;
  error?: WorkLoadError;
  /** The surfaces shown are from an earlier successful load. */
  stale: boolean;
};

/** A discovered Work, showing its snapshot's values when the snapshot is at least as new. */
export type WorkRow = WorkCatalogRow & { snapshot?: WorkSurfaces };

export type WorkState = {
  catalog: Accessor<CatalogState>;
  /** Catalog rows reconciled with accepted snapshots, in catalog order. */
  rows: Accessor<WorkRow[]>;
  evidence: Accessor<CatalogEvidence>;
  selectedId: Accessor<string | undefined>;
  /** The selected Work's snapshot state. */
  detail: Accessor<SnapshotState>;
  loadCatalog: (options?: { prefetch?: number }) => Promise<void>;
  select: (workId: string) => Promise<void>;
  /** Reload the selected Work. A no-op when nothing is selected. */
  refresh: () => Promise<void>;
  dispose: () => void;
};

export type WorkStateOptions = {
  get?: (path: string) => Promise<unknown>;
};

/** Snapshots kept across Work; the selected one and any in flight are never evicted. */
const RETAINED_SNAPSHOTS = 16;

export function classifyWorkError(error: unknown): WorkLoadError {
  if (error instanceof IncompatibleWorkPayload) {
    return { kind: "incompatible", message: error.message };
  }
  if (typeof error === "object" && error !== null && "kind" in error) {
    const payload = error as { kind: string; detail?: unknown };
    if (payload.kind === "Offline") {
      return { kind: "offline", message: "The Heiwa runtime is not reachable." };
    }
    if (payload.kind === "Http" && typeof payload.detail === "object" && payload.detail !== null) {
      const detail = payload.detail as { status?: number; body?: string };
      if (detail.status === 404 && typeof detail.body === "string" && detail.body.includes("unknown_work")) {
        return { kind: "not_found", message: "The runtime no longer has this Work." };
      }
      return { kind: "unavailable", message: `The runtime could not load Work (HTTP ${detail.status ?? "error"}).` };
    }
    if (payload.kind === "Decode") {
      return { kind: "incompatible", message: "The runtime returned Work data this app cannot read." };
    }
  }
  return { kind: "unavailable", message: "Work could not be loaded." };
}

export function createWorkState(options: WorkStateOptions = {}): WorkState {
  const get = options.get ?? ((path: string) => apiGet<unknown>(path));

  const [catalog, setCatalog] = createSignal<CatalogState>({ status: "idle", stale: false, refreshing: false });
  const [selectedId, setSelectedId] = createSignal<string>();
  const [snapshots, setSnapshots] = createSignal<ReadonlyMap<string, SnapshotState>>(new Map());
  const generations = new Map<string, number>();

  let catalogGeneration = 0;
  let disposed = false;

  const update = (workId: string, next: (current: SnapshotState | undefined) => SnapshotState): void => {
    const selected = selectedId();
    setSnapshots((current) => {
      const map = new Map(current);
      const value = next(map.get(workId));
      map.delete(workId);
      map.set(workId, value);
      for (const [id, entry] of map) {
        if (map.size <= RETAINED_SNAPSHOTS) break;
        if (id !== selected && !entry.loading) map.delete(id);
      }
      return map;
    });
  };

  const fetchSnapshot = async (workId: string): Promise<void> => {
    const generation = (generations.get(workId) ?? 0) + 1;
    generations.set(workId, generation);
    // Keep what is shown, and any error, until a valid reply replaces them.
    update(workId, (current) => ({ ...current, workId, loading: true, stale: current?.stale ?? false }));
    const current = () => !disposed && generations.get(workId) === generation;
    try {
      const payload = await get(`/api/v1/operator/work/${encodeURIComponent(workId)}/surfaces`);
      if (!current()) return;
      const surfaces = parseWorkSurfaces(payload, workId);
      update(workId, () => ({ workId, surfaces, loading: false, stale: false }));
    } catch (cause) {
      if (!current()) return;
      const error = classifyWorkError(cause);
      if (error.kind === "not_found") {
        // Authoritative: the runtime says this Work does not exist.
        update(workId, () => ({ workId, loading: false, error, stale: false }));
        return;
      }
      update(workId, (previous) => ({
        workId,
        surfaces: previous?.surfaces,
        loading: false,
        error,
        stale: Boolean(previous?.surfaces),
      }));
    }
  };

  const rows = createMemo<WorkRow[]>(() => {
    const known = snapshots();
    return (catalog().catalog?.rows ?? []).map((row) => {
      const surfaces = known.get(row.workId)?.surfaces;
      if (!surfaces || surfaces.identity.workRevision < row.revision) return row;
      const record = workRecord(surfaces);
      return {
        ...row,
        intent: record.intent,
        status: record.status,
        rawStatus: record.rawStatus,
        revision: surfaces.identity.workRevision,
        updatedAt: record.updatedAt ?? row.updatedAt,
        snapshot: surfaces,
      };
    });
  });

  const loadCatalog = async (loadOptions: { prefetch?: number } = {}): Promise<void> => {
    if (disposed) return;
    const generation = ++catalogGeneration;
    setCatalog((current) => current.catalog
      ? { ...current, refreshing: true }
      : { status: "loading", error: current.error, stale: false, refreshing: true });
    let parsed: WorkCatalog;
    try {
      parsed = parseWorkCatalog(await get("/api/v1/operator/work"));
    } catch (cause) {
      if (disposed || generation !== catalogGeneration) return;
      const error = classifyWorkError(cause);
      // A failed refresh never turns a known catalog into "no Work".
      setCatalog((current) => current.catalog
        ? { status: "ready", catalog: current.catalog, error, stale: true, refreshing: false }
        : { status: "error", error, stale: false, refreshing: false });
      return;
    }
    if (disposed || generation !== catalogGeneration) return;
    setCatalog({ status: "ready", catalog: parsed, stale: false, refreshing: false });

    const known = snapshots();
    const targets = new Set<string>();
    const selected = selectedId();
    const selectedRow = selected ? parsed.rows.find((row) => row.workId === selected) : undefined;
    const selectedSnapshot = selected ? known.get(selected) : undefined;
    // The catalog proves the selected Work changed since its snapshot.
    if (selectedRow && selectedSnapshot?.surfaces && !selectedSnapshot.loading
      && selectedSnapshot.surfaces.identity.workRevision < selectedRow.revision) {
      targets.add(selectedRow.workId);
    }
    // Home reads each shown Work's Home projection; the fan-out is bounded.
    const prefetch = Math.max(0, loadOptions.prefetch ?? 0);
    for (const row of homeOrder(rows()).slice(0, prefetch)) {
      if (isUnfinished(row.status) && !known.get(row.workId)?.loading) targets.add(row.workId);
    }
    await Promise.all([...targets].map(fetchSnapshot));
  };

  const select = async (workId: string): Promise<void> => {
    if (disposed || !workId) return;
    setSelectedId(workId);
    await fetchSnapshot(workId);
  };

  const refresh = async (): Promise<void> => {
    const workId = selectedId();
    if (disposed || !workId) return;
    await fetchSnapshot(workId);
  };

  const detail = createMemo<SnapshotState>(() => {
    const id = selectedId();
    return (id ? snapshots().get(id) : undefined) ?? { workId: id, loading: false, stale: false };
  });

  return {
    catalog,
    rows,
    evidence: createMemo(() => catalogEvidence(catalog().catalog)),
    selectedId,
    detail,
    loadCatalog,
    select,
    refresh,
    dispose: () => {
      disposed = true;
      catalogGeneration += 1;
    },
  };
}
