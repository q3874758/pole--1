const state = {
  dashboard: null,
  logs: null,
  blockchain: null,
  activeView: "overview",
};

const els = {};

function initElements() {
  els.refreshAll = document.getElementById("refresh-all");
  els.lastRefresh = document.getElementById("last-refresh");
  els.serviceStatePill = document.getElementById("service-state-pill");
  els.serviceState = document.getElementById("service-state");
  els.servicePid = document.getElementById("service-pid");
  els.serviceRecoverable = document.getElementById("service-recoverable");
  els.serviceStale = document.getElementById("service-stale");
  els.serviceActionResult = document.getElementById("service-action-result");

  els.nodeId = document.getElementById("node-id");
  els.rewardAddress = document.getElementById("reward-address");
  els.chainId = document.getElementById("chain-id");
  els.appVersion = document.getElementById("app-version");

  els.chainStatusPill = document.getElementById("chain-status-pill");
  els.blockHeight = document.getElementById("block-height");
  els.bcChainId = document.getElementById("bc-chain-id");
  els.grpcStatus = document.getElementById("grpc-status");
  els.httpStatus = document.getElementById("http-status");
  els.blockHash = document.getElementById("block-hash");
  els.blockTime = document.getElementById("block-time");

  els.nextEpoch = document.getElementById("next-epoch");
  els.nextSlot = document.getElementById("next-slot");
  els.ticksCompleted = document.getElementById("ticks-completed");
  els.lowImpactMode = document.getElementById("low-impact-mode");
  els.inlineVerify = document.getElementById("inline-verify");
  els.inlinePropose = document.getElementById("inline-propose");

  els.totalSupply = document.getElementById("total-supply");
  els.annualRate = document.getElementById("annual-rate");
  els.currentYear = document.getElementById("current-year");
  els.emissionYearEl = document.getElementById("emission-year");
  els.playerReward = document.getElementById("player-reward");
  els.serviceReward = document.getElementById("service-reward");
  els.blockReward = document.getElementById("block-reward");
  els.tailEmission = document.getElementById("tail-emission");

  els.totalSize = document.getElementById("total-size");
  els.batchCount = document.getElementById("batch-count");
  els.epochCount = document.getElementById("epoch-count");
  els.payloadCount = document.getElementById("payload-count");
  els.preparedCount = document.getElementById("prepared-count");
  els.settlementCount = document.getElementById("settlement-count");
  els.logCount = document.getElementById("log-count");
  els.dbSize = document.getElementById("db-size");
  els.dataDir = document.getElementById("data-dir");

  els.p2pMode = document.getElementById("p2p-mode");
  els.localPeerId = document.getElementById("local-peer-id");
  els.connectedPeers = document.getElementById("connected-peers");
  els.peersList = document.getElementById("peers-list");

  els.targetAppIds = document.getElementById("target-app-ids");
  els.gameProcesses = document.getElementById("game-processes");
  els.rewardSourceEl = document.getElementById("reward-source");
  els.bgPriority = document.getElementById("bg-priority");

  els.activeChallenges = document.getElementById("active-challenges");
  els.completedChallenges = document.getElementById("completed-challenges");
  els.failedChallenges = document.getElementById("failed-challenges");
  els.lastChallengeEpoch = document.getElementById("last-challenge-epoch");

  els.updateCurrentVersion = document.getElementById("update-current-version");
  els.updateChannel = document.getElementById("update-channel");
  els.updateAvailable = document.getElementById("update-available");
  els.updateVersionBadge = document.getElementById("update-version-badge");

  els.logsList = document.getElementById("logs-list");
  els.refreshLogs = document.getElementById("refresh-logs");
}

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
  if (element) {
    element.textContent = value ?? "-";
  }
}

function boolLabel(value) {
  return value ? "是" : "否";
}

function boolLabelEnabled(value) {
  return value ? "已启用" : "已禁用";
}

function renderDashboard(data) {
  state.dashboard = data;
  const d = data.dashboard;
  const service = d.service;
  const node = d.node;
  const storage = d.storage;
  const tokenomics = d.tokenomics;
  const network = d.network;
  const challenge = d.challenge_activity;
  const config = d.config;

  setText(els.serviceState, service.state);
  els.serviceState.textContent = service.state;
  els.serviceState.dataset.state = service.state;
  els.serviceStatePill.textContent = service.state;
  els.serviceStatePill.dataset.state = service.state;
  setText(els.servicePid, service.pid ?? "-");
  setText(els.serviceRecoverable, boolLabel(service.recoverable_without_manual_cleanup));
  setText(els.serviceStale, boolLabel(service.stale));

  setText(els.nodeId, node.node_id);
  setText(els.rewardAddress, node.reward_address);
  setText(els.chainId, node.chain_id);
  setText(els.appVersion, d.current_version);

  setText(els.nextEpoch, String(node.next_epoch_id));
  setText(els.nextSlot, String(node.next_slot_id));
  setText(els.ticksCompleted, String(node.ticks_completed));
  setText(els.lowImpactMode, boolLabelEnabled(node.low_impact_mode));
  setText(els.inlineVerify, boolLabelEnabled(node.inline_verify_enabled));
  setText(els.inlinePropose, boolLabelEnabled(node.inline_propose_enabled));

  setText(els.totalSupply, tokenomics.total_supply);
  setText(els.annualRate, `${tokenomics.annual_emission_rate_bps / 100}%`);
  setText(els.currentYear, String(tokenomics.current_year));
  setText(els.emissionYearEl, String(tokenomics.emission_year));
  setText(els.playerReward, tokenomics.player_reward_budget_per_hour);
  setText(els.serviceReward, tokenomics.service_reward_budget_per_hour);
  setText(els.blockReward, tokenomics.player_block_reward);
  setText(els.tailEmission, tokenomics.tail_emission_active ? `已启用 (${tokenomics.tail_emission_rate_bps} bps)` : "已禁用");

  setText(els.totalSize, storage.total_size_formatted);
  setText(els.batchCount, String(storage.batch_count));
  setText(els.epochCount, String(storage.epoch_count));
  setText(els.payloadCount, String(storage.payload_count));
  setText(els.preparedCount, String(storage.prepared_epoch_count));
  setText(els.settlementCount, String(storage.settlement_count));
  setText(els.logCount, String(storage.log_files_count));
  setText(els.dbSize, formatBytes(storage.db_size_bytes));
  setText(els.dataDir, storage.data_dir);

  setText(els.p2pMode, network.mode);
  setText(els.localPeerId, network.local_peer_id);
  setText(els.connectedPeers, String(network.connected_peers));

  if (network.peers && network.peers.length > 0) {
    els.peersList.innerHTML = network.peers.map(peer => `
      <div class="peer-item">
        <span class="peer-id">${peer.peer_id.substring(0, 16)}...</span>
        <span class="peer-status">${peer.connected ? "已连接" : "未连接"}</span>
      </div>
    `).join("");
  } else {
    els.peersList.innerHTML = '<p class="muted">暂无连接的节点</p>';
  }

  setText(els.targetAppIds, config.target_app_ids.join(", ") || "-");
  setText(els.gameProcesses, config.game_process_names.join(", ") || "-");
  setText(els.rewardSourceEl, config.reward_source);
  setText(els.bgPriority, boolLabelEnabled(config.os_background_priority));

  setText(els.activeChallenges, String(challenge.active_challenges));
  setText(els.completedChallenges, String(challenge.completed_challenges));
  setText(els.failedChallenges, String(challenge.failed_challenges));
  setText(els.lastChallengeEpoch, challenge.last_challenge_epoch > 0 ? String(challenge.last_challenge_epoch) : "-");

  setText(els.updateCurrentVersion, d.current_version);
  setText(els.updateChannel, "stable");
  setText(els.updateAvailable, d.update_available ? "是" : "否");
  els.updateVersionBadge.textContent = d.update_available ? "有可用更新" : "当前";
}

function renderBlockchain(data) {
  state.blockchain = data;
  const bc = data.blockchain;
  const status = bc.online ? "运行中" : "离线";
  els.chainStatusPill.textContent = status;
  els.chainStatusPill.dataset.state = bc.online ? "running" : "offline";
  setText(els.blockHeight, bc.block_height > 0 ? String(bc.block_height) : "-");
  setText(els.bcChainId, bc.chain_id || "-");
  setText(els.grpcStatus, bc.grpc_online ? "在线" : "离线");
  setText(els.httpStatus, bc.http_online ? "在线" : "离线");
  setText(els.blockHash, bc.block_hash ? bc.block_hash.substring(0, 16) + "..." : "-");
  setText(els.blockTime, bc.block_time ? bc.block_time.replace("T", " ").replace("Z", "") : "-");
}

async function refreshBlockchain() {
  try {
    const data = await requestJson("/api/blockchain");
    renderBlockchain(data);
  } catch (error) {
    if (state.blockchain === null) {
      renderBlockchain({ blockchain: { online: false, block_height: 0, block_hash: "", chain_id: "", http_online: false, grpc_online: false, block_time: "" } });
    }
  }
}

function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

async function runServiceAction(action) {
  els.serviceActionResult.textContent = `正在执行 ${action} ...`;
  try {
    const result = await requestJson(`/api/service/${action}`, {
      method: "POST",
      body: JSON.stringify({}),
    });
    els.serviceActionResult.textContent = JSON.stringify(result, null, 2);
    await refreshDashboard();
  } catch (error) {
    els.serviceActionResult.textContent = `服务操作失败: ${error.message}`;
  }
}

async function refreshDashboard() {
  try {
    const data = await requestJson("/api/dashboard");
    renderDashboard(data);
    els.lastRefresh.textContent = `上次同步: ${new Date().toLocaleString("zh-CN")}`;
  } catch (error) {
    els.lastRefresh.textContent = `同步失败: ${error.message}`;
  }
}

async function refreshAll() {
  await refreshDashboard();
  await refreshBlockchain();
}

async function refreshLogs() {
  try {
    const data = await requestJson("/api/logs");
    renderLogs(data);
  } catch (error) {
    els.logsList.innerHTML = `<article><pre>日志刷新失败: ${escapeHtml(error.message)}</pre></article>`;
  }
}

function renderLogs(response) {
  state.logs = response;
  if (!response.logs || !response.logs.length) {
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

function initEventListeners() {
  if (els.refreshAll) {
    els.refreshAll.addEventListener("click", () => {
      refreshAll().catch((error) => {
        els.lastRefresh.textContent = `刷新失败: ${error.message}`;
      });
    });
  }

  if (els.refreshLogs) {
    els.refreshLogs.addEventListener("click", refreshLogs);
  }

  document.querySelectorAll("[data-service-action]").forEach((button) => {
    button.addEventListener("click", () => {
      runServiceAction(button.dataset.serviceAction);
    });
  });
}

document.addEventListener("DOMContentLoaded", () => {
  initElements();
  initEventListeners();

  refreshDashboard().catch((error) => {
    els.lastRefresh.textContent = `初始加载失败: ${error.message}`;
  });
  refreshBlockchain().catch(() => {});

  setInterval(refreshDashboard, 30000);
  setInterval(refreshBlockchain, 15000);
});
