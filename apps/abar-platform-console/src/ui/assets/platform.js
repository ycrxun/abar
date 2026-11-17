const state = {
  me: null,
  overview: null,
  view: "overview",
  selectedAgent: null,
  reviewTarget: null
};

const el = (id) => document.getElementById(id);
const node = (tag, className, text) => {
  const item = document.createElement(tag);
  if (className) item.className = className;
  if (text !== undefined) item.textContent = text;
  return item;
};

async function api(path, options = {}) {
  const method = String(options.method || "GET").toUpperCase();
  const csrf = ["GET", "HEAD"].includes(method)
    ? {}
    : { "X-ABAR-CSRF": state.me?.csrfToken || "" };
  const response = await fetch(path, {
    ...options,
    credentials: "same-origin",
    headers: {
      "Content-Type": "application/json",
      ...csrf,
      ...(options.headers || {})
    }
  });
  if (response.status === 401) {
    const returnTo = encodeURIComponent(
      `${window.location.pathname}${window.location.search}`
    );
    window.location.href = `/auth/login?returnTo=${returnTo}`;
    throw new Error("登录状态已失效");
  }
  const body = await response.json().catch(() => ({}));
  if (!response.ok) {
    throw new Error(body.error || body.message || `HTTP ${response.status}`);
  }
  return body;
}

function showView(view) {
  state.view = view;
  for (const item of document.querySelectorAll(".nav-item")) {
    item.classList.toggle("active", item.dataset.view === view);
  }
  const titles = {
    overview: ["平台概览", "当前 Tenant 的 Agent 与 Runtime 状态"],
    agents: ["Agent 目录", "Revision、Scope 与评审证据"],
    runtime: ["Runtime", "执行状态、Session 与 Message 聚合"]
  };
  el("viewTitle").textContent = titles[view][0];
  el("viewSubtitle").textContent = titles[view][1];
  el("overviewView").hidden = view !== "overview";
  el("agentsView").hidden = view !== "agents";
  el("runtimeView").hidden = view !== "runtime";
  if (view === "agents" && !state.selectedAgent && state.overview?.agents?.length) {
    selectAgent(state.overview.agents[0].agent);
  }
}

function metric(label, value) {
  const card = node("div", "metric");
  card.append(node("span", "", label), node("strong", "", String(value ?? 0)));
  return card;
}

function renderMetrics() {
  const data = state.overview;
  const agents = data.agents || [];
  const revisions = agents.flatMap((agent) => agent.revisions || []);
  const waiting = revisions.filter((revision) => revision.status === "waiting_review");
  const working = (data.runtime.messages?.running || 0)
    + (data.runtime.messages?.queued || 0);
  const root = el("metrics");
  root.replaceChildren(
    metric("Agent", agents.length),
    metric("Current Revision", agents.filter((agent) => agent.currentRevision).length),
    metric("等待评审", waiting.length),
    metric("Session", data.runtime.sessions?.active || 0),
    metric("执行中 Message", working),
    metric("Token 总量", data.runtime.tokens?.total || 0)
  );
}

function renderRuntime() {
  el("tenant").textContent = `tenant: ${state.overview.tenant}`;
  el("generatedAt").textContent = state.overview.generatedAt || "";
  el("runtimeStatus").textContent = state.overview.runtime.status;
  el("runtimeStatus").className =
    `status ${state.overview.runtime.status === "ready" ? "" : "degraded"}`;
  const facts = el("runtimeSummary");
  facts.replaceChildren();
  for (const [label, value] of [
    ["Server", state.overview.serverVersion],
    ["Active Worker", state.overview.runtime.activeWorkers],
    ["Provider", state.overview.runtime.providerConfigured ? "ready" : "invalid"],
    ["Storage", state.overview.runtime.storageWritable ? "writable" : "read-only"],
    ["Prompt Token", state.overview.runtime.tokens?.prompt || 0],
    ["Completion Token", state.overview.runtime.tokens?.completion || 0]
  ]) {
    const row = node("div");
    row.append(node("dt", "", label), node("dd", "", String(value)));
    facts.append(row);
  }
  renderStateCounts("sessionStates", state.overview.runtime.sessions);
  renderStateCounts("messageStates", state.overview.runtime.messages);
  el("sessionTotal").textContent =
    `total ${sumCounts(state.overview.runtime.sessions)}`;
  el("messageTotal").textContent =
    `total ${sumCounts(state.overview.runtime.messages)}`;
  const infra = el("runtimeInfra");
  infra.replaceChildren();
  for (const [label, value] of [
    ["API", state.overview.apiVersion],
    ["Server Version", state.overview.serverVersion],
    ["Provider Configured", state.overview.runtime.providerConfigured ? "yes" : "no"],
    ["Storage Writable", state.overview.runtime.storageWritable ? "yes" : "no"]
  ]) {
    const row = node("div");
    row.append(node("dt", "", label), node("dd", "", String(value)));
    infra.append(row);
  }
}

function sumCounts(counts = {}) {
  return Object.values(counts).reduce((sum, value) => sum + value, 0);
}

function renderStateCounts(id, counts = {}) {
  const root = el(id);
  root.replaceChildren();
  const labels = {
    active: "活跃", idle: "空闲", closed: "关闭", archived: "归档",
    queued: "排队", running: "运行", waiting_approval: "等待审批",
    waiting_external: "等待外部", completed: "完成", failed: "失败",
    cancelled: "取消"
  };
  for (const [key, value] of Object.entries(counts)) {
    const cell = node("div", "state-cell");
    cell.append(node("span", "", labels[key] || key), node("strong", "", String(value)));
    root.append(cell);
  }
}

function agentRow(agent) {
  const button = node("button", "agent-row");
  if (agent.agent === state.selectedAgent) button.classList.add("active");
  button.type = "button";
  button.append(
    node("span", "agent-name", agent.agent),
    node("span", "agent-meta",
      `rev ${agent.currentRevision ?? "—"} · ${agent.revisionCount} 个版本`),
    node("span", "agent-meta",
      `${agent.sessions} Session · ${agent.messages} Message`)
  );
  button.addEventListener("click", () => selectAgent(agent.agent));
  return button;
}

function renderAgents() {
  const agents = state.overview.agents || [];
  el("agentCount").textContent = `${agents.length} Agents`;
  const overviewAgents = el("overviewAgents");
  overviewAgents.replaceChildren();
  for (const agent of agents.slice(0, 6)) overviewAgents.append(agentRow(agent));
  if (!agents.length) overviewAgents.append(node("div", "muted", "尚未发布 Agent Revision"));
  const root = el("agents");
  root.replaceChildren();
  for (const agent of agents) root.append(agentRow(agent));
  if (!agents.length) root.append(node("div", "muted", "尚未发布 Agent Revision"));
  renderAgentDetail();
}

function selectAgent(name) {
  state.selectedAgent = name;
  renderAgents();
}

function renderAgentDetail() {
  const root = el("agentDetail");
  const agent = state.overview?.agents?.find(
    (item) => item.agent === state.selectedAgent
  );
  if (!agent) {
    root.replaceChildren(node("div", "empty-detail", "选择一个 Agent 查看 Revision 与 Scope。"));
    return;
  }
  root.replaceChildren();
  const head = node("div", "detail-head");
  head.append(
    node("h3", "", agent.agent),
    node("p", "",
      `Current Revision ${agent.currentRevision ?? "—"} · ${agent.sessions} Session · ` +
      `${agent.messages} Message · ${agent.tokens?.total || 0} Token`)
  );
  root.append(head);
  for (const revision of agent.revisions || []) {
    root.append(revisionCard(agent, revision));
  }
}

function revisionCard(agent, revision) {
  const card = node("article", "revision");
  const head = node("div", "revision-head");
  const title = node("div");
  title.append(node("strong", "", `Revision ${revision.revision}`));
  title.append(node("div", "muted",
    `${revision.modelClass} · max ${revision.maxSteps} steps`));
  head.append(title);
  head.append(node("span", `revision-status ${revision.status}`,
    revisionLabel(revision.status)));
  if (revision.status !== "current") {
    const review = node("button", "review-button", "评审 Scope");
    review.type = "button";
    review.addEventListener("click", () => openReview(agent, revision));
    head.append(review);
  }
  card.append(head);
  const grid = node("div", "revision-grid");
  grid.append(
    field("Read Tools", revision.readTools?.length || 0),
    field("Write Tools", revision.writeTools?.length || 0),
    field("Skills", revision.skills?.length || 0),
    field("Knowledge", revision.knowledgeSources?.length || 0)
  );
  card.append(grid);
  if (revision.writeTools?.length) {
    card.append(chips("Write Scope", revision.writeTools));
  }
  if (revision.review) {
    card.append(node("p", "muted",
      `评审：${reviewLabel(revision.review.decision)} · ${revision.review.reviewer}` +
      (revision.review.comment ? ` · ${revision.review.comment}` : "")));
  }
  return card;
}

function field(label, value) {
  const item = node("div", "revision-field");
  item.append(node("span", "", label), node("code", "", String(value)));
  return item;
}

function chips(label, values) {
  const root = node("div");
  root.append(node("span", "muted", label));
  const list = node("div", "chips");
  for (const value of values) list.append(node("code", "", value));
  root.append(list);
  return root;
}

function revisionLabel(status) {
  return {
    current: "Current",
    approved: "已批准",
    rejected: "已拒绝",
    waiting_review: "等待评审"
  }[status] || status;
}

function reviewLabel(decision) {
  return decision === "approved" ? "已批准" : "已拒绝";
}

function openReview(agent, revision) {
  state.reviewTarget = { agent, revision };
  el("reviewTitle").textContent =
    `${agent.agent} · Revision ${revision.revision}`;
  el("reviewComment").value = "";
  el("reviewError").textContent = "";
  const scope = el("reviewScope");
  scope.replaceChildren(
    chips("Read Tools", revision.readTools || []),
    chips("Write Tools", revision.writeTools || []),
    chips("Skills", revision.skills || []),
    chips("Knowledge Sources", revision.knowledgeSources || [])
  );
  el("reviewPanel").hidden = false;
  el("reviewComment").focus();
}

function closeReview() {
  state.reviewTarget = null;
  el("reviewPanel").hidden = true;
}

async function submitReview(decision) {
  if (!state.reviewTarget) return;
  const { agent, revision } = state.reviewTarget;
  el("reviewApprove").disabled = true;
  el("reviewReject").disabled = true;
  el("reviewError").textContent = "";
  try {
    await api(
      `/api/agents/${agent.agent}/revisions/${revision.revision}/review`,
      {
        method: "POST",
        body: JSON.stringify({
          decision,
          comment: el("reviewComment").value.trim() || null
        })
      }
    );
    closeReview();
    await loadOverview();
  } catch (error) {
    el("reviewError").textContent = error.message;
  } finally {
    el("reviewApprove").disabled = false;
    el("reviewReject").disabled = false;
  }
}

async function loadOverview() {
  el("error").textContent = "";
  try {
    state.overview = await api("/api/overview");
    renderMetrics();
    renderRuntime();
    renderAgents();
  } catch (error) {
    el("error").textContent = error.message;
  }
}

function renderUser() {
  const me = state.me;
  el("currentUser").textContent =
    `${me.displayName || me.subjectId} · ${me.tenant} · ${(me.roles || []).join(", ")}`;
  el("logout").hidden = me.mode !== "oidc";
}

for (const button of document.querySelectorAll(".nav-item")) {
  button.addEventListener("click", () => showView(button.dataset.view));
}
for (const button of document.querySelectorAll("[data-view-target]")) {
  button.addEventListener("click", () => showView(button.dataset.viewTarget));
}
el("refresh").addEventListener("click", loadOverview);
el("reviewClose").addEventListener("click", closeReview);
el("reviewApprove").addEventListener("click", () => submitReview("approved"));
el("reviewReject").addEventListener("click", () => submitReview("rejected"));
el("logout").addEventListener("click", async () => {
  try {
    const result = await api("/auth/logout", { method: "POST" });
    window.location.href = result.redirect || "/";
  } catch (error) {
    el("error").textContent = error.message;
  }
});
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && !el("reviewPanel").hidden) closeReview();
});

async function initialize() {
  try {
    state.me = await api("/api/me");
    renderUser();
    await loadOverview();
    showView("overview");
  } catch (error) {
    el("error").textContent = error.message;
  }
}

initialize();
