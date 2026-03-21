const els = {
  timestamp: document.getElementById("timestamp"),
  bestOpportunity: document.getElementById("best-opportunity"),
  agentBrief: document.getElementById("agent-brief"),
  statusChips: document.getElementById("status-chips"),
  sourceHealth: document.getElementById("source-health"),
  cohortSummary: document.getElementById("cohort-summary"),
  cohortLeaderboard: document.getElementById("cohort-leaderboard"),
  topOpportunities: document.getElementById("top-opportunities"),
  liveTop5: document.getElementById("live-top5"),
  coinmarketcapMovers: document.getElementById("coinmarketcap-movers"),
  supervisorLog: document.getElementById("supervisor-log"),
  gatewayLog: document.getElementById("gateway-log"),
  openclawSummary: document.getElementById("openclaw-summary"),
  actionResult: document.getElementById("action-result"),
  chatMessages: document.getElementById("chat-messages"),
  chatInput: document.getElementById("chat-input"),
  sendChat: document.getElementById("send-chat"),
  panelCheckboxes: document.getElementById("panel-checkboxes"),
  saveLayout: document.getElementById("save-layout"),
  resetLayout: document.getElementById("reset-layout"),
};

const panelNames = {
  hero: "Mission Brief",
  status: "System Status",
  controls: "Operator Controls",
  chat: "Operator Chat",
  cohort: "Active Cohort",
  leaderboard: "Cohort Leaderboard",
  opportunities: "Best Opportunities",
  live: "Live Top 5",
  movers: "CoinMarketCap Movers",
  logs: "Supervisor Log",
  openclaw: "OpenClaw Bridge",
};

const panelRefs = Object.fromEntries(
  Array.from(document.querySelectorAll("[data-panel]")).map((element) => [element.dataset.panel, element])
);

let currentSettings = {
  visible_panels: Object.keys(panelNames),
  density: "dense",
};

function chipClass(ok) {
  if (ok === "ok" || ok === true) return "chip chip-ok";
  if (ok === "warn") return "chip chip-warn";
  return "chip chip-bad";
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function renderRows(rows, template, fallback = "No data yet.") {
  if (!rows || rows.length === 0) {
    return `<div class="empty">${escapeHtml(fallback)}</div>`;
  }
  return `<div class="table">${rows.map(template).join("")}</div>`;
}

function valueOrFallback(value, fallback = "--") {
  if (value === null || value === undefined || value === "") {
    return fallback;
  }
  return value;
}

function renderMetric(label, value, accent = "") {
  return `
    <div class="bot-stat ${accent}">
      <span class="bot-stat-label">${escapeHtml(label)}</span>
      <span class="bot-stat-value">${escapeHtml(valueOrFallback(value))}</span>
    </div>
  `;
}

function renderBotBoard(rows, fallback, renderCard) {
  if (!rows || rows.length === 0) {
    return `<div class="empty">${escapeHtml(fallback)}</div>`;
  }
  return `<div class="bot-board">${rows.map(renderCard).join("")}</div>`;
}

function renderChat(chat) {
  const messages = chat?.messages || [];
  if (messages.length === 0) {
    els.chatMessages.innerHTML = `<div class="empty">No operator notes yet.</div>`;
    return;
  }
  els.chatMessages.innerHTML = messages
    .slice(-12)
    .map((message) => {
      const role = escapeHtml(message.role || "operator");
      const kind = escapeHtml(message.kind || "note");
      const timestamp = escapeHtml(String(message.timestamp || "--"));
      const action = message.action ? ` · ${escapeHtml(message.action)}` : "";
      return `
        <article class="chat-message chat-message-role-${escapeHtml(message.role || "operator")}">
          <div class="chat-message-header">
            <span>${role} · ${kind}${action}</span>
            <span>${timestamp}</span>
          </div>
          <div class="chat-message-text">${escapeHtml(message.text || "")}</div>
        </article>
      `;
    })
    .join("");
  els.chatMessages.scrollTop = els.chatMessages.scrollHeight;
}

function renderSettings(settings) {
  currentSettings = {
    visible_panels: settings.visible_panels || Object.keys(panelNames),
    density: settings.density || "dense",
  };
  document.body.dataset.density = currentSettings.density;
  Object.entries(panelRefs).forEach(([name, element]) => {
    if (name === "customize") {
      element.classList.remove("panel-hidden");
      return;
    }
    element.classList.toggle("panel-hidden", !currentSettings.visible_panels.includes(name));
  });
  document.querySelectorAll('input[name="density"]').forEach((input) => {
    input.checked = input.value === currentSettings.density;
  });
  els.panelCheckboxes.innerHTML = Object.entries(panelNames)
    .map(
      ([key, label]) => `
        <label>
          <input type="checkbox" data-panel-toggle="${escapeHtml(key)}" ${currentSettings.visible_panels.includes(key) ? "checked" : ""} />
          ${escapeHtml(label)}
        </label>
      `
    )
    .join("");
  els.panelCheckboxes.querySelectorAll("[data-panel-toggle]").forEach((input) => {
    input.addEventListener("change", () => {
      input.closest("label").classList.toggle("selected", input.checked);
    });
  });
}

function renderState(snapshot) {
  const supervisor = snapshot.supervisor || {};
  const cohort = supervisor.cohort || {};
  const services = snapshot.services || {};
  const openclaw = snapshot.openclaw || {};
  const sourceStatuses = supervisor.sources || {};
  const cmcSnapshot = (supervisor.source_snapshots || {}).coinmarketcap || {};
  const settings = snapshot.settings || {};
  const chat = snapshot.chat || {};

  els.timestamp.textContent = snapshot.timestamp || "--";
  els.bestOpportunity.textContent = snapshot.best_opportunity ? snapshot.best_opportunity.slug : "--";
  els.agentBrief.textContent = snapshot.agent_brief || "No operator brief available.";
  renderSettings(settings);
  renderChat(chat);

  const chips = [];
  chips.push(`<span class="${chipClass(services.market_supervisor?.running)}">supervisor ${services.market_supervisor?.running ? "running" : "stopped"}</span>`);
  Object.entries(sourceStatuses).forEach(([name, payload]) => {
    chips.push(`<span class="${chipClass(payload.status)}">${escapeHtml(name)} ${escapeHtml(payload.status)}</span>`);
  });
  els.statusChips.innerHTML = chips.join("");

  els.sourceHealth.innerHTML = Object.entries(sourceStatuses)
    .map(([name, payload]) => `<div class="card"><strong>${escapeHtml(name)}</strong><br>${escapeHtml(payload.message || payload.status || "unknown")}</div>`)
    .join("");

  els.cohortSummary.innerHTML = [
    `<div class="card"><strong>ID</strong><br>${escapeHtml(cohort.cohort_id || "--")}</div>`,
    `<div class="card"><strong>Status</strong><br>${escapeHtml(cohort.status || "--")}</div>`,
    `<div class="card"><strong>Progress</strong><br>${escapeHtml(cohort.progress_percent || 0)}%</div>`,
    `<div class="card"><strong>Winner</strong><br>${escapeHtml(cohort.winner?.wallet_id || "pending")}</div>`,
  ].join("");

  els.cohortLeaderboard.innerHTML = renderBotBoard(
    cohort.leaderboard || [],
    "The cohort has not opened positions yet.",
    (row, index) => `
      <article class="bot-card ${index === 0 ? "bot-card-leader" : ""}">
        <div class="bot-card-top">
          <div>
            <div class="bot-card-title">${escapeHtml(row.wallet_id)}</div>
            <div class="bot-card-subtitle">${escapeHtml(row.label)}</div>
          </div>
          <span class="bot-tag">${escapeHtml(row.last_action || "HOLD")}</span>
        </div>
        <div class="bot-stats">
          ${renderMetric("Equity", row.equity, "bot-stat-accent-lime")}
          ${renderMetric("Trades", row.trade_count, "bot-stat-accent-cyan")}
          ${renderMetric("Action", row.last_action, "bot-stat-accent-violet")}
        </div>
      </article>
    `
  );

  els.topOpportunities.innerHTML = renderBotBoard(
    supervisor.top_opportunities || [],
    "No opportunities scored yet.",
    (row) => `
      <article class="bot-card">
        <div class="bot-card-top">
          <div>
            <div class="bot-card-title">${escapeHtml(row.slug)}</div>
            <div class="bot-card-subtitle">${escapeHtml(row.question || "")}</div>
          </div>
          <span class="bot-tag">${escapeHtml(row.strategy || "scan")}</span>
        </div>
        <div class="bot-stats">
          ${renderMetric("EV", row.avg_expected_value, "bot-stat-accent-lime")}
          ${renderMetric("Buy signals", row.buy_signals, "bot-stat-accent-cyan")}
          ${renderMetric("Score", row.score, "bot-stat-accent-violet")}
        </div>
      </article>
    `
  );

  els.liveTop5.innerHTML = renderBotBoard(
    supervisor.live_top5 || [],
    "No winners promoted yet.",
    (row, index) => `
      <article class="bot-card ${index === 0 ? "bot-card-leader" : ""}">
        <div class="bot-card-top">
          <div>
            <div class="bot-card-title">${escapeHtml(row.wallet_id)}</div>
            <div class="bot-card-subtitle">${escapeHtml(row.label)}</div>
          </div>
          <span class="bot-tag">${escapeHtml(row.last_action || "LIVE")}</span>
        </div>
        <div class="bot-stats">
          ${renderMetric("Promotion", row.promotion_score, "bot-stat-accent-violet")}
          ${renderMetric("Equity", row.equity, "bot-stat-accent-lime")}
          ${renderMetric("Trades", row.trade_count, "bot-stat-accent-cyan")}
        </div>
      </article>
    `
  );

  els.coinmarketcapMovers.innerHTML = renderBotBoard(
    cmcSnapshot.top_movers_24h || [],
    "No CoinMarketCap movers yet.",
    (row) => `
      <article class="bot-card">
        <div class="bot-card-top">
          <div>
            <div class="bot-card-title">${escapeHtml(row.symbol)}</div>
            <div class="bot-card-subtitle">${escapeHtml(row.name || "")}</div>
          </div>
          <span class="bot-tag">${escapeHtml(row.rank || "--")}</span>
        </div>
        <div class="bot-stats">
          ${renderMetric("24h", `${valueOrFallback(row.percent_change_24h)}%`, "bot-stat-accent-lime")}
          ${renderMetric("Rank", row.rank, "bot-stat-accent-cyan")}
          ${renderMetric("Volume", row.volume_24h, "bot-stat-accent-violet")}
        </div>
      </article>
    `
  );

  els.supervisorLog.textContent = (snapshot.logs?.market_supervisor || []).join("\n") || "No supervisor log lines yet.";
  els.gatewayLog.textContent = (snapshot.logs?.gateway || []).join("\n") || "No gateway log lines yet.";
  const pairing = openclaw.pairing || {};
  els.openclawSummary.innerHTML = [
    `<div class="card"><strong>Profile</strong><br>${escapeHtml(openclaw.profile || "--")}</div>`,
    `<div class="card"><strong>Gateway Port</strong><br>${escapeHtml(openclaw.gateway_port || "--")}</div>`,
    `<div class="card"><strong>Pairing</strong><br>${escapeHtml(pairing.status || "--")}</div>`,
    `<div class="card"><strong>Allowed Origin</strong><br>${escapeHtml(pairing.cockpit_origin || "--")}</div>`,
    `<div class="card"><strong>Paired Control UIs</strong><br>${escapeHtml(pairing.paired_control_ui_devices || 0)}</div>`,
    `<div class="card"><strong>Pending Requests</strong><br>${escapeHtml(pairing.pending_control_ui_requests || 0)}</div>`,
  ].join("");
  const pairingMessage = document.getElementById("openclaw-pairing-message");
  if (pairingMessage) {
    pairingMessage.textContent = pairing.message || "OpenClaw pairing status unavailable.";
    pairingMessage.dataset.state = pairing.status || "unknown";
  }
}

async function runAction(action) {
  const popup = action === "open_openclaw_dashboard" ? window.open("about:blank", "_blank") : null;
  els.actionResult.textContent = `Running ${action}...`;
  try {
    const response = await fetch("/api/action", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ action }),
    });
    const payload = await response.json();
    els.actionResult.textContent = JSON.stringify(payload, null, 2);
    if (payload.snapshot) renderState(payload.snapshot);
    if (payload.dashboard_url) {
      if (popup && !popup.closed) {
        popup.location.href = payload.dashboard_url;
        popup.focus();
      } else {
        els.actionResult.innerHTML = `OpenClaw dashboard: <a href="${escapeHtml(payload.dashboard_url)}" target="_blank" rel="noopener noreferrer">${escapeHtml(payload.dashboard_url)}</a>`;
      }
    }
  } catch (error) {
    if (popup && !popup.closed) {
      popup.close();
    }
    throw error;
  }
}

async function submitChat(text) {
  const trimmed = text.trim();
  if (!trimmed) return;
  els.actionResult.textContent = "Sending chat command...";
  const response = await fetch("/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "submit_chat",
      text: trimmed,
    }),
  });
  const payload = await response.json();
  els.actionResult.textContent = JSON.stringify(payload, null, 2);
  els.chatInput.value = "";
  if (payload.snapshot) renderState(payload.snapshot);
}

async function saveLayout() {
  const selectedPanels = Array.from(els.panelCheckboxes.querySelectorAll("[data-panel-toggle]"))
    .filter((input) => input.checked)
    .map((input) => input.dataset.panelToggle);
  const selectedDensity = document.querySelector('input[name="density"]:checked')?.value || "dense";
  const response = await fetch("/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "set_visible_panels",
      visible_panels: selectedPanels,
    }),
  });
  const panelsPayload = await response.json();
  const densityResponse = await fetch("/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "set_density",
      density: selectedDensity,
    }),
  });
  const densityPayload = await densityResponse.json();
  els.actionResult.textContent = JSON.stringify({ panels: panelsPayload, density: densityPayload }, null, 2);
  if (panelsPayload.snapshot) renderState(panelsPayload.snapshot);
}

async function resetLayout() {
  const response = await fetch("/api/settings");
  const settings = await response.json();
  await fetch("/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "set_visible_panels",
      visible_panels: settings.visible_panels || Object.keys(panelNames),
    }),
  });
  await fetch("/api/action", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      action: "set_density",
      density: settings.density || "dense",
    }),
  });
  const snapshot = await fetch("/api/state").then((result) => result.json());
  renderState(snapshot);
}

document.querySelectorAll("[data-action]").forEach((button) => {
  button.addEventListener("click", () => {
    runAction(button.dataset.action).catch((error) => {
      els.actionResult.textContent = String(error);
    });
  });
});

document.querySelectorAll("[data-chat-command]").forEach((button) => {
  button.addEventListener("click", () => {
    els.chatInput.value = button.dataset.chatCommand;
    submitChat(els.chatInput.value).catch((error) => {
      els.actionResult.textContent = String(error);
    });
  });
});

els.chatInput.addEventListener("keydown", (event) => {
  if (event.key === "Enter" && !event.shiftKey) {
    event.preventDefault();
    submitChat(els.chatInput.value).catch((error) => {
      els.actionResult.textContent = String(error);
    });
  }
});

els.sendChat.addEventListener("click", () => {
  submitChat(els.chatInput.value).catch((error) => {
    els.actionResult.textContent = String(error);
  });
});

els.saveLayout.addEventListener("click", () => {
  saveLayout().catch((error) => {
    els.actionResult.textContent = String(error);
  });
});

els.resetLayout.addEventListener("click", () => {
  resetLayout().catch((error) => {
    els.actionResult.textContent = String(error);
  });
});

fetch("/api/state")
  .then((response) => response.json())
  .then(renderState)
  .catch((error) => {
    els.agentBrief.textContent = `Failed to load initial state: ${error}`;
  });

const events = new EventSource("/api/events");
events.onmessage = (event) => {
  renderState(JSON.parse(event.data));
};
events.onerror = () => {
  els.agentBrief.textContent = "Live update stream disconnected. Waiting for reconnect…";
};
