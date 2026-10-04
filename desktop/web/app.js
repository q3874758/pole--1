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

  // Hero Bento - Right (Reward & Wallet)
  els.totalPlayerReward = document.getElementById("total-player-reward");
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
  els.infoUpdateStatus = document.getElementById("info-update-status");
  els.gitSyncBadge = document.getElementById("git-sync-badge");
  els.gitCommitInfo = document.getElementById("git-commit-info");
  els.chkAutoSync = document.getElementById("chk-auto-sync");
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
  els.statSessionsCount.textContent = formatNumber(play_sessions_count);

  // Player Rewards
  if (player_blocks_count > 0) {
    const calculatedReward = player_blocks_count * 850; // 85% of 1000 POLE per block
    els.totalPlayerReward.textContent = formatNumber(calculatedReward);
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
  if (tokenomics.player_reward && !state.gaming?.player_blocks_count) {
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

function renderGitStatus(git) {
  if (!git) return;
  state.git = git;
  if (els.gitCommitInfo) {
    els.gitCommitInfo.textContent = `${git.branch} (${git.current_commit})`;
  }
  if (els.infoUpdateStatus) {
    els.infoUpdateStatus.textContent = git.message || (git.synced ? "已是最新代码" : "发现远程新提交");
  }
  if (els.gitSyncBadge) {
    if (git.synced) {
      els.gitSyncBadge.textContent = git.is_git_repo ? "Git 已同步" : "最新版";
      els.gitSyncBadge.className = "badge badge-success";
    } else {
      els.gitSyncBadge.textContent = "发现新提交";
      els.gitSyncBadge.className = "badge badge-warning";
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
    const [gamingData, dashData, bcData, storageData, logsData, gitData] = await Promise.all([
      apiGet("/api/gaming"),
      apiGet("/api/dashboard"),
      apiGet("/api/blockchain"),
      apiGet("/api/storage"),
      apiGet("/api/logs"),
      apiGet("/api/git/status"),
    ]);

    if (gamingData) renderGaming(gamingData);
    if (dashData) renderDashboard(dashData);
    if (bcData) renderBlockchain(bcData);
    if (storageData && dashData) {
      renderDashboard({ ...dashData, storage: { ...dashData.storage, ...storageData } });
    }
    if (logsData) renderLogs(logsData);
    if (gitData) renderGitStatus(gitData);
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

  // Git Sync button handler
  if (els.btnGitSync) {
    els.btnGitSync.addEventListener("click", async () => {
      try {
        els.btnGitSync.disabled = true;
        els.btnGitSync.textContent = "🔄 正在同步 GitHub 仓库...";
        showToast("正在与 GitHub 仓库同步...");
        logToConsole("正在与 GitHub 远程仓库同步代码...");

        const res = await apiPost("/api/git/sync");
        if (res && res.ok) {
          showToast(res.message || "✅ GitHub 仓库同步完成！");
          logToConsole(`[Git 同步] ${res.message}`);
        } else {
          const errMsg = res ? res.message : "同步请求失败";
          showToast(`❌ 同步未完成: ${errMsg}`);
          logToConsole(`[Git 同步失败] ${errMsg}`);
        }
        await refreshAll();
      } catch (err) {
        showToast(`❌ 同步异常: ${err.message}`);
        logToConsole(`[Git 同步异常] ${err.message}`);
      } finally {
        els.btnGitSync.disabled = false;
        els.btnGitSync.textContent = "🔄 立即与 GitHub 仓库同步";
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

  // Background Git auto-sync check every 5 minutes
  if (state.autoSyncTimer) clearInterval(state.autoSyncTimer);
  state.autoSyncTimer = setInterval(async () => {
    if (els.chkAutoSync && els.chkAutoSync.checked) {
      const git = await apiGet("/api/git/status");
      if (git && !git.synced && git.update_available) {
        logToConsole(`[GitHub 自动同步] 检测到远程新提交: ${git.remote_commit}，正在拉取同步...`);
        const res = await apiPost("/api/git/sync");
        if (res && res.ok) {
          showToast("🎉 已自动与 GitHub 仓库同步最新代码！");
          logToConsole(`[GitHub 自动同步] ${res.message}`);
          refreshAll();
        }
      }
    }
  }, 300000);
}

// Bootstrap
document.addEventListener("DOMContentLoaded", () => {
  initElements();
  setupEvents();
  refreshAll();
  setupAutoRefresh();
  logToConsole("PoLE 玩家控制台已就绪，正在实时监听并连接后台服务...");
});
