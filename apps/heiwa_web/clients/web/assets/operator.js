(function () {
  const STORAGE_KEY = "heiwa_operator_token";
  const view = document.body?.dataset?.view || "";
  const state = {
    token: window.localStorage.getItem(STORAGE_KEY) || "",
    socket: null,
  };

  const endpointForView = {
    connections: { method: "GET", path: "/auth/providers", auth: true },
    missions: { method: "GET", path: "/missions", auth: true },
    live: { method: "GET", path: "/missions?status=running&limit=50", auth: true },
    approvals: { method: "GET", path: "/approvals", auth: true },
    "rate-groups": { method: "GET", path: "/rate-groups", auth: true },
    cells: { method: "POST", path: "/call/heiwa_get_cells_catalog", auth: false, body: {} },
    history: { method: "GET", path: "/history", auth: true },
  };

  function escapeHtml(value) {
    return String(value ?? "")
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;");
  }

  function statusClass(status) {
    const normalized = String(status || "").toLowerCase();
    if (["connected", "completed", "pass", "ok", "running", "approved"].includes(normalized)) return "ok";
    if (["waiting_approval", "paused", "pending", "warning", "unsupported", "rejected", "expired"].includes(normalized)) return "warn";
    return "fail";
  }

  function providerLabel(provider) {
    const map = {
      "google-gemini-cli": "Gemini CLI",
      "codex": "Codex",
      "claude-code": "Claude Code",
      "google-antigravity": "Antigravity",
    };
    return map[provider] || provider;
  }

  function authHeaders(required) {
    const headers = { "Content-Type": "application/json" };
    if (required && state.token) {
      headers.Authorization = `Bearer ${state.token}`;
    }
    return headers;
  }

  function baseUrl() {
    return `${window.location.protocol}//${window.location.host}`;
  }

  async function fetchJSON(definition) {
    const response = await fetch(`${baseUrl()}${definition.path}`, {
      method: definition.method || "GET",
      headers: authHeaders(definition.auth !== false),
      body: definition.body ? JSON.stringify(definition.body) : undefined,
      cache: "no-store",
    });
    if (response.status === 401 || response.status === 403) {
      throw new Error("Operator token rejected");
    }
    if (!response.ok) {
      throw new Error(`${response.status} ${response.statusText}`);
    }
    return response.json();
  }

  function setToken() {
    const next = window.prompt("Heiwa operator token", state.token || "");
    if (next === null) return;
    state.token = next.trim();
    if (state.token) {
      window.localStorage.setItem(STORAGE_KEY, state.token);
    } else {
      window.localStorage.removeItem(STORAGE_KEY);
    }
    renderTokenState();
    void refresh();
  }

  function clearToken() {
    state.token = "";
    window.localStorage.removeItem(STORAGE_KEY);
    renderTokenState();
    renderError("Operator token cleared.");
  }

  function renderTokenState() {
    const chip = document.getElementById("token-status");
    if (chip) {
      chip.textContent = state.token ? "token loaded" : "token missing";
      chip.className = `token-chip ${state.token ? "ok" : "warn"}`;
    }
  }

  function mount(html) {
    const root = document.getElementById("operator-root");
    if (root) root.innerHTML = html;
  }

  function renderError(message) {
    mount(`<div class="empty-state">${escapeHtml(message)}</div>`);
  }

  function renderConnections(payload) {
    const providers = payload.providers || [];
    if (!providers.length) {
      return `<div class="empty-state">No provider metadata has been synced yet. Run <code>heiwa auth status</code> locally first.</div>`;
    }
    return `
      <div class="card-list">
        ${providers
          .map(
            (provider) => `
              <article>
                <div class="operator-toolbar">
                  <h3>${escapeHtml(providerLabel(provider.provider_id))}</h3>
                  <span class="pill ${statusClass(provider.status)}">${escapeHtml(provider.status)}</span>
                </div>
                <p><strong>Auth:</strong> ${escapeHtml(provider.auth_kind)}</p>
                <p><strong>Rate Group:</strong> ${escapeHtml(provider.rate_group)}</p>
                <p><strong>Default Model:</strong> <code>${escapeHtml(provider.default_model || "-")}</code></p>
                <p><strong>Validated:</strong> ${escapeHtml(provider.last_validated_at || "never")}</p>
                <p><strong>Error:</strong> ${escapeHtml(provider.last_error || "-")}</p>
              </article>
            `
          )
          .join("")}
      </div>
    `;
  }

  function renderMissions(payload) {
    const missions = payload.missions || [];
    if (!missions.length) {
      return `<div class="empty-state">No missions recorded yet.</div>`;
    }
    return `
      <div class="data-table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>Mission</th>
              <th>Status</th>
              <th>Intent</th>
              <th>Tool</th>
              <th>Model</th>
              <th>Updated</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            ${missions
              .map(
                (mission) => `
                  <tr>
                    <td><code>${escapeHtml(mission.mission_id)}</code><br />${escapeHtml((mission.prompt || "").slice(0, 120))}</td>
                    <td><span class="pill ${statusClass(mission.status)}">${escapeHtml(mission.status)}</span></td>
                    <td>${escapeHtml(mission.intent_class || "-")}</td>
                    <td>${escapeHtml(mission.target_tool || "-")}</td>
                    <td><code>${escapeHtml(mission.target_model || "-")}</code></td>
                    <td>${escapeHtml(mission.updated_at || "-")}</td>
                    <td>
                      <div class="operator-actions">
                        <button class="btn btn-outline" data-action="pause-mission" data-id="${escapeHtml(mission.mission_id)}">Pause</button>
                        <button class="btn btn-outline" data-action="resume-mission" data-id="${escapeHtml(mission.mission_id)}">Resume</button>
                      </div>
                    </td>
                  </tr>
                `
              )
              .join("")}
          </tbody>
        </table>
      </div>
    `;
  }

  function renderLive(snapshot) {
    const tasks = snapshot.live_tasks || (snapshot.missions || []).map((mission) => ({
      task_id: mission.mission_id,
      status: mission.status,
      intent_class: mission.intent_class,
      target_tool: mission.target_tool,
      target_model: mission.target_model,
      progress: mission.summary || mission.error || "",
    }));
    if (!tasks.length) {
      return `<div class="empty-state">No live task activity yet.</div>`;
    }
    return `
      <div class="card-list">
        ${tasks
          .slice()
          .reverse()
          .slice(0, 15)
          .map(
            (task) => `
              <article>
                <div class="operator-toolbar">
                  <h3><code>${escapeHtml(task.task_id)}</code></h3>
                  <span class="pill ${statusClass(task.run_status || task.status)}">${escapeHtml(task.run_status || task.status || "unknown")}</span>
                </div>
                <p><strong>Route:</strong> ${escapeHtml(task.intent_class || task.route?.intent_class || "-")} -> ${escapeHtml(task.target_tool || task.dispatch?.adapter_tool || "-")}</p>
                <p><strong>Model:</strong> <code>${escapeHtml(task.target_model || task.dispatch?.target_model || "-")}</code></p>
                <p><strong>Progress:</strong> ${escapeHtml(task.progress || task.summary || task.message || "-")}</p>
              </article>
            `
          )
          .join("")}
      </div>
    `;
  }

  function renderApprovals(payload) {
    const approvals = payload.approvals || [];
    if (!approvals.length) {
      return `<div class="empty-state">No approvals pending.</div>`;
    }
    return `
      <div class="card-list">
        ${approvals
          .map(
            (approval) => `
              <article>
                <div class="operator-toolbar">
                  <h3><code>${escapeHtml(approval.task_id)}</code></h3>
                  <span class="pill ${statusClass(approval.status)}">${escapeHtml(approval.status)}</span>
                </div>
                <p><strong>Risk:</strong> ${escapeHtml(approval.risk_level || "-")}</p>
                <p><strong>Requested By:</strong> ${escapeHtml(approval.requested_by || "-")}</p>
                <p><strong>Prompt:</strong> ${escapeHtml(approval.raw_text_excerpt || "-")}</p>
                <div class="operator-actions">
                  <button class="btn btn-solid" data-action="approve-task" data-id="${escapeHtml(approval.task_id)}">Approve</button>
                  <button class="btn btn-outline" data-action="reject-task" data-id="${escapeHtml(approval.task_id)}">Reject</button>
                </div>
              </article>
            `
          )
          .join("")}
      </div>
    `;
  }

  function renderRateGroups(payload) {
    const groups = payload.rate_groups || {};
    const entries = Object.entries(groups);
    if (!entries.length) {
      return `<div class="empty-state">No rate group data available.</div>`;
    }
    return `
      <div class="data-table-wrap">
        <table class="data-table">
          <thead>
            <tr>
              <th>Group</th>
              <th>Used</th>
              <th>Remaining</th>
              <th>Reserve</th>
              <th>Surplus</th>
              <th>Cooldown</th>
            </tr>
          </thead>
          <tbody>
            ${entries
              .map(([name, data]) => {
                if (data.unlimited) {
                  return `<tr><td>${escapeHtml(name)}</td><td>-</td><td>∞</td><td>-</td><td>∞</td><td>local</td></tr>`;
                }
                return `
                  <tr>
                    <td>${escapeHtml(name)}</td>
                    <td>${escapeHtml(data.used)}</td>
                    <td>${escapeHtml(data.remaining)}</td>
                    <td>${escapeHtml(data.reserve)}</td>
                    <td>${escapeHtml(data.surplus_headroom)}</td>
                    <td>${escapeHtml(Math.round(data.cooldown_remaining || 0))}s</td>
                  </tr>
                `;
              })
              .join("")}
          </tbody>
        </table>
      </div>
    `;
  }

  function renderCells(payload) {
    const cells = payload.cells || [];
    if (!cells.length) {
      return `<div class="empty-state">No cells catalog available.</div>`;
    }
    return `
      <div class="card-list">
        ${cells
          .map(
            (cell) => `
              <article>
                <div class="operator-toolbar">
                  <h3>${escapeHtml(cell.display_name || cell.cell_id)}</h3>
                  <span class="pill ok">${escapeHtml(cell.gateway_tool || "-")}</span>
                </div>
                <p>${escapeHtml(cell.description || "")}</p>
                <p><strong>Runtime:</strong> ${escapeHtml(cell.target_runtime || "-")}</p>
                <p><strong>Models:</strong> ${escapeHtml(Object.values(cell.models || {}).flat().join(", ") || "-")}</p>
              </article>
            `
          )
          .join("")}
      </div>
    `;
  }

  function renderHistory(payload) {
    const summaries = payload.session_summaries || [];
    const artifacts = payload.artifacts || [];
    return `
      <div class="operator-grid">
        <article class="panel">
          <h2>Session Summaries</h2>
          ${
            summaries.length
              ? `<div class="stack">${summaries
                  .map(
                    (summary) => `<div class="mono-block"><strong>${escapeHtml(summary.created_at || "")}</strong>\n${escapeHtml(summary.summary_text || "")}</div>`
                  )
                  .join("")}</div>`
              : `<div class="empty-state">No compacted session summaries yet.</div>`
          }
        </article>
        <article class="panel">
          <h2>Artifacts</h2>
          ${
            artifacts.length
              ? `<div class="stack">${artifacts
                  .map(
                    (artifact) => `<div class="mono-block"><strong>${escapeHtml(artifact.title || artifact.artifact_id)}</strong>\n${escapeHtml(artifact.created_at || "")}\n${escapeHtml(artifact.path || artifact.uri || "")}</div>`
                  )
                  .join("")}</div>`
              : `<div class="empty-state">No artifacts recorded yet.</div>`
          }
        </article>
      </div>
    `;
  }

  function render(viewName, payload) {
    if (viewName === "connections") return renderConnections(payload);
    if (viewName === "missions") return renderMissions(payload);
    if (viewName === "live") return renderLive(payload);
    if (viewName === "approvals") return renderApprovals(payload);
    if (viewName === "rate-groups") return renderRateGroups(payload);
    if (viewName === "cells") return renderCells(payload.content ? JSON.parse(payload.content[0].text) : payload);
    if (viewName === "history") return renderHistory(payload);
    return `<div class="empty-state">Unknown operator view.</div>`;
  }

  async function refresh() {
    const definition = endpointForView[view];
    if (!definition) return;
    if (definition.auth !== false && !state.token) {
      renderError("Operator token missing. Use Set Token to load the current hub token.");
      return;
    }
    try {
      const payload = await fetchJSON(definition);
      mount(render(view, payload));
    } catch (error) {
      renderError(error.message || String(error));
    }
  }

  async function mutate(path) {
    const response = await fetch(`${baseUrl()}${path}`, {
      method: "POST",
      headers: authHeaders(true),
      body: JSON.stringify({ actor: "operator-app", reason: "operator_app" }),
    });
    if (!response.ok) {
      throw new Error(`${response.status} ${response.statusText}`);
    }
    return response.json();
  }

  async function handleAction(event) {
    const button = event.target.closest("[data-action]");
    if (!button) return;
    const action = button.dataset.action;
    const id = button.dataset.id;
    if (!action) return;
    try {
      if (action === "approve-task") await mutate(`/tasks/${id}/approve`);
      if (action === "reject-task") await mutate(`/tasks/${id}/reject`);
      if (action === "pause-mission") await mutate(`/missions/${id}/pause`);
      if (action === "resume-mission") await mutate(`/missions/${id}/resume`);
      await refresh();
    } catch (error) {
      renderError(error.message || String(error));
    }
  }

  function connectOperatorStream() {
    if (!state.token || !["connections", "missions", "live", "approvals", "rate-groups", "history"].includes(view)) {
      return;
    }
    try {
      if (state.socket) state.socket.close();
      const scheme = window.location.protocol === "https:" ? "wss" : "ws";
      state.socket = new WebSocket(`${scheme}://${window.location.host}/ws/operator?token=${encodeURIComponent(state.token)}`);
      state.socket.addEventListener("message", (event) => {
        const payload = JSON.parse(event.data);
        if (view === "connections") mount(renderConnections({ providers: payload.providers || [] }));
        if (view === "missions") mount(renderMissions({ missions: payload.missions || [] }));
        if (view === "live") mount(renderLive(payload));
        if (view === "approvals") mount(renderApprovals({ approvals: payload.approvals || [] }));
        if (view === "rate-groups") mount(renderRateGroups({ rate_groups: payload.rate_groups || {} }));
        if (view === "history") mount(renderHistory(payload.history || {}));
      });
    } catch {
      // Ignore websocket setup failures and keep polling.
    }
  }

  document.addEventListener("DOMContentLoaded", () => {
    renderTokenState();
    document.getElementById("set-token")?.addEventListener("click", setToken);
    document.getElementById("clear-token")?.addEventListener("click", clearToken);
    document.getElementById("refresh-view")?.addEventListener("click", () => void refresh());
    document.addEventListener("click", (event) => void handleAction(event));
    void refresh();
    connectOperatorStream();
  });
})();
