/**
 * PoLE Protocol - Esports / Cyberpunk Gamer Web Dashboard JS
 * v0.1.3 - Proof of Live Engagement
 */

const state = {
  gaming: null,
  dashboard: null,
  blockchain: null,
  storage: null,
  logs: [],
  rewardAddress: "",
  autoRefreshTimer: null,
  isRefreshing: false,
};

// UI Elements Cache
const els = {};

function initElements() {
  // Header
  els.appVersionBadge = document.getElementById("app-version-badge");
  els.headerServiceLabel = document.getElementById("header-service-label");
  els.chipDaemon = document.getElementById("chip-daemon");
  els.autoRefreshCb = document.getElementById("auto-refresh-cb");
  els.btnManualRefresh = document.getElementById("btn-manual-refresh");
  els.toast = document.getElementById("toast");

  // Hero Bento - Left (Game)
  els.gameEngagementBadge = document.getElementById("game-engagement-badge");
  els.gameAvatarIcon = document.getElementById("game-avatar-icon");
  els.currentGameTitle = document.getElementById("current-game-title");
  els.currentGamePid = document.getElementById("current-game-pid");
  els.gameEngagementDesc = document.getElementById("game-engagement-desc");
  els.nodeMemoryUsage = document.getElementById("node-memory-usage");

  els.totalPlayerReward = document.getElementById("total-player-reward");
  els.pendingPlayerReward = document.getElementById("pending-player-reward");
  els.pendingBlocksCount = document.getElementById("pending-blocks-count");
  els.settlementStatusBadge = document.getElementById("settlement-status-badge");
  els.verificationNoticeBanner = document.getElementById("verification-notice-banner");
  els.hourlyRewardRate = document.getElementById("hourly-reward-rate");
  els.playerAddressPreview = document.getElementById("player-address-preview");
  els.btnCopyAddress = document.getElementById("btn-copy-address");
  els.btnChangeAddress = document.getElementById("btn-change-address");

  // 4 Core Metrics
  els.statHeartbeatsCount = document.getElementById("stat-heartbeats-count");
  els.statSessionsCount = document.getElementById("stat-sessions-count");
  els.statPeersCount = document.getElementById("stat-peers-count");
  els.statBlockHeight = document.getElementById("stat-block-height");
  els.statChainStatusText = document.getElementById("stat-chain-status-text");

  // Tabs
  els.tabButtons = document.querySelectorAll(".tab-btn");
  els.tabPanes = document.querySelectorAll(".tab-pane");

  // Tab 1: Gaming Detail
  els.gameProcessList = document.getElementById("game-process-list");
  els.inputNewGame = document.getElementById("input-new-game");
  els.btnAddGame = document.getElementById("btn-add-game");

  // Tab 2: Tokenomics
  els.dtEmissionYear = document.getElementById("dt-emission-year");
  els.dtTailEmission = document.getElementById("dt-tail-emission");
  els.dtTargetWeight = document.getElementById("dt-target-weight");

  // Tab 3: Network & Storage
  els.netP2pMode = document.getElementById("net-p2p-mode");
  els.netLocalNodeId = document.getElementById("net-local-node-id");
  els.netConnectedCount = document.getElementById("net-connected-count");
  els.peersListBox = document.getElementById("peers-list-box");
  els.storageDataDir = document.getElementById("storage-data-dir");
  els.storageTotalSize = document.getElementById("storage-total-size");
  els.storageBatchCount = document.getElementById("storage-batch-count");
  els.storagePayloadCount = document.getElementById("storage-payload-count");
  els.storageDbSize = document.getElementById("storage-db-size");

  // Tab 4: Node Controls & Logs
  els.btnSvcStart = document.getElementById("btn-svc-start");
  els.btnSvcStop = document.getElementById("btn-svc-stop");
  els.btnSvcCheck = document.getElementById("btn-svc-check");
  els.infoAppVersion = document.getElementById("info-app-version");
  els.infoRuntimeMode = document.getElementById("info-runtime-mode");
  els.selUpdateChannel = document.getElementById("sel-update-channel");
  els.infoRemoteVersion = document.getElementById("info-remote-version");
  els.infoUpdateStatus = document.getElementById("info-update-status");
  els.updateChannelBadge = document.getElementById("update-channel-badge");
  els.updateDetailsBox = document.getElementById("update-details-box");
  els.chkAutoSync = document.getElementById("chk-auto-sync");
  els.btnCheckUpdate = document.getElementById("btn-check-update");
  els.btnGitSync = document.getElementById("btn-git-sync");
  els.btnClearConsole = document.getElementById("btn-clear-console");
  els.consoleStream = document.getElementById("console-stream");
}

// HTTP Helper
async function apiGet(path) {
  try {
    const res = await fetch(path, {
      headers: { Accept: "application/json" },
      cache: "no-store",
    });
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    return await res.json();
  } catch (err) {
    console.warn(`[PoLE API GET ${path}] failed:`, err);
    return null;
  }
}

async function apiPost(path, data = {}) {
  try {
    const res = await fetch(path, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Accept: "application/json",
      },
      body: JSON.stringify(data),
    });
    if (!res.ok) {
      const errText = await res.text();
      throw new Error(errText || `HTTP ${res.status}`);
    }
    return await res.json();
  } catch (err) {
    console.error(`[PoLE API POST ${path}] failed:`, err);
    throw err;
  }
}

// Helpers
function formatNumber(num) {
  if (num === null || num === undefined) return "0";
  return Number(num).toLocaleString("en-US");
}

function truncateStr(str, front = 8, back = 6) {
  if (!str) return "-";
  if (str.length <= front + back + 3) return str;
  return `${str.slice(0, front)}...${str.slice(-back)}`;
}

function formatBytes(bytes) {
  if (!bytes || isNaN(bytes)) return "-";
  const num = Number(bytes);
  if (num >= 1024 * 1024 * 1024) return (num / (1024 * 1024 * 1024)).toFixed(2) + " GB";
  if (num >= 1024 * 1024) return (num / (1024 * 1024)).toFixed(2) + " MB";
  if (num >= 1024) return (num / 1024).toFixed(2) + " KB";
  return num + " B";
}

function showToast(message) {
  if (!els.toast) return;
  els.toast.textContent = message;
  els.toast.classList.add("show");
  clearTimeout(els.toastTimeout);
  els.toastTimeout = setTimeout(() => {
    els.toast.classList.remove("show");
  }, 2200);
}

function logToConsole(message) {
  if (!els.consoleStream) return;
  const now = new Date().toLocaleTimeString();
  const line = `[${now}] ${message}\n`;
  els.consoleStream.textContent += line;
  els.consoleStream.scrollTop = els.consoleStream.scrollHeight;
}

// Renderers
function renderGaming(rawData) {
  if (!rawData) return;
  const data = rawData.gaming || rawData;
  state.gaming = data;

  const {
    active_game_processes = [],
    configured_game_processes = [],
    foreground_process,
    foreground_title,
    engagement_state = "NoGame",
    play_heartbeats_count = 0,
    play_sessions_count = 0,
    player_blocks_count = 0,
    verified_player_blocks_count = 0,
    pending_player_blocks_count = 0,
    witness_attestations_count = 0,
    verified_player_reward = 0,
    pending_player_reward = 0,
    verification_status = "PendingWitness",
    working_set_mb = 0,
  } = data;

  // Engagement Status Badge & Radar
  if (engagement_state === "InWorld") {
    els.gameEngagementBadge.className = "badge-status badge-active";
    els.gameEngagementBadge.textContent = "🟢 有效游玩中 (发奖中)";
    els.gameAvatarIcon.textContent = "🎮";
    els.currentGameTitle.textContent =
      foreground_title || foreground_process || (active_game_processes.length ? active_game_processes.join(", ") : "游戏进行中");
    els.currentGamePid.textContent = foreground_process
      ? `进程: ${foreground_process}`
      : (active_game_processes.length ? `已识别: ${active_game_processes[0]}` : "已识别");
    els.gameEngagementDesc.textContent =
      "PoLE 正在见证真实游玩活跃度。支持挂机睡觉 (AFK) 全额发放见证心跳，收益持续累积中。";
  } else if (engagement_state === "MainMenu") {
    els.gameEngagementBadge.className = "badge-status badge-paused";
    els.gameEngagementBadge.textContent = "⏸️ 主菜单 (卡屏暂停)";
    els.gameAvatarIcon.textContent = "⏸️";
    els.currentGameTitle.textContent =
      foreground_title || foreground_process || "游戏主菜单等待中";
    els.currentGamePid.textContent = foreground_process ? `进程: ${foreground_process}` : "暂停中";
    els.gameEngagementDesc.textContent =
      "检测到处于游戏标题/暂停主菜单，记账自动平滑暂停以防挂机作弊。进入游戏世界后自动恢复记账。";
  } else {
    els.gameEngagementBadge.className = "badge-status badge-waiting";
    els.gameEngagementBadge.textContent = "⚪ 等待游戏启动";
    els.gameAvatarIcon.textContent = "🕹️";
    els.currentGameTitle.textContent = "未检测到前台游戏";
    els.currentGamePid.textContent = "PID: -";
    els.gameEngagementDesc.textContent =
      "PoLE 后台静默侦测中。直接启动任意 Steam 游戏，系统将毫秒级自动识别并开始见证记账。";
  }

  // Working Set Memory
  if (working_set_mb > 0) {
    els.nodeMemoryUsage.textContent = `${working_set_mb.toFixed(1)} MB (极低开销)`;
  }

  // Core Metrics
  els.statHeartbeatsCount.textContent = formatNumber(play_heartbeats_count);
  els.statSessionsCount.textContent = formatNumber(witness_attestations_count);

  // Player Rewards - Verified vs Pending
  if (els.totalPlayerReward) {
    els.totalPlayerReward.textContent = formatNumber(verified_player_reward);
  }
  if (els.pendingPlayerReward) {
    els.pendingPlayerReward.textContent = formatNumber(pending_player_reward);
  }
  if (els.pendingBlocksCount) {
    els.pendingBlocksCount.textContent = formatNumber(pending_player_blocks_count);
  }

  // Settlement & Verification Badge
  if (els.settlementStatusBadge) {
    if (verified_player_blocks_count > 0) {
      els.settlementStatusBadge.className = "badge-verified";
      els.settlementStatusBadge.textContent = "已获其他节点见证验证";
    } else {
      els.settlementStatusBadge.className = "badge-pending";
      els.settlementStatusBadge.textContent = "待其他节点验证";
    }
  }

  // Verification Notice Banner
  if (els.verificationNoticeBanner) {
    if (pending_player_blocks_count > 0 && verified_player_blocks_count === 0) {
      els.verificationNoticeBanner.style.display = "block";
      els.verificationNoticeBanner.className = "verification-notice-banner banner-warning";
      els.verificationNoticeBanner.innerHTML =
        `⚠️ <strong>单节点待见证状态</strong>：节点奖励必须经过其他节点验证后方可最终结算发放。当前已有 <strong>${pending_player_blocks_count}</strong> 个区块处于【待见证】队列（预估待结算: ${formatNumber(pending_player_reward)} POLE），须连接并由其他对等节点验证背书后方可正式发放入账。`;
    } else if (verified_player_blocks_count > 0) {
      els.verificationNoticeBanner.style.display = "block";
      els.verificationNoticeBanner.className = "verification-notice-banner banner-success";
      els.verificationNoticeBanner.innerHTML =
        `✅ <strong>已验证结算</strong>：${verified_player_blocks_count} 个区块已经过对等见证节点背书，奖励已发放。${pending_player_blocks_count > 0 ? ` (另有 ${pending_player_blocks_count} 个新区块待见证)` : ''}`;
    } else {
      els.verificationNoticeBanner.style.display = "none";
    }
  }
}

function renderDashboard(rawData) {
  if (!rawData) return;
  const data = rawData.dashboard || rawData;
  state.dashboard = data;

  const {
    service = {},
    node = {},
    tokenomics = {},
    network = {},
    storage = {},
    config = {},
    meta = {},
    update_available = false,
  } = data;

  // Header Service State
  const isRunning =
    service.state === "running" ||
    service.state === "starting" ||
    service.running ||
    service.managed_status === "running" ||
    Boolean(service.pid);

  const pulseDot = els.chipDaemon.querySelector(".pulse-dot");
  if (isRunning) {
    els.headerServiceLabel.textContent = `节点运行中${service.pid ? ` (PID: ${service.pid})` : ""}`;
    pulseDot.className = "pulse-dot active";
  } else {
    els.headerServiceLabel.textContent = "节点已停止";
    pulseDot.className = "pulse-dot stopped";
  }

  // Reward Address
  const isPlaceholderAddr = (a) => !a || /^0+$/.test(a) || /^1+$/.test(a) || /^2+$/.test(a) || /^3+$/.test(a) || /^4+$/.test(a);
  let addr = config.reward_address || node.reward_address;
  if (isPlaceholderAddr(addr) && node.node_id && !isPlaceholderAddr(node.node_id)) {
    addr = node.node_id;
  }
  if (addr) {
    state.rewardAddress = addr;
    els.playerAddressPreview.textContent = truncateStr(addr, 10, 8);
    els.playerAddressPreview.title = addr;
  }

  // Tokenomics
  if (tokenomics.player_block_reward) {
    els.hourlyRewardRate.textContent = `${tokenomics.player_block_reward} / Block`;
  }
  if (tokenomics.player_reward && !state.gaming) {
    els.totalPlayerReward.textContent = formatNumber(tokenomics.player_reward);
  }
  if (els.dtEmissionYear && (tokenomics.emission_year || config.emission_year)) {
    els.dtEmissionYear.textContent = `Year ${tokenomics.emission_year || config.emission_year}`;
  }
  if (els.dtTailEmission) {
    els.dtTailEmission.textContent = "年化 2% 托底";
  }
  if (els.dtTargetWeight && tokenomics.player_reward_budget_per_hour) {
    els.dtTargetWeight.textContent = tokenomics.player_reward_budget_per_hour;
  }

  // Network
  els.statPeersCount.textContent = formatNumber(network.connected_peers || 0);
  if (els.netP2pMode) els.netP2pMode.textContent = network.mode || "Socket Gossip 总线";
  if (els.netLocalNodeId) els.netLocalNodeId.textContent = network.local_peer_id || config.node_id || node.node_id || "-";
  if (els.netConnectedCount) els.netConnectedCount.textContent = formatNumber(network.connected_peers || 0);

  if (els.peersListBox) {
    if (network.peers && Array.isArray(network.peers) && network.peers.length > 0) {
      els.peersListBox.innerHTML = network.peers
        .map(
          (p) =>
            `<div class="badge-item" style="margin-bottom:6px">🌐 ${p.addr || p.id || p}</div>`
        )
        .join("");
    } else {
      els.peersListBox.innerHTML =
        '<p class="muted">当前处于局域网/自组网监听状态，等待邻居节点握手广播</p>';
    }
  }

  // Storage
  if (storage.data_dir && els.storageDataDir) els.storageDataDir.textContent = storage.data_dir;
  if (els.storageTotalSize) {
    els.storageTotalSize.textContent = storage.total_size_formatted || formatBytes(storage.total_size_bytes);
  }
  if (storage.batch_count !== undefined && els.storageBatchCount)
    els.storageBatchCount.textContent = formatNumber(storage.batch_count);
  if (storage.payload_count !== undefined && els.storagePayloadCount)
    els.storagePayloadCount.textContent = formatNumber(storage.payload_count);
  if (els.storageDbSize) {
    els.storageDbSize.textContent = storage.db_size_bytes > 0 ? formatBytes(storage.db_size_bytes) : "< 1 MB (轻公链)";
  }

  // Game Processes List in Tab 1
  const games = Array.isArray(config.game_process_names)
    ? config.game_process_names
    : (config.game_process_names ? [config.game_process_names] : ["Genesis.exe"]);

  if (els.gameProcessList) {
    els.gameProcessList.innerHTML = games
      .map((name) => `<li class="badge-item">${name}</li>`)
      .join("");
  }

  // Versions
  if (meta.app_version && els.appVersionBadge) {
    els.appVersionBadge.textContent = `v${meta.app_version}`;
  }
  if (meta.app_version && els.infoAppVersion) {
    els.infoAppVersion.textContent = `v${meta.app_version} (Windows x64)`;
  }
  if (els.infoUpdateStatus) {
    els.infoUpdateStatus.textContent = update_available ? "发现新版本，可随时更新" : "已是最新版本";
  }
}

function renderUpdateStatus(u) {
  if (!u) return;
  state.update = u;
  if (els.infoAppVersion) {
    els.infoAppVersion.textContent = `${u.current_version} (Windows x64)`;
  }
  if (els.infoRuntimeMode) {
    els.infoRuntimeMode.textContent = u.runtime_mode || (u.is_git_repo ? "Git 源码工作区" : "绿色便携免安装版");
  }
  if (els.selUpdateChannel && u.active_channel) {
    if (els.selUpdateChannel.value !== u.active_channel) {
      els.selUpdateChannel.value = u.active_channel;
    }
  }
  if (els.infoRemoteVersion) {
    els.infoRemoteVersion.textContent = u.remote_version || u.current_version;
  }
  if (els.infoUpdateStatus) {
    els.infoUpdateStatus.textContent = u.message || (u.update_available ? "发现新版本" : "已是最新版本");
  }
  if (els.updateChannelBadge) {
    const channelNames = { stable: "稳定通道", beta: "测试通道", dev: "开发通道" };
    const cName = channelNames[u.active_channel] || u.active_channel;
    if (u.update_available) {
      els.updateChannelBadge.textContent = `${cName} · 发现新版`;
      els.updateChannelBadge.className = "badge badge-warning";
    } else {
      els.updateChannelBadge.textContent = `${cName} · 最新`;
      els.updateChannelBadge.className = "badge badge-success";
    }
  }
  if (els.updateDetailsBox) {
    if (u.update_available && u.download_url) {
      els.updateDetailsBox.style.display = "block";
      els.updateDetailsBox.innerHTML = `
        <strong>💡 新版本提示：</strong>${u.release_name || u.remote_version}<br>
        <a href="${u.download_url}" target="_blank" rel="noreferrer" style="color:var(--accent-cyan);text-decoration:underline;">📥 点击下载最新便携包 (${u.remote_version})</a>
      `;
    } else {
      els.updateDetailsBox.style.display = "none";
    }
  }
}

function renderBlockchain(rawData) {
  if (!rawData) return;
  const bc = rawData.blockchain || rawData;
  state.blockchain = bc;

  if (bc.block_height !== undefined) {
    els.statBlockHeight.textContent = `#${formatNumber(bc.block_height)}`;
  }
  if (els.statChainStatusText) {
    els.statChainStatusText.textContent = bc.online
      ? `本地链在线 · ${bc.chain_id || "pole-local"}`
      : "本地链就绪 (共识正常)";
  }
}

function renderLogs(rawData) {
  if (!rawData || !els.consoleStream) return;
  const logsArr = rawData.logs || [];
  if (Array.isArray(logsArr) && logsArr.length > 0) {
    const combined = logsArr
      .map((item) => (typeof item === "string" ? item : (item.text || "")))
      .filter(Boolean)
      .join("\n");
    if (combined && combined !== els.consoleStream.textContent) {
      els.consoleStream.textContent = combined;
      els.consoleStream.scrollTop = els.consoleStream.scrollHeight;
    }
  }
}

// Fetch all data
async function refreshAll() {
  if (state.isRefreshing) return;
  state.isRefreshing = true;

  if (els.btnManualRefresh) {
    els.btnManualRefresh.classList.add("spinning");
  }

  try {
    const [gamingData, dashData, bcData, storageData, logsData, updateData] = await Promise.all([
      apiGet("/api/gaming"),
      apiGet("/api/dashboard"),
      apiGet("/api/blockchain"),
      apiGet("/api/storage"),
      apiGet("/api/logs"),
      apiGet("/api/update/status").catch(() => apiGet("/api/git/status")),
    ]);

    if (gamingData) renderGaming(gamingData);
    if (dashData) renderDashboard(dashData);
    if (bcData) renderBlockchain(bcData);
    if (storageData && dashData) {
      renderDashboard({ ...dashData, storage: { ...dashData.storage, ...storageData } });
    }
    if (logsData) renderLogs(logsData);
    if (updateData) renderUpdateStatus(updateData);
  } catch (err) {
    console.error("refreshAll error:", err);
  } finally {
    state.isRefreshing = false;
    if (els.btnManualRefresh) {
      els.btnManualRefresh.classList.remove("spinning");
    }
  }
}

// Setup Auto-Refresh
function setupAutoRefresh() {
  if (state.autoRefreshTimer) {
    clearInterval(state.autoRefreshTimer);
    state.autoRefreshTimer = null;
  }

  if (els.autoRefreshCb && els.autoRefreshCb.checked) {
    state.autoRefreshTimer = setInterval(refreshAll, 3000);
  }
}

// Setup Event Listeners
function setupEvents() {
  // Manual Refresh
  if (els.btnManualRefresh) {
    els.btnManualRefresh.addEventListener("click", () => {
      refreshAll();
      showToast("已刷新最新数据");
    });
  }

  // Auto-refresh Toggle
  if (els.autoRefreshCb) {
    els.autoRefreshCb.addEventListener("change", () => {
      setupAutoRefresh();
      showToast(els.autoRefreshCb.checked ? "已启用每 3 秒自动同步" : "已暂停自动同步");
    });
  }

  // Copy Address
  if (els.btnCopyAddress) {
    els.btnCopyAddress.addEventListener("click", async () => {
      const address = state.rewardAddress || els.playerAddressPreview.textContent;
      if (!address || address === "正在读取账户地址...") {
        showToast("地址暂不可用");
        return;
      }
      try {
        await navigator.clipboard.writeText(address);
        showToast("✅ 已复制收款地址到剪贴板！");
      } catch (err) {
        const textarea = document.createElement("textarea");
        textarea.value = address;
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand("copy");
        document.body.removeChild(textarea);
        showToast("✅ 已复制收款地址！");
      }
    });
  }

  // Change Address
  if (els.btnChangeAddress) {
    els.btnChangeAddress.addEventListener("click", async () => {
      const current = state.rewardAddress || "";
      const input = window.prompt("请输入新的 64 位十六进制收款地址 (32 字节)：", current);
      if (!input) return;
      const clean = input.trim().toLowerCase();
      if (!/^[0-9a-f]{64}$/.test(clean)) {
        alert("地址格式不正确！必须为 64 位十六进制字符 (例如: 8fb4159595f244b7...)");
        return;
      }
      try {
        const resp = await apiPost("/api/config", { reward_address: clean });
        if (resp && resp.config) {
          state.rewardAddress = clean;
          els.playerAddressPreview.textContent = truncateStr(clean, 10, 8);
          els.playerAddressPreview.title = clean;
          showToast("✅ 收款地址已成功更新并生效！");
        } else {
          showToast("❌ 保存失败，请检查控制台");
        }
      } catch (err) {
        showToast("❌ 更新请求失败: " + err.message);
      }
    });
  }

  // Tab Switching
  els.tabButtons.forEach((btn) => {
    btn.addEventListener("click", () => {
      const targetTab = btn.getAttribute("data-tab");
      els.tabButtons.forEach((b) => b.classList.remove("active"));
      els.tabPanes.forEach((p) => p.classList.remove("active"));

      btn.classList.add("active");
      const targetPane = document.getElementById(targetTab);
      if (targetPane) {
        targetPane.classList.add("active");
      }
    });
  });

  // Add Game Process
  if (els.btnAddGame && els.inputNewGame) {
    els.btnAddGame.addEventListener("click", async () => {
      const val = els.inputNewGame.value.trim();
      if (!val) {
        showToast("请输入游戏进程名 (如 cs2.exe)");
        return;
      }

      const currentList = Array.isArray(state.dashboard?.config?.game_process_names)
        ? state.dashboard.config.game_process_names
        : ["Genesis.exe"];

      if (currentList.includes(val)) {
        showToast("该游戏进程已在列表中");
        return;
      }

      const updated = [...currentList, val];
      try {
        await apiPost("/api/config", { game_process_names: updated });
        els.inputNewGame.value = "";
        showToast(`已成功添加游戏: ${val}`);
        logToConsole(`已添加游戏进程: ${val}`);
        refreshAll();
      } catch (err) {
        showToast(`添加失败: ${err.message}`);
      }
    });
  }

  // Service Management Actions
  if (els.btnSvcStart) {
    els.btnSvcStart.addEventListener("click", async () => {
      try {
        logToConsole("正在发送服务启动请求...");
        showToast("正在启动后台节点...");
        await apiPost("/api/service/start");
        logToConsole("启动命令已发送，正在检查运行状态...");
        setTimeout(refreshAll, 1500);
      } catch (err) {
        showToast(`启动失败: ${err.message}`);
        logToConsole(`启动错误: ${err.message}`);
      }
    });
  }

  if (els.btnSvcStop) {
    els.btnSvcStop.addEventListener("click", async () => {
      try {
        logToConsole("正在发送服务停止请求...");
        showToast("正在停止后台服务...");
        await apiPost("/api/service/stop");
        logToConsole("停止命令已执行");
        setTimeout(refreshAll, 1000);
      } catch (err) {
        showToast(`停止失败: ${err.message}`);
        logToConsole(`停止错误: ${err.message}`);
      }
    });
  }

  if (els.btnSvcCheck) {
    els.btnSvcCheck.addEventListener("click", async () => {
      try {
        logToConsole("执行节点自检与健康排查...");
        showToast("正在自检健康状态...");
        const res = await apiPost("/api/service/status");
        logToConsole(`自检结果: ${JSON.stringify(res)}`);
        refreshAll();
      } catch (err) {
        logToConsole(`自检异常: ${err.message}`);
      }
    });
  }

  // Channel dropdown handler
  if (els.selUpdateChannel) {
    els.selUpdateChannel.addEventListener("change", async (e) => {
      const newChannel = e.target.value;
      try {
        showToast(`正在切换至【${newChannel}】通道...`);
        const res = await apiPost("/api/update/channel", { channel: newChannel });
        if (res) {
          renderUpdateStatus(res);
          showToast(`✅ 已切换至【${res.active_channel}】更新通道`);
          logToConsole(`[更新通道] 已切换至 ${res.active_channel} 通道，当前版本 ${res.current_version}，最新可用 ${res.remote_version}`);
        }
      } catch (err) {
        showToast(`❌ 切换通道失败: ${err.message}`);
        logToConsole(`[切换通道失败] ${err.message}`);
      }
    });
  }

  // Check update button handler
  if (els.btnCheckUpdate) {
    els.btnCheckUpdate.addEventListener("click", async () => {
      try {
        els.btnCheckUpdate.disabled = true;
        els.btnCheckUpdate.textContent = "🔍 正在检查...";
        showToast("正在联网检查最新版本...");
        logToConsole("[检查更新] 正在与 GitHub 检查最新发布版本与代码提交...");
        const res = await apiPost("/api/update/check");
        if (res) {
          renderUpdateStatus(res);
          showToast(res.update_available ? `🎉 ${res.message}` : `✅ ${res.message}`);
          logToConsole(`[检查更新结果] ${res.message}`);
        }
      } catch (err) {
        showToast(`❌ 检查更新异常: ${err.message}`);
        logToConsole(`[检查更新异常] ${err.message}`);
      } finally {
        els.btnCheckUpdate.disabled = false;
        els.btnCheckUpdate.textContent = "🔍 检查更新";
      }
    });
  }

  // Apply update / Git Sync button handler
  if (els.btnGitSync) {
    els.btnGitSync.addEventListener("click", async () => {
      try {
        els.btnGitSync.disabled = true;
        els.btnGitSync.textContent = "🔄 正在更新...";
        showToast("正在执行更新操作...");
        logToConsole("[执行更新] 正在拉取更新...");

        const res = await apiPost("/api/update/sync").catch(() => apiPost("/api/git/sync"));
        if (res && res.ok) {
          showToast(res.message || "✅ 更新处理完成！");
          logToConsole(`[更新完成] ${res.message}`);
        } else {
          const errMsg = res ? res.message : "更新请求失败";
          showToast(`❌ 更新未完成: ${errMsg}`);
          logToConsole(`[更新失败] ${errMsg}`);
        }
        await refreshAll();
      } catch (err) {
        showToast(`❌ 更新异常: ${err.message}`);
        logToConsole(`[更新异常] ${err.message}`);
      } finally {
        els.btnGitSync.disabled = false;
        els.btnGitSync.textContent = "🔄 立即更新";
      }
    });
  }

  // Clear Console
  if (els.btnClearConsole && els.consoleStream) {
    els.btnClearConsole.addEventListener("click", () => {
      els.consoleStream.textContent = "";
      showToast("控制台已清空");
    });
  }
}

// Setup Auto-Refresh
function setupAutoRefresh() {
  if (state.autoRefreshTimer) {
    clearInterval(state.autoRefreshTimer);
    state.autoRefreshTimer = null;
  }
  state.autoRefreshTimer = setInterval(refreshAll, 5000);

  // Background Git/Update auto-check every 15 minutes
  if (state.autoSyncTimer) clearInterval(state.autoSyncTimer);
  state.autoSyncTimer = setInterval(async () => {
    if (els.chkAutoSync && els.chkAutoSync.checked) {
      const update = await apiPost("/api/update/check");
      if (update && update.update_available) {
        logToConsole(`[后台自动更新检查] 发现新版本: ${update.remote_version} (${update.message})`);
        renderUpdateStatus(update);
      }
    }
  }, 900000);
}

// Bootstrap
document.addEventListener("DOMContentLoaded", () => {
  initElements();
  setupEvents();
  refreshAll();
  setupAutoRefresh();
  logToConsole("PoLE 玩家控制台已就绪，正在实时监听并连接后台服务...");
});
