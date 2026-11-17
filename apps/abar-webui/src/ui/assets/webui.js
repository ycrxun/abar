
    const state = {
      view: null, sessions: [], selected: null, selectedMessage: null,
      detail: null, approvals: [], selectedApproval: null, me: null, busy: false
    };
    const statusNames = {
      queued: "已排队", running: "运行中", waiting_approval: "等待审批",
      waiting_external: "等待外部确认", completed: "已完成", failed: "失败",
      cancelled: "已取消", active: "活跃", idle: "空闲",
      closed: "已关闭", archived: "已归档"
    };
    const eventNames = {
      "message.created": "Message 已创建",
      "message.started": "Message 开始执行",
      "approval.requested": "发起人工审批",
      "approval.granted": "审批通过",
      "approval.rejected": "审批拒绝",
      "message.waiting_external": "等待外部结果",
      "message.resumed": "审批后恢复执行",
      "message.reconciled": "查询外部结果",
      "message.completed": "Message 完成",
      "message.failed": "Message 失败",
      "message.cancelled": "Message 取消",
      "migration.completed": "旧 Session 已导入"
    };
    const el = (id) => document.getElementById(id);
    const layoutStorageKey = "abar.webui.layout";
    const layout = { sidebarCollapsed: false, contextExpanded: false };
    function contextEnabled() {
      return state.view?.context?.enabled !== false;
    }
    function approvalEnabled() {
      return state.view?.approval?.enabled !== false;
    }
    function saveLayout() {
      try {
        localStorage.setItem(layoutStorageKey, JSON.stringify(layout));
      } catch {}
    }
    function restoreLayout() {
      try {
        const saved = JSON.parse(localStorage.getItem(layoutStorageKey) || "{}");
        layout.sidebarCollapsed = saved.sidebarCollapsed === true;
        layout.contextExpanded = saved.contextExpanded === true;
      } catch {}
      el("appShell").classList.toggle("sidebar-collapsed", layout.sidebarCollapsed);
      applyContextMode();
      renderLayoutControls();
    }
    function renderLayoutControls() {
      const mobile = window.matchMedia("(max-width: 900px)").matches;
      const collapsed = mobile
        ? !el("sidebar").classList.contains("open")
        : layout.sidebarCollapsed;
      el("sidebarToggle").textContent = collapsed ? "☰" : "⇤";
      el("sidebarToggle").setAttribute("aria-expanded", String(!collapsed));
      el("sidebarToggle").setAttribute(
        "aria-label",
        collapsed ? "展开侧边栏" : "折叠侧边栏"
      );
      el("sidebarToggle").title = collapsed ? "展开侧边栏" : "折叠侧边栏";
      el("contextToggle").setAttribute(
        "aria-pressed",
        String(layout.contextExpanded)
      );
      el("contextToggle").setAttribute(
        "aria-label",
        layout.contextExpanded ? "还原上下文宽度" : "放大上下文"
      );
      el("contextToggle").title =
        layout.contextExpanded ? "还原上下文宽度" : "放大上下文";
      el("contextToggleIcon").textContent = layout.contextExpanded ? "⊡" : "⤢";
      el("contextToggleLabel").textContent =
        layout.contextExpanded ? "还原" : "放大";
    }
    function toggleSidebar() {
      if (window.matchMedia("(max-width: 900px)").matches) {
        el("sidebar").classList.toggle("open");
      } else {
        layout.sidebarCollapsed = !layout.sidebarCollapsed;
        el("appShell").classList.toggle(
          "sidebar-collapsed",
          layout.sidebarCollapsed
        );
        saveLayout();
      }
      renderLayoutControls();
    }
    function toggleContextWidth() {
      layout.contextExpanded = !layout.contextExpanded;
      if (!layout.contextExpanded) closeFloatingComposer();
      applyContextMode();
      saveLayout();
      renderLayoutControls();
    }
    function applyContextMode() {
      const available = contextEnabled();
      const focused =
        layout.contextExpanded &&
        available &&
        Boolean(state.detail?.id) &&
        !state.selectedApproval;
      el("contextRail").hidden = !available;
      el("workspace").classList.toggle("context-disabled", !available);
      el("workspace").classList.toggle(
        "context-expanded",
        layout.contextExpanded && available
      );
      document.body.classList.toggle("context-focus-mode", focused);
    }
    const requestId = () => crypto.randomUUID
      ? crypto.randomUUID()
      : `task_${Date.now()}_${Math.random().toString(16).slice(2)}`;
    const node = (tag, className, text) => {
      const item = document.createElement(tag);
      if (className) item.className = className;
      if (text !== undefined) item.textContent = text;
      return item;
    };
    function resizeTextArea(id) {
      const input = el(id);
      input.style.height = "auto";
      input.style.height = `${Math.min(input.scrollHeight, 150)}px`;
    }
    function resizeComposer() {
      resizeTextArea("taskInput");
      resizeTextArea("floatingInput");
    }
    function setComposerDraft(value) {
      el("taskInput").value = value;
      el("floatingInput").value = value;
      resizeComposer();
    }
    function closeFloatingComposer() {
      if (el("floatingComposer").hidden) return;
      setComposerDraft(el("floatingInput").value);
      el("floatingComposer").hidden = true;
      el("workspace").classList.remove("floating-composer-open");
      el("workspace").classList.remove("quick-palette-open");
      el("floatingQuickSection").hidden = true;
      el("floatingQuickAdd").setAttribute("aria-expanded", "false");
      el("floatingLauncher").setAttribute("aria-expanded", "false");
    }
    function openFloatingComposer() {
      setComposerDraft(el("taskInput").value);
      renderFloatingConversation();
      el("floatingComposer").hidden = false;
      el("workspace").classList.add("floating-composer-open");
      el("floatingLauncher").setAttribute("aria-expanded", "true");
      el("floatingInput").focus();
      requestAnimationFrame(() => {
        const scroll = el("floatingConversation").parentElement;
        scroll.scrollTop = scroll.scrollHeight;
      });
    }
    function toggleFloatingComposer() {
      if (el("floatingComposer").hidden) {
        openFloatingComposer();
      } else {
        closeFloatingComposer();
      }
    }
    async function api(path, options = {}) {
      const method = String(options.method || "GET").toUpperCase();
      const csrf = method === "GET" || method === "HEAD"
        ? {}
        : { "X-ABAR-CSRF": state.me?.csrfToken || "" };
      const response = await fetch(path, {
        ...options,
        credentials: "same-origin",
        headers: { "Content-Type": "application/json", ...csrf, ...(options.headers || {}) }
      });
      if (response.status === 401) {
        const returnTo = encodeURIComponent(
          `${window.location.pathname}${window.location.search}`
        );
        window.location.href = `/auth/login?returnTo=${returnTo}`;
        throw new Error("登录状态已失效");
      }
      const body = await response.json().catch(() => ({}));
      if (!response.ok) throw new Error(body.error || body.message || `HTTP ${response.status}`);
      return body;
    }
    function getPath(value, path) {
      if (!path) return value;
      return path.split(".").reduce((current, key) => current?.[key], value);
    }
    function displayValue(value, join = ", ") {
      if (value === undefined || value === null) return "";
      if (Array.isArray(value)) return value.join(join);
      if (typeof value === "object") return JSON.stringify(value);
      return String(value);
    }
    function shortTime(value) {
      if (!value) return "";
      const date = new Date(value);
      if (Number.isNaN(date.getTime())) return String(value);
      const now = new Date();
      return date.toDateString() === now.toDateString()
        ? date.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })
        : date.toLocaleDateString([], { month: "2-digit", day: "2-digit" });
    }
    function interpolate(template, values) {
      return String(template || "").replace(/\{\{([^}]+)\}\}/g, (_, key) => values[key] || "");
    }
    function toolResult(toolId) {
      const results = state.detail?.toolResults || [];
      return [...results].reverse().find((item) => item.toolId === toolId)?.result;
    }
    function currentConfirmation() {
      const result = [...(state.detail?.toolResults || [])]
        .reverse()
        .map((item) => item.result)
        .find((value) => value?.confirmation && typeof value.confirmation === "object");
      return result
        ? {
            confirmation: result.confirmation,
            agentRevision: state.detail.agentRevision
          }
        : null;
    }
    async function setSelected(id) {
      closeStream();
      stopFallback();
      state.selected = id;
      state.selectedMessage = null;
      state.selectedApproval = null;
      state.detail = null;
      closeFloatingComposer();
      el("approvalDetail").hidden = true;
      el("empty").hidden = true;
      el("detail").hidden = false;
      el("taskForm").hidden = false;
      el("floatingLauncher").hidden = false;
      el("sidebar").classList.remove("open");
      renderLayoutControls();
      renderSessions();
      renderApprovals();
      renderReplyControls();
      loadDetail();
    }
    function setSelectedApproval(approval) {
      closeStream();
      stopFallback();
      state.selectedApproval = approval;
      state.selected = null;
      state.detail = null;
      closeFloatingComposer();
      el("taskForm").hidden = true;
      el("floatingLauncher").hidden = true;
      el("workspace").classList.remove("with-context");
      applyContextMode();
      renderSessions();
      renderApprovals();
      renderApprovalDetail();
      el("sidebar").classList.remove("open");
      renderLayoutControls();
    }
    function startNewChat() {
      closeStream();
      stopFallback();
      detailRequest += 1;
      state.selected = null;
      state.selectedMessage = null;
      state.selectedApproval = null;
      state.detail = null;
      closeFloatingComposer();
      el("approvalDetail").hidden = true;
      el("detail").hidden = true;
      el("empty").hidden = false;
      el("taskForm").hidden = false;
      el("floatingLauncher").hidden = false;
      el("workspace").classList.remove("with-context");
      setComposerDraft("");
      el("createError").textContent = "";
      el("sidebar").classList.remove("open");
      renderLayoutControls();
      renderSessions();
      renderApprovals();
      renderReplyControls();
      renderFloatingConversation();
      el("taskInput").focus();
    }
    function renderTask() {
      const task = state.view?.task || {};
      el("emptyTitle").textContent = task.title || "开始一段对话";
      el("taskInput").placeholder =
        task.chatPlaceholder || "告诉 Agent 你想了解什么或要执行什么";
      const fields = el("taskFields");
      fields.replaceChildren();
      for (const field of task.fields || []) {
        const label = node("label", "", field.label || field.name);
        let input;
        if (field.type === "select") {
          input = node("select");
          for (const option of field.options || []) {
            const item = node("option", "", option);
            item.value = option;
            input.append(item);
          }
          input.value = field.defaultValue || field.options?.[0] || "";
        } else if (field.type === "textarea") {
          input = node("textarea");
          input.value = field.defaultValue || "";
        } else {
          input = node("input");
          input.type = "text";
          input.value = field.defaultValue || "";
        }
        input.id = `taskField-${field.name}`;
        input.placeholder = field.placeholder || "";
        label.append(input);
        fields.append(label);
      }
      const actions = el("taskActions");
      actions.replaceChildren();
      for (const action of task.actions || []) {
        const button = node("button", "secondary", action.label || action.id);
        button.type = "button";
        button.addEventListener("click", () => applyTask(action));
        actions.append(button);
      }
      const floatingActions = el("floatingActions");
      floatingActions.replaceChildren();
      for (const action of task.actions || []) {
        const button = node("button", "floating-action", action.label || action.id);
        button.type = "button";
        button.addEventListener("click", () => applyTask(action, true));
        floatingActions.append(button);
      }
      if (!(task.actions || []).length) {
        floatingActions.append(
          node("div", "floating-action-empty", "当前 Agent 没有业务快捷内容")
        );
      }
      el("businessQuick").hidden =
        !(task.actions || []).length && !(task.fields || []).length;
      const suggestions = el("emptySuggestions");
      suggestions.replaceChildren();
      for (const action of (task.actions || []).slice(0, 3)) {
        const button = node("button", "suggestion");
        button.type = "button";
        button.append(
          node("strong", "", action.label || action.id),
          node("span", "", "填入下方输入框")
        );
        button.addEventListener("click", () => applyTask(action));
        suggestions.append(button);
      }
    }
    function renderView() {
      const metadata = state.view?.metadata || {};
      el("appTitle").textContent = metadata.title || "Agent WebUI";
      el("appSubtitle").textContent = metadata.subtitle || "";
      document.title = metadata.title || "Agent WebUI";
      el("floatingTitle").textContent = metadata.title || "Agent";
      el("emptySubtitle").textContent =
        metadata.subtitle || "直接描述目标，Agent 会判断需要了解什么以及下一步执行什么。";
      el("agentIdentity").textContent =
        state.view?.binding?.agent || "未绑定 Agent";
      const badges = el("appBadges");
      badges.replaceChildren();
      for (const text of metadata.badges || []) badges.append(node("span", "badge", text));
      el("approvalsSection").hidden = !approvalEnabled();
      renderTask();
      applyContextMode();
      renderFloatingConversation();
    }
    function renderUser() {
      const me = state.me;
      if (!me) return;
      const roles = (me.roles || []).join(", ") || "无角色";
      el("currentUser").textContent =
        `${me.displayName || me.subjectId} · ${me.tenant} · ${roles}`;
      el("currentUser").title = `${me.mode}${me.issuer ? ` · ${me.issuer}` : ""}`;
      el("logout").hidden = me.mode !== "oidc";
    }
    function renderSessions() {
      const root = el("sessions");
      root.replaceChildren();
      if (!state.sessions.length) {
        root.append(node("div", "event-detail", "暂无任务"));
        return;
      }
      for (const session of state.sessions) {
        const button = node("button", "session-item");
        if (session.id === state.selected) button.classList.add("active");
        button.type = "button";
        button.title = `${session.id}\n${session.latestMessageInput || "新会话"}`;
        button.append(
          node("span", "session-title", session.latestMessageInput || "新会话"),
          node("span", "session-meta",
            `${statusNames[session.state] || session.state || ""} · ${shortTime(session.lastActiveAt || session.createdAt)}`)
        );
        button.addEventListener("click", () => setSelected(session.id));
        root.append(button);
      }
    }
    function renderApprovals() {
      const root = el("approvals");
      root.replaceChildren();
      if (!state.approvals.length) {
        root.append(node("div", "event-detail", "暂无待审批任务"));
        return;
      }
      for (const approval of state.approvals) {
        const button = node("button", "approval-item");
        if (approval.approvalId === state.selectedApproval?.approvalId) {
          button.classList.add("active");
        }
        button.type = "button";
        button.append(
          node("span", "approval-tool", approval.toolId),
          node("span", "session-id", `${approval.agent} · ${approval.requester}`),
          node("span", "session-meta", approval.pendingSince)
        );
        button.addEventListener("click", () => setSelectedApproval(approval));
        root.append(button);
      }
    }
    async function loadApprovals() {
      if (!approvalEnabled()) {
        state.approvals = [];
        renderApprovals();
        return;
      }
      try {
        const data = await api("/api/approvals");
        state.approvals = data.approvals || [];
        if (state.selectedApproval) {
          const current = state.approvals.find(
            (item) => item.approvalId === state.selectedApproval.approvalId
          );
          if (current) {
            state.selectedApproval = current;
          } else {
            state.selectedApproval = null;
            el("approvalDetail").hidden = true;
            el("empty").hidden = false;
            el("taskForm").hidden = false;
            el("floatingLauncher").hidden = false;
            el("workspace").classList.remove("with-context");
            applyContextMode();
          }
        }
        renderApprovals();
      } catch (error) {
        if (error.message !== "登录状态已失效") {
          el("createError").textContent = error.message;
        }
      }
    }
    async function loadSessions(selectFirst = true) {
      if (state.busy) return;
      state.busy = true;
      try {
        const data = await api("/api/sessions");
        state.sessions = data.sessions || [];
        if (state.selected && !state.sessions.some((session) => session.id === state.selected)) {
          state.selected = null;
          state.detail = null;
          el("detail").hidden = true;
          el("empty").hidden = false;
          el("taskForm").hidden = false;
        }
        if (!state.selected && selectFirst && state.sessions.length) {
          state.selected = state.sessions[0].id;
        }
        renderSessions();
      } catch (error) {
        el("createError").textContent = error.message;
      } finally {
        state.busy = false;
      }
    }
    function renderMetrics() {
      const root = el("metrics");
      root.replaceChildren();
      for (const metric of state.view?.metrics || []) {
        const value = toolResult(metric.toolId);
        if (value === undefined) continue;
        const card = node("div", "metric");
        card.append(
          node("span", "", metric.label || ""),
          node("strong", "", displayValue(getPath(value, metric.path)) || "—")
        );
        root.append(card);
      }
      root.hidden = !root.children.length;
    }
    function renderPanels() {
      const root = el("panels");
      root.replaceChildren();
      for (const panel of state.view?.panels || []) {
        if (panel.type !== "table") continue;
        const result = toolResult(panel.toolId);
        if (result === undefined) continue;
        const container = node("div", "panel");
        const heading = node("div", "detail-head");
        const titleBox = node("div");
        titleBox.append(node("h3", "", panel.title || ""));
        if (result && panel.meta) {
          const meta = node("div", "event-detail");
          meta.textContent = (panel.meta || [])
            .map((item) => `${item.label || ""} ${displayValue(getPath(result, item.path))}`)
            .join(" · ");
          titleBox.append(meta);
        }
        heading.append(titleBox);
        container.append(heading);
        const rows = getPath(result, panel.rowsPath);
        if (!Array.isArray(rows) || !rows.length) {
          container.append(node("div", "empty", "尚无结构化数据"));
          root.append(container);
          continue;
        }
        const wrap = node("div", "table-wrap");
        const table = node("table");
        const head = node("thead");
        const headRow = node("tr");
        for (const column of panel.columns || []) {
          headRow.append(node("th", "", column.label || ""));
        }
        head.append(headRow);
        const body = node("tbody");
        for (const row of rows) {
          const tr = node("tr");
          for (const column of panel.columns || []) {
            const values = {};
            for (const path of column.paths || (column.path ? [column.path] : [])) {
              values[path] = displayValue(getPath(row, path), column.join);
            }
            const text = column.format
              ? interpolate(column.format, values)
              : displayValue(getPath(row, column.path), column.join);
            tr.append(node("td", column.className || "", text));
          }
          body.append(tr);
        }
        table.append(head, body);
        wrap.append(table);
        container.append(wrap);
        root.append(container);
      }
    }
    function renderApproval() {
      const root = el("approval");
      root.replaceChildren();
      if (!approvalEnabled()) return;
      const approval = state.detail?.pendingApproval;
      if (!approval) return;
      const panel = node("div", "panel approval");
      panel.append(node("h3", "", state.view?.approval?.title || "等待人工审批"));
      panel.append(node("div", "event-detail",
        `审批 ID：${approval.approvalId} · 工具：${approval.toolId}`));
      if (approval.argumentsJson) {
        let argumentsValue = approval.argumentsJson;
        try {
          argumentsValue = JSON.stringify(JSON.parse(approval.argumentsJson), null, 2);
        } catch {}
        panel.append(node("pre", "", argumentsValue));
      }
      const fields = node("div", "approval-fields");
      const scopeLabel = node("label", "", "审批范围（逗号分隔）");
      const scopes = node("input");
      scopes.id = "approvalScopes";
      scopes.value = (approval.requiredScopes || []).join(", ");
      scopeLabel.append(scopes);
      fields.append(scopeLabel);
      panel.append(node("div", "event-detail", "审批身份由 Agent Server 的 AuthProvider 提供。"));
      const buttons = node("div", "button-row");
      const approve = node("button", "", state.view?.approval?.approveLabel || "批准");
      const reject = node("button", "danger", state.view?.approval?.rejectLabel || "拒绝");
      approve.addEventListener("click", () => submitApproval("approved"));
      reject.addEventListener("click", () => submitApproval("rejected"));
      buttons.append(approve, reject);
      panel.append(fields, buttons);
      root.append(panel);
    }
    function renderApprovalDetail() {
      const approval = state.selectedApproval;
      if (!approval) {
        el("approvalDetail").hidden = true;
        return;
      }
      el("empty").hidden = true;
      el("detail").hidden = true;
      el("approvalDetail").hidden = false;
      el("approvalTaskId").textContent = approval.approvalId;
      el("approvalTaskMeta").textContent =
        `${approval.sessionId} · ${approval.messageId} · Revision ${approval.agentRevision}`;
      el("approvalTool").textContent = approval.toolId;
      el("approvalRequester").textContent =
        `申请人：${approval.requester} · Agent：${approval.agent} · 等待时间：${approval.pendingSince}`;
      el("inboxApprovalScopes").value = "";
      el("inboxApprovalComment").value = "";
      el("approvalError").textContent = "";
      let argumentsValue = approval.argumentsJson;
      try {
        argumentsValue = JSON.stringify(JSON.parse(approval.argumentsJson), null, 2);
      } catch {}
      el("approvalArguments").textContent = argumentsValue;
    }
    function renderMessageActions() {
      const root = el("messageActions");
      root.replaceChildren();
      const detail = state.detail;
      if (!detail?.messageId) return;
      const plan = currentPlan();
      if (detail.state === "completed" && plan) {
        const confirm = node(
          "button",
          "",
          state.view?.confirmation?.label || "确认执行"
        );
        confirm.addEventListener("click", confirmCurrentPlan);
        root.append(confirm);
      }
      if (detail.state === "waiting_external") {
        const query = node("button", "", "查询外部结果");
        query.addEventListener("click", () => controlMessage("reconcile", { action: "query" }));
        const cancel = node("button", "secondary", "终止外部恢复");
        cancel.addEventListener("click", () => controlMessage("reconcile", { action: "cancel" }));
        root.append(query, cancel);
      } else if (["queued", "running", "waiting_approval"].includes(detail.state)) {
        const cancel = node("button", "secondary", "停止执行");
        cancel.addEventListener("click", () => controlMessage("cancel", null));
        root.append(cancel);
      }
    }
    function renderConversation() {
      const root = el("conversation");
      const atBottom = root.scrollHeight - root.scrollTop - root.clientHeight < 80;
      root.replaceChildren();
      const messages = state.detail?.messages || [];
      if (!messages.length) {
        root.append(node("div", "event-detail", "尚无对话"));
      }
      for (const message of messages) {
        const userTurn = node("div", "turn user");
        const userMeta = node("div", "turn-meta");
        userMeta.append(
          node("span", "", `你 · ${message.createdAt || ""}`),
          node("span", "", statusNames[message.state] || message.state || "")
        );
        userTurn.append(userMeta, node("div", "turn-content", message.input || ""));
        root.append(userTurn);
        if (message.output) {
          const assistantTurn = node("div", "turn");
          const assistantMeta = node("div", "turn-meta");
          assistantMeta.append(
            node("span", "", "Agent"),
            node("span", "", `Tokens ${message.modelUsage?.totalTokens || 0}`)
          );
          assistantTurn.append(
            assistantMeta,
            node("div", "turn-content", message.output)
          );
          root.append(assistantTurn);
        }
      }
      if (atBottom || !state.detail?.messages?.length) root.scrollTop = root.scrollHeight;
    }
    function renderFloatingConversation() {
      const root = el("floatingConversation");
      root.replaceChildren();
      const messages = state.detail?.messages || [];
      if (!messages.length) {
        const empty = node("div", "floating-empty");
        const content = node("div");
        content.append(
          node("strong", "", state.view?.task?.title || "开始一段对话"),
          node("span", "", state.view?.metadata?.subtitle || "描述任务，或点选下方业务内容")
        );
        empty.append(content);
        root.append(empty);
      }
      for (const [index, message] of messages.entries()) {
        const userTurn = node("div", "floating-turn user");
        const userMeta = node("div", "floating-turn-meta");
        userMeta.append(
          node("span", "", "你"),
          node("span", "", shortTime(message.createdAt))
        );
        userTurn.append(
          userMeta,
          node("div", "floating-bubble", message.input || "")
        );
        root.append(userTurn);
        if (message.output) {
          const assistantTurn = node("div", "floating-turn");
          const assistantMeta = node("div", "floating-turn-meta");
          assistantMeta.append(
            node("span", "", "Agent"),
            node("span", "",
              `${statusNames[message.state] || message.state || ""} · ${message.modelUsage?.totalTokens || 0} tokens`)
          );
          assistantTurn.append(
            assistantMeta,
            node("div", "floating-bubble", message.output)
          );
          root.append(assistantTurn);
        } else if (index === messages.length - 1 && state.detail?.state !== "completed") {
          const assistantTurn = node("div", "floating-turn");
          assistantTurn.append(
            node("div", "floating-turn-meta", "Agent"),
            node("div", "floating-bubble",
              statusNames[state.detail?.state] || state.detail?.state || "处理中")
          );
          root.append(assistantTurn);
        }
      }
      el("floatingSubtitle").textContent = state.detail?.id
        ? `${statusNames[state.detail.state] || state.detail.state} · Revision ${state.detail.agentRevision || "—"}`
        : state.view?.metadata?.subtitle || "会话、处理建议与执行";
    }
    function renderReplyControls() {
      const active = ["queued", "running", "waiting_approval", "waiting_external"]
        .includes(state.detail?.state);
      el("taskSend").disabled = active;
      el("floatingSend").disabled = active;
      el("floatingLauncher").hidden = el("taskForm").hidden;
      if (el("floatingLauncher").hidden) closeFloatingComposer();
      el("taskInput").placeholder = state.detail?.id
        ? "继续当前对话，补充条件或调整方案"
        : state.view?.task?.chatPlaceholder ||
          "告诉 Agent 你想了解什么或要执行什么";
      el("floatingInput").placeholder = state.detail?.id
        ? "继续当前对话，或点选业务快捷内容"
        : state.view?.task?.chatPlaceholder ||
          "描述任务，或点选业务快捷内容";
      el("composerMode").textContent = state.detail?.id
        ? `继续对话 · ${statusNames[state.detail.state] || state.detail.state}`
        : "新对话";
      el("taskHint").textContent = active
        ? "Agent 正在执行当前 Message"
        : "Enter 发送 · Shift+Enter 换行";
      el("workspace").classList.toggle(
        "with-context",
        Boolean(state.detail?.id && !state.selectedApproval && contextEnabled())
      );
      applyContextMode();
    }
    function renderTimeline() {
      const root = el("timeline");
      root.replaceChildren();
      for (const event of state.detail?.events || []) {
        const item = node("div", "event");
        item.append(node("div", "event-type", eventNames[event.type] || event.type));
        if (event.summary) item.append(node("div", "event-detail", event.summary));
        item.append(node("div", "event-time", event.at));
        root.append(item);
      }
    }
    function renderDetail() {
      const detail = state.detail;
      if (!detail) return;
      el("approvalDetail").hidden = true;
      el("empty").hidden = true;
      el("detail").hidden = false;
      const titleResult = toolResult(state.view?.detail?.titleToolId);
      const titleValues = {
        [state.view?.detail?.titlePath || ""]:
          displayValue(getPath(titleResult, state.view?.detail?.titlePath))
      };
      el("detailTitle").textContent =
        interpolate(state.view?.detail?.titleTemplate || "任务详情", titleValues);
      el("sessionId").textContent = detail.id;
      el("revisionRef").textContent =
        `${detail.agent || ""} · Revision ${detail.agentRevision || "—"} · ${detail.messageId || "尚无 Message"}` +
        ` · Tokens ${detail.modelUsage?.totalTokens || 0}`;
      const messageSelect = el("messageSelect");
      messageSelect.replaceChildren();
      for (const message of detail.messages || []) {
        const option = node("option", "", `${statusNames[message.state] || message.state} · ${message.input}`);
        option.value = message.id;
        messageSelect.append(option);
      }
      messageSelect.value = detail.messageId || "";
      messageSelect.hidden = (detail.messages || []).length < 2;
      el("status").textContent = statusNames[detail.state] || detail.state;
      el("status").className = `status ${detail.state}`;
      renderConversation();
      renderFloatingConversation();
      renderReplyControls();
      renderMetrics();
      renderPanels();
      renderApproval();
      renderMessageActions();
      const composition = detail.composition;
      el("compositionSummary").textContent = composition
        ? `${composition.model?.modelId || "unknown"}@${composition.model?.version || "unknown"} · ` +
          `${composition.tools?.length || 0} Tools · ${composition.skills?.length || 0} Skills · ` +
          `${composition.knowledge?.length || 0} Knowledge`
        : "尚无运行证据";
      el("composition").textContent = JSON.stringify(composition, null, 2);
      el("output").textContent = detail.output || "尚无最终输出";
      el("rawResults").textContent = JSON.stringify(detail.toolResults || [], null, 2);
      renderTimeline();
    }
    let detailRequest = 0;
    async function loadDetail() {
      if (!state.selected || state.selectedApproval) return;
      const request = ++detailRequest;
      try {
        const sameMessage =
          state.detail?.messageId && state.detail.messageId === state.selectedMessage;
        const params = new URLSearchParams();
        if (state.selectedMessage) params.set("messageId", state.selectedMessage);
        if (sameMessage) params.set("eventAfter", String(state.detail.eventCursor || 0));
        const query = params.toString();
        const suffix = query ? `?${query}` : "";
        const detail = await api(`/api/sessions/${state.selected}${suffix}`);
        if (request !== detailRequest) return;
        if (sameMessage) {
          detail.events = [...(state.detail.events || []), ...(detail.events || [])];
        }
        state.detail = detail;
        state.selectedMessage = state.detail.messageId;
        renderDetail();
        connectStream();
      } catch (error) {
        el("createError").textContent = error.message;
      }
    }
    async function createSession(input, button) {
      const value = input.trim();
      if (!value) {
        el("createError").textContent = "任务内容不能为空";
        return;
      }
      el("createError").textContent = "";
      button.disabled = true;
      try {
        const created = await api("/api/sessions", {
          method: "POST",
          body: JSON.stringify({ input: value, requestId: requestId() })
        });
        state.selectedApproval = null;
        state.selected = created.sessionId;
        state.selectedMessage = created.messageId;
        setComposerDraft("");
        el("empty").hidden = true;
        el("detail").hidden = false;
        await loadSessions(false);
        await loadDetail();
      } catch (error) {
        el("createError").textContent = error.message;
      } finally {
        button.disabled = false;
      }
    }
    function applyTask(action, fromFloating = false) {
      const values = {};
      for (const field of state.view?.task?.fields || []) {
        values[field.name] = el(`taskField-${field.name}`)?.value || "";
      }
      setComposerDraft(interpolate(action.inputTemplate, values));
      el("taskHint").textContent = "已填入业务指令，确认后可发送";
      el("quickPanel").hidden = true;
      el("quickToggle").setAttribute("aria-expanded", "false");
      if (fromFloating) {
        el("floatingQuickSection").hidden = true;
        el("floatingQuickAdd").setAttribute("aria-expanded", "false");
        el("workspace").classList.remove("quick-palette-open");
      }
      (fromFloating ? el("floatingInput") : el("taskInput")).focus();
    }
    async function submitComposer(button) {
      const input = el("taskInput").value;
      if (state.selected && state.detail?.id) {
        await sendSessionMessage(input, button);
      } else {
        await createSession(input, button);
      }
    }
    async function sendSessionMessage(input, button) {
      if (!state.detail?.id) return;
      const value = input.trim();
      if (!value) {
        el("createError").textContent = "消息内容不能为空";
        return;
      }
      el("createError").textContent = "";
      button.disabled = true;
      try {
        const created = await api(`/api/sessions/${state.detail.id}/messages`, {
          method: "POST",
          body: JSON.stringify({ input: value })
        });
        state.selectedMessage = created.messageId;
        setComposerDraft("");
        await loadSessions(false);
        await loadDetail();
      } catch (error) {
        el("createError").textContent = error.message;
      } finally {
        renderReplyControls();
      }
    }
    async function submitApproval(decision) {
      const approval = state.detail?.pendingApproval;
      if (!approval) return;
      const scopes = (el("approvalScopes")?.value || "")
        .split(",").map((value) => value.trim()).filter(Boolean);
      try {
        await api(`/api/sessions/${state.detail.id}/messages/${state.detail.messageId}/approval`, {
          method: "POST",
          body: JSON.stringify({
            approvalId: approval.approvalId,
            decision,
            scopes,
            comment: decision === "approved" ? "WebUI 审批通过" : "WebUI 审批拒绝"
          })
        });
        await loadDetail();
        await loadSessions(false);
      } catch (error) {
        el("createError").textContent = error.message;
      }
    }
    async function submitInboxApproval(decision) {
      const approval = state.selectedApproval;
      if (!approval) return;
      const scopes = (el("inboxApprovalScopes")?.value || "")
        .split(",").map((value) => value.trim()).filter(Boolean);
      if (decision === "approved" && !scopes.length) {
        el("approvalError").textContent = "批准操作至少需要一个审批范围";
        return;
      }
      el("approvalError").textContent = "";
      try {
        await api(
          `/api/sessions/${approval.sessionId}/messages/${approval.messageId}/approval`,
          {
            method: "POST",
            body: JSON.stringify({
              approvalId: approval.approvalId,
              decision,
              scopes,
              comment: el("inboxApprovalComment").value ||
                (decision === "approved" ? "WebUI 审批通过" : "WebUI 审批拒绝")
            })
          }
        );
        await loadApprovals();
        startNewChat();
        await loadSessions(false);
      } catch (error) {
        el("approvalError").textContent = error.message;
      }
    }
    async function controlMessage(action, body) {
      if (!state.detail?.messageId) return;
      el("createError").textContent = "";
      try {
        await api(`/api/messages/${state.detail.messageId}/${action}`, {
          method: "POST",
          body: body ? JSON.stringify(body) : undefined
        });
        await loadDetail();
        await loadSessions(false);
      } catch (error) {
        el("createError").textContent = error.message;
      }
    }
    async function confirmCurrentPlan() {
      const confirmation = currentConfirmation();
      if (!state.detail?.id || !state.detail?.messageId || !confirmation) {
        el("createError").textContent = "当前 Message 没有可确认的结构化业务对象";
        return;
      }
      el("createError").textContent = "";
      try {
        const created = await api(
          `/api/sessions/${state.detail.id}/messages/${state.detail.messageId}/confirm`,
          {
            method: "POST",
            body: JSON.stringify(confirmation)
          }
        );
        state.selectedMessage = created.messageId;
        await loadSessions(false);
        await loadDetail();
      } catch (error) {
        el("createError").textContent = error.message;
      }
    }
    el("newChat").addEventListener("click", startNewChat);
    el("sidebarToggle").addEventListener("click", toggleSidebar);
    el("contextToggle").addEventListener("click", toggleContextWidth);
    el("floatingLauncher").addEventListener("click", toggleFloatingComposer);
    el("floatingClose").addEventListener("click", closeFloatingComposer);
    el("floatingQuickAdd").addEventListener("click", () => {
      const section = el("floatingQuickSection");
      section.hidden = !section.hidden;
      el("floatingQuickAdd").setAttribute("aria-expanded", String(!section.hidden));
      el("workspace").classList.toggle("quick-palette-open", !section.hidden);
    });
    el("floatingSend").addEventListener("click", async () => {
      setComposerDraft(el("floatingInput").value);
      closeFloatingComposer();
      await submitComposer(el("taskSend"));
    });
    window.addEventListener("resize", renderLayoutControls);
    el("quickToggle").addEventListener("click", () => {
      const panel = el("quickPanel");
      panel.hidden = !panel.hidden;
      el("quickToggle").setAttribute("aria-expanded", String(!panel.hidden));
    });
    el("refresh").addEventListener("click", async () => {
      await loadSessions(false);
      await loadApprovals();
      await loadDetail();
    });
    el("logout").addEventListener("click", async () => {
      try {
        const result = await api("/auth/logout", { method: "POST" });
        window.location.href = result.redirect || "/";
      } catch (error) {
        el("createError").textContent = error.message;
      }
    });
    el("inboxApprove").addEventListener("click", () => submitInboxApproval("approved"));
    el("inboxReject").addEventListener("click", () => submitInboxApproval("rejected"));
    el("taskSend").addEventListener("click", () => submitComposer(el("taskSend")));
    el("taskInput").addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        submitComposer(el("taskSend"));
      }
    });
    el("taskInput").addEventListener("input", () => {
      el("floatingInput").value = el("taskInput").value;
      resizeComposer();
    });
    el("floatingInput").addEventListener("input", () => {
      el("taskInput").value = el("floatingInput").value;
      resizeComposer();
    });
    el("floatingInput").addEventListener("keydown", (event) => {
      if (event.key === "Enter" && !event.shiftKey && !event.isComposing) {
        event.preventDefault();
        el("floatingSend").click();
      }
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape" && !el("floatingComposer").hidden) {
        closeFloatingComposer();
        el("floatingLauncher").focus();
      }
    });
    el("messageSelect").addEventListener("change", async (event) => {
      state.selectedMessage = event.target.value;
      await loadDetail();
    });
    el("taskForm").addEventListener("submit", (event) => {
      event.preventDefault();
      submitComposer(el("taskSend"));
    });
    async function initialize() {
      try {
        restoreLayout();
        state.me = await api("/api/me");
        renderUser();
        state.view = await api("/api/view");
        renderView();
        await Promise.all([loadSessions(false), loadApprovals()]);
        renderReplyControls();
      } catch (error) {
        el("createError").textContent = error.message;
      }
    }
    let eventSource = null;
    let streamKey = "";
    let refreshTimer = null;
    let fallbackTimer = null;
    function closeStream() {
      if (eventSource) eventSource.close();
      eventSource = null;
      streamKey = "";
    }
    function scheduleSnapshotRefresh() {
      if (refreshTimer) clearTimeout(refreshTimer);
      refreshTimer = setTimeout(async () => {
        refreshTimer = null;
        await Promise.all([loadSessions(false), loadApprovals()]);
        await loadDetail();
      }, 120);
    }
    function stopFallback() {
      if (fallbackTimer) clearTimeout(fallbackTimer);
      fallbackTimer = null;
    }
    function startFallback() {
      if (fallbackTimer) return;
      fallbackTimer = setTimeout(async () => {
        fallbackTimer = null;
        await Promise.all([loadSessions(false), loadApprovals()]);
        await loadDetail();
      }, 5000);
    }
    function connectStream() {
      if (document.hidden || !state.detail?.id || !state.detail?.messageId) {
        closeStream();
        return;
      }
      const key = `${state.detail.id}:${state.detail.messageId}`;
      if (eventSource && streamKey === key && eventSource.readyState !== EventSource.CLOSED) {
        return;
      }
      closeStream();
      if (!("EventSource" in window)) {
        startFallback();
        return;
      }
      streamKey = key;
      const after = state.detail.eventCursor || 0;
      eventSource = new EventSource(
        `/api/sessions/${state.detail.id}/messages/${state.detail.messageId}/stream?after=${after}`
      );
      eventSource.addEventListener("open", stopFallback);
      eventSource.addEventListener("runtime", (message) => {
        if (streamKey !== key) return;
        const event = JSON.parse(message.data);
        if (!(state.detail.events || []).some((item) => item.sequence === event.sequence)) {
          state.detail.events = [...(state.detail.events || []), {
            sequence: event.sequence,
            type: event.eventType,
            at: event.at,
            data: event.data,
            summary: ""
          }];
          state.detail.eventCursor = event.sequence;
          renderTimeline();
        }
        if ([
          "message.completed",
          "message.failed",
          "message.cancelled",
          "approval.requested",
          "approval.granted",
          "approval.rejected"
        ].includes(event.eventType)) {
          scheduleSnapshotRefresh();
        }
      });
      eventSource.addEventListener("message", (message) => {
        if (streamKey !== key) return;
        const snapshot = JSON.parse(message.data);
        state.detail.state = snapshot.state;
        state.detail.pendingApproval = snapshot.pendingApproval;
        state.detail.approval = snapshot.approval;
        state.detail.output = snapshot.output;
        state.detail.modelUsage = snapshot.modelUsage;
        el("status").textContent = statusNames[snapshot.state] || snapshot.state;
        el("status").className = `status ${snapshot.state}`;
        renderReplyControls();
        renderApproval();
        scheduleSnapshotRefresh();
      });
      eventSource.addEventListener("error", () => {
        if (eventSource?.readyState === EventSource.CLOSED) startFallback();
      });
    }
    document.addEventListener("visibilitychange", () => {
      if (document.hidden) {
        closeStream();
        stopFallback();
      } else {
        loadDetail().finally(connectStream);
      }
    });
    initialize();
  
