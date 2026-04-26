const state = {
  meta: null,
  update: null,
  status: null,
  config: null,
  logs: null,
  activeView: "overview",
};

const els = {
  refreshAll: document.getElementById("refresh-all"),
  refreshLogs: document.getElementById("refresh-logs"),
  lastRefresh: document.getElementById("last-refresh"),
  serviceStatePill: document.getElementById("service-state-pill"),
  serviceState: document.getElementById("service-state"),
  servicePid: document.getElementById("service-pid"),
  serviceRecoverable: document.getElementById("service-recoverable"),
  serviceStale: document.getElementById("service-stale"),
  serviceActionResult: document.getElementById("service-action-result"),
  nodeChainId: document.getElementById("node-chain-id"),
  nodeId: document.getElementById("node-id"),
  nodeRewardAddress: document.getElementById("node-reward-address"),
  nodeDataDir: document.getElementById("node-data-dir"),
  nodeNextEpoch: document.getElementById("node-next-epoch"),
  nodeNextSlot: document.getElementById("node-next-slot"),
  nodeTicksCompleted: document.getElementById("node-ticks-completed"),
  nodeLowImpact: document.getElementById("node-low-impact"),
  nodeInlineVerify: document.getElementById("node-inline-verify"),
  nodeInlinePropose: document.getElementById("node-inline-propose"),
  configPath: document.getElementById("config-path"),
  targetAppIds: document.getElementById("target-app-ids"),
  gameProcessNames: document.getElementById("game-process-names"),
  rewardSource: document.getElementById("reward-source"),
  emissionYear: document.getElementById("emission-year"),
  lowImpactMode: document.getElementById("low-impact-mode"),
  osBackgroundPriority: document.getElementById("os-background-priority"),
  configForm: document.getElementById("config-form"),
  configStatus: document.getElementById("config-status"),
  logsList: document.getElementById("logs-list"),
  metaAppName: document.getElementById("meta-app-name"),
  metaAppVersion: document.getElementById("meta-app-version"),
  metaPlatform: document.getElementById("meta-platform"),
  metaServiceManager: document.getElementById("meta-service-manager"),
  metaDefaultBind: document.getElementById("meta-default-bind"),
  metaRemoteDefault: document.getElementById("meta-remote-default"),
  metaConfigDir: document.getElementById("meta-config-dir"),
  metaLogDir: document.getElementById("meta-log-dir"),
  metaUpdateDir: document.getElementById("meta-update-dir"),
  updateVersionBadge: document.getElementById("update-version-badge"),
  updateCurrentVersion: document.getElementById("update-current-version"),
  updateDir: document.getElementById("update-dir"),
  updateChannel: document.getElementById("update-channel"),
  updateRollback: document.getElementById("update-rollback"),
  updateSummary: document.getElementById("update-summary"),
  updateActionStatus: document.getElementById("update-action-status"),
  updateActionResult: document.getElementById("update-action-result"),
  stageUpdate: document.getElementById("stage-update"),
  applyUpdate: document.getElementById("apply-update"),
  commitInstall: document.getElementById("commit-install"),
  rollbackUpdate: document.getElementById("rollback-update"),
  installRootOverride: document.getElementById("install-root-override"),
  installedLayoutRootOverride: document.getElementById(
    "installed-layout-root-override",
  ),
  useInstalledLayout: document.getElementById("use-installed-layout"),
  allowSystemInstallWrite: document.getElementById("allow-system-install-write"),
  stopServiceBeforeInstall: document.getElementById("stop-service-before-install"),
  startServiceAfterInstall: document.getElementById("start-service-after-install"),
  stopServiceBeforeRollback: document.getElementById("stop-service-before-rollback"),
  startServiceAfterRollback: document.getElementById("start-service-after-rollback"),
  viewTabs: Array.from(document.querySelectorAll("[data-view-target]")),
  viewPanes: Array.from(document.querySelectorAll("[data-view]")),
};

async function requestJson(path, options = {}) {
  const response = await fetch(path, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `请求失败: ${response.status}`);
  }
  return response.json();
}

function setText(element, value) {
  element.textContent = value ?? "-";
}

function boolLabel(value) {
  return value ? "已启用" : "已禁用";
}

function renderStatus(status) {
  state.status = status;
  const service = status.service;
  const node = status.node;

  setText(els.serviceState, service.state);
  setText(els.servicePid, service.pid ?? "-");
  setText(
    els.serviceRecoverable,
    service.recoverable_without_manual_cleanup ? "是" : "否",
  );
  setText(els.serviceStale, service.stale ? "是" : "否");
  setText(els.nodeChainId, node.chain_id);
  setText(els.nodeId, node.node_id);
  setText(els.nodeRewardAddress, node.reward_address);
  setText(els.nodeDataDir, node.data_dir);
  setText(els.nodeNextEpoch, String(node.next_epoch_id));
  setText(els.nodeNextSlot, String(node.next_slot_id));
  setText(els.nodeTicksCompleted, String(node.ticks_completed));
  setText(els.nodeLowImpact, boolLabel(node.low_impact_mode));
  setText(els.nodeInlineVerify, boolLabel(node.inline_verify_enabled));
  setText(els.nodeInlinePropose, boolLabel(node.inline_propose_enabled));

  els.serviceStatePill.textContent = service.state;
  els.serviceStatePill.dataset.state = service.state;
}

function renderConfig(response) {
  state.config = response;
  const config = response.config;
  setText(els.configPath, config.config_path);
  els.targetAppIds.value = config.target_app_ids.join(",");
  els.gameProcessNames.value = config.game_process_names.join(",");
  els.rewardSource.value = config.reward_source;
  els.emissionYear.value = config.emission_year;
  els.lowImpactMode.checked = Boolean(config.low_impact_mode);
  els.osBackgroundPriority.checked = Boolean(config.os_background_priority);
}

function renderMeta(response) {
  state.meta = response;
  const meta = response.app;
  const layout = meta.install_layout;
  setText(els.metaAppName, meta.app_name);
  setText(els.metaAppVersion, meta.app_version);
  setText(els.metaPlatform, `${layout.platform} / ${layout.mode}`);
  setText(els.metaServiceManager, meta.service_manager);
  setText(els.metaDefaultBind, meta.control_api_default_bind_addr);
  setText(
    els.metaRemoteDefault,
    meta.remote_access_default_enabled ? "已启用" : "已禁用",
  );
  setText(els.metaConfigDir, layout.config_dir);
  setText(els.metaLogDir, layout.log_dir);
  setText(els.metaUpdateDir, layout.update_dir);
}

function renderUpdate(response) {
  state.update = response;
  const update = response.update;
  setText(els.updateCurrentVersion, update.current_version);
  setText(els.updateDir, update.update_dir);
  setText(els.updateChannel, update.channel);
  setText(els.updateRollback, update.rollback_status);
  setText(
    els.updateSummary,
    `清单: ${update.latest_manifest_path} | 产物数: ${update.artifact_count} | 签名: ${update.signing_status} | 最新可用: ${update.latest_available_version ?? "尚未发布"} | 待处理目标: ${update.pending_target_version ?? "无"} | 已应用目标: ${update.applied_target_version ?? "无"} | 选中产物: ${update.selected_artifact_kind ?? "无"}:${update.selected_artifact_path ?? "无"} | 已执行产物: ${update.executed_artifact_path ?? "无"} | 计划安装: ${update.planned_install_path ?? "无"} | 备份路径: ${update.planned_backup_path ?? "无"} | 已完成安装: ${update.executed_install_path ?? "无"} | 目标模式: ${update.install_target_mode ?? "无"} | 安装操作: ${update.install_action_status} | 切换状态: ${update.switch_execution_status} | 服务窗口: ${update.service_window_status}。`,
  );
  els.updateVersionBadge.textContent = update.update_available
    ? "有可用更新"
    : update.current_version;
}

function renderLogs(response) {
  state.logs = response;
  if (!response.logs.length) {
    els.logsList.innerHTML = "<article><pre>没有可用的日志文件。</pre></article>";
    return;
  }
  els.logsList.innerHTML = response.logs
    .map(
      (entry) => `
        <article>
          <header>
            <strong>${entry.source}</strong>
            <code>${entry.path}</code>
          </header>
          <pre>${escapeHtml(entry.text || "(空)")}</pre>
        </article>
      `,
    )
    .join("");
}

function escapeHtml(text) {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function serviceRequestPayload() {
  return {};
}

function syncActiveView(nextView) {
  state.activeView = nextView;
  els.viewTabs.forEach((tab) => {
    tab.classList.toggle("active", tab.dataset.viewTarget === nextView);
  });
  els.viewPanes.forEach((pane) => {
    pane.classList.toggle("is-visible", pane.dataset.view === nextView);
  });
  window.location.hash = nextView;
}

function initializeViewSelection() {
  const requested = window.location.hash.replace("#", "");
  const knownView = els.viewPanes.some((pane) => pane.dataset.view === requested)
    ? requested
    : "overview";
  syncActiveView(knownView);
}

async function refreshAll() {
  const [meta, update, status, config, logs] = await Promise.all([
    requestJson("/api/meta"),
    requestJson("/api/update"),
    requestJson("/api/status"),
    requestJson("/api/config"),
    requestJson("/api/logs"),
  ]);
  renderMeta(meta);
  renderUpdate(update);
  renderStatus(status);
  renderConfig(config);
  renderLogs(logs);
  els.lastRefresh.textContent = `上次同步: ${new Date().toLocaleString("zh-CN")}`;
}

async function runServiceAction(action) {
  els.serviceActionResult.textContent = `正在执行 ${action} ...`;
  try {
    const result = await requestJson(`/api/service/${action}`, {
      method: "POST",
      body: JSON.stringify(serviceRequestPayload()),
    });
    els.serviceActionResult.textContent = JSON.stringify(result, null, 2);
    const status = await requestJson("/api/status");
    renderStatus(status);
  } catch (error) {
    els.serviceActionResult.textContent = `服务操作失败: ${error.message}`;
  }
}

async function stageUpdate() {
  await runUpdateAction("stage", "正在准备更新...");
}

async function applyUpdate() {
  await runUpdateAction("apply", "正在应用更新...");
}

async function rollbackUpdate() {
  await runUpdateAction("rollback", "正在回滚...");
}

async function commitInstall() {
  await runUpdateAction("commit-install", "正在准备安装...");
}

async function runUpdateAction(action, pendingMessage) {
  els.updateActionStatus.textContent = pendingMessage;
  els.updateActionResult.textContent = pendingMessage;
  try {
    const payload =
      action === "commit-install"
        ? {
            channel: "stable",
            install_root_override:
              els.installRootOverride.value.trim() || null,
            installed_layout_root_override:
              els.installedLayoutRootOverride.value.trim() || null,
            use_installed_layout: els.useInstalledLayout.checked,
            allow_system_install_write: els.allowSystemInstallWrite.checked,
            stop_service_before_install: els.stopServiceBeforeInstall.checked,
            start_service_after_install: els.startServiceAfterInstall.checked,
          }
        : action === "rollback"
          ? {
              channel: "stable",
              stop_service_before_rollback: els.stopServiceBeforeRollback.checked,
              start_service_after_rollback: els.startServiceAfterRollback.checked,
            }
        : { channel: "stable" };
    const result = await requestJson(`/api/update/${action}`, {
      method: "POST",
      body: JSON.stringify(payload),
    });
    els.updateActionResult.textContent = JSON.stringify(result, null, 2);
    els.updateActionStatus.textContent = `更新操作: ${result.status}`;
    const update = await requestJson("/api/update");
    renderUpdate(update);
  } catch (error) {
    els.updateActionStatus.textContent = `更新操作失败: ${error.message}`;
    els.updateActionResult.textContent = `更新操作失败: ${error.message}`;
  }
}

function parseListInput(value) {
  return value
    .split(",")
    .map((entry) => entry.trim())
    .filter(Boolean);
}

async function saveConfig(event) {
  event.preventDefault();
  els.configStatus.textContent = "正在保存...";
  const payload = {
    target_app_ids: parseListInput(els.targetAppIds.value).map((value) =>
      Number.parseInt(value, 10),
    ),
    game_process_names: parseListInput(els.gameProcessNames.value),
    reward_source: els.rewardSource.value,
    emission_year: Number.parseInt(els.emissionYear.value || "0", 10),
    low_impact_mode: els.lowImpactMode.checked,
    os_background_priority: els.osBackgroundPriority.checked,
  };

  try {
    const response = await requestJson("/api/config", {
      method: "POST",
      body: JSON.stringify(payload),
    });
    renderConfig(response);
    els.configStatus.textContent = "配置已保存";
    const status = await requestJson("/api/status");
    renderStatus(status);
  } catch (error) {
    els.configStatus.textContent = `保存失败: ${error.message}`;
  }
}

els.refreshAll.addEventListener("click", () => {
  refreshAll().catch((error) => {
    els.lastRefresh.textContent = `刷新失败: ${error.message}`;
  });
});

els.refreshLogs.addEventListener("click", () => {
  requestJson("/api/logs")
    .then(renderLogs)
    .catch((error) => {
      els.logsList.innerHTML = `<article><pre>日志刷新失败: ${escapeHtml(error.message)}</pre></article>`;
    });
});

els.stageUpdate.addEventListener("click", stageUpdate);
els.applyUpdate.addEventListener("click", applyUpdate);
els.commitInstall.addEventListener("click", commitInstall);
els.rollbackUpdate.addEventListener("click", rollbackUpdate);

document
  .querySelectorAll("[data-service-action]")
  .forEach((button) =>
    button.addEventListener("click", () =>
      runServiceAction(button.dataset.serviceAction),
    ),
  );

els.viewTabs.forEach((tab) => {
  tab.addEventListener("click", () => {
    syncActiveView(tab.dataset.viewTarget);
  });
});

els.configForm.addEventListener("submit", saveConfig);
window.addEventListener("hashchange", initializeViewSelection);

initializeViewSelection();
refreshAll().catch((error) => {
  els.lastRefresh.textContent = `初始加载失败: ${error.message}`;
});
