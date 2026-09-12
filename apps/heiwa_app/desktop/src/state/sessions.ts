import { createMemo, createSignal, type Accessor } from "solid-js";
import type { OperatorCatalogResponse, OperatorProject, OperatorThreadSummary } from "../operator/types";
import { apiGet, apiPost } from "../runtime";

export type SessionState = {
  threads: Accessor<OperatorThreadSummary[]>;
  projects: Accessor<OperatorProject[]>;
  loading: Accessor<boolean>;
  error: Accessor<string | undefined>;
  truncated: Accessor<boolean>;
  selectedId: Accessor<string | undefined>;
  draft: Accessor<string>;
  load: () => Promise<void>;
  refresh: () => Promise<void>;
  select: (threadId: string) => Promise<void>;
  setDraft: (value: string) => void;
  setDraftFor: (threadId: string, value: string) => void;
  clearDraftIfUnchanged: (threadId: string, submitted: string) => void;
  createThread: (title?: string, projectId?: string | null) => Promise<OperatorThreadSummary>;
  createProject: (title: string) => Promise<OperatorProject>;
  renameProject: (projectId: string, title: string) => Promise<void>;
  archiveProject: (projectId: string) => Promise<void>;
  restoreProject: (projectId: string) => Promise<void>;
  renameThread: (threadId: string, title: string) => Promise<void>;
  moveThread: (threadId: string, projectId: string | null) => Promise<void>;
  archiveThread: (threadId: string) => Promise<void>;
  restoreThread: (threadId: string) => Promise<void>;
};

export type SessionStateOptions = {
  get?: (path: string) => Promise<OperatorCatalogResponse>;
  post?: (path: string, body: unknown) => Promise<{ ok: boolean; data: { thread?: OperatorThreadSummary; project?: OperatorProject } }>;
  start: (threadId: string) => Promise<void>;
  dispose?: () => void;
  initialSelectedId?: string;
};

function message(error: unknown): string {
  return error instanceof Error ? error.message : "Sessions could not be loaded.";
}

export function createSessionState(options: SessionStateOptions): SessionState {
  const get = options.get ?? apiGet<OperatorCatalogResponse>;
  const post = options.post ?? apiPost;
  const [threads, setThreads] = createSignal<OperatorThreadSummary[]>([]);
  const [projects, setProjects] = createSignal<OperatorProject[]>([]);
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string>();
  const [truncated, setTruncated] = createSignal(false);
  const [selectedId, setSelectedId] = createSignal<string | undefined>(options.initialSelectedId);
  const [drafts, setDrafts] = createSignal<Record<string, string>>({});
  const draft = createMemo(() => drafts()[selectedId() ?? ""] ?? "");

  let catalogGeneration = 0;
  const invalidateCatalog = (): void => {
    catalogGeneration += 1;
    setLoading(false);
  };

  const select = async (threadId: string): Promise<void> => {
    if (!threads().some((thread) => thread.thread_id === threadId && !thread.archived)) return;
    setSelectedId(threadId);
    await options.start(threadId);
  };

  const loadCatalog = async (restoreSelection: boolean): Promise<void> => {
    const generation = ++catalogGeneration;
    setLoading(true);
    setError(undefined);
    try {
      const response = await get("/api/v1/operator/catalog");
      if (generation !== catalogGeneration) return;
      if (!response?.ok || !Array.isArray(response.data?.threads) || !Array.isArray(response.data?.projects)) {
        throw new Error("The runtime returned an invalid session catalog.");
      }
      const preferred = selectedId();
      const previous = threads().find((thread) => thread.thread_id === preferred);
      // A bounded page omitting an observed session does not revoke it.
      const retained = response.data.truncated && previous && !previous.archived
        && !response.data.threads.some((thread) => thread.thread_id === preferred);
      const nextThreads = retained ? [...response.data.threads, previous] : response.data.threads;
      const available = nextThreads.filter((thread) => !thread.archived);
      setThreads(nextThreads);
      setProjects(response.data.projects);
      setTruncated(Boolean(response.data.truncated));
      const target = available.find((thread) => thread.thread_id === preferred) ?? available[0];
      if (target && (restoreSelection || target.thread_id !== preferred)) await select(target.thread_id);
      else if (!target && (!response.data.truncated || nextThreads.some((thread) => thread.thread_id === preferred && thread.archived))) {
        setSelectedId(undefined);
        options.dispose?.();
      }
    } catch (cause) {
      if (generation === catalogGeneration) setError(message(cause));
    } finally {
      if (generation === catalogGeneration) setLoading(false);
    }
  };

  const createThread = async (title?: string, projectId: string | null = null): Promise<OperatorThreadSummary> => {
    const response = await post("/api/v1/operator/threads", { title, project_id: projectId });
    const thread = response?.data?.thread;
    if (!response?.ok || !thread?.thread_id) throw new Error("The runtime did not create a session.");
    invalidateCatalog();
    setThreads((current) => [thread, ...current.filter((item) => item.thread_id !== thread.thread_id)]);
    await select(thread.thread_id);
    return thread;
  };

  const createProject = async (title: string): Promise<OperatorProject> => {
    const response = await post("/api/v1/operator/projects", { title });
    const project = response?.data?.project;
    if (!response?.ok || !project?.project_id) throw new Error("The runtime did not create a project.");
    invalidateCatalog();
    setProjects((current) => [...current.filter((item) => item.project_id !== project.project_id), project]);
    return project;
  };

  const updateThread = async (threadId: string, body: Record<string, unknown>): Promise<void> => {
    const response = await post(`/api/v1/operator/threads/${encodeURIComponent(threadId)}/metadata`, body);
    const thread = response?.data?.thread;
    if (!response?.ok || !thread?.thread_id) throw new Error("The runtime did not update this session.");
    invalidateCatalog();
    setThreads((current) => current.map((item) => item.thread_id === threadId ? thread : item));
    if (thread.archived && selectedId() === threadId) {
      const next = threads().find((item) => !item.archived);
      if (next) await select(next.thread_id);
      else {
        setSelectedId(undefined);
        options.dispose?.();
      }
    }
  };

  const updateProject = async (projectId: string, body: Record<string, unknown>): Promise<void> => {
    const response = await post(`/api/v1/operator/projects/${encodeURIComponent(projectId)}/metadata`, body);
    const project = response?.data?.project;
    if (!response?.ok || !project?.project_id) throw new Error("The runtime did not update this project.");
    invalidateCatalog();
    setProjects((current) => current.map((item) => item.project_id === projectId ? project : item));
  };

  return {
    threads,
    projects,
    loading,
    error,
    truncated,
    selectedId,
    draft,
    load: () => loadCatalog(true),
    refresh: () => loadCatalog(false),
    select,
    setDraft: (value) => setDrafts((current) => ({ ...current, [selectedId() ?? ""]: value })),
    setDraftFor: (threadId, value) => setDrafts((current) => ({ ...current, [threadId]: value })),
    clearDraftIfUnchanged: (threadId, submitted) => setDrafts((current) => current[threadId] === submitted ? { ...current, [threadId]: "" } : current),
    createThread,
    createProject,
    renameProject: (projectId, title) => updateProject(projectId, { title }),
    archiveProject: (projectId) => updateProject(projectId, { archived: true }),
    restoreProject: (projectId) => updateProject(projectId, { archived: false }),
    renameThread: (threadId, title) => updateThread(threadId, { title }),
    moveThread: (threadId, projectId) => updateThread(threadId, { project_id: projectId }),
    archiveThread: (threadId) => updateThread(threadId, { archived: true }),
    restoreThread: (threadId) => updateThread(threadId, { archived: false }),
  };
}
