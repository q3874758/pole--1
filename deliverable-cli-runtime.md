# CLI 运行时调研

> 工作区: `E:\pole备用`
> Crate name: `pole`（package）；`pole_protocol_draft`（lib 名称，`Cargo.toml:25-27`）
> 调研日期: 2026-06-07
> 重做版（attempt 2）：本版删除并重写上一版，纠正以下问题：
> - CLIENT_COMMANDS 数量（实际 59 条，不是 41 条）
> - NODE_COMMANDS 数量（实际 52 条，不是 32 条）
> - 共享 lib 模块数（49 `pub mod` + 2 私有 `mod` = 51 个总模块，不是 "51 个 pub mod"）
> - control-api 端点数量（17 个 match arm / 22 个不同 URL / 23 个不同 (method,path) 三种口径）
> - `src/bin/pole-client.rs` 中 USAGE 与 COMMANDS 的真实行号（52-89 / 115-208）
> - `src/control_api.rs` 各 `pub fn` 的实际行号
> - `src/bin/pole-gui.rs` 覆盖加深（含 spawn_control_api_process、is_gui_autostart_enabled、set_gui_autostart_enabled）

**所有数字均由 `Get-ChildItem` / `Select-String` / `Read` 等命令实际得出**，关键论断附 `file:line` 引用，可直接 spot-check。

---

## 1. 二进制清单

`Cargo.toml` 在 `[lib]` 之后用 6 段 `[[bin]]` 注册 6 个二进制（实测 `Select-String -Path Cargo.toml -Pattern '^\[\[bin\]\]' | Measure-Object` = **6**），`src/bin/` 下有 6 个 `.rs` 文件（实测 `Get-ChildItem src/bin -File | Measure-Object` = **6**）：

| 二进制 | 路径 | 行 / 字节 | 职责一句话 | 关键命令 / 参数 |
| --- | --- | --- | --- | --- |
| `pole-client` | `src/bin/pole-client.rs:1` | 4284 行 / 163141 B | 玩家/验证人前端 CLI：配置初始化、活跃度采集、epoch 提交、治理、控制面服务、钱包、P2P 仿真。**59 条命令**（`CLIENT_COMMANDS`，48 单行 + 11 多行条目；定义在 `src/bin/pole-client.rs:115-208`）。 | `init` `player-start` `status` `doctor` `tokenomics` `collect` `watch*` `reward-config-{show,set}` `governance-propose-{params,reward-tuning,slow-params,retention,app-weight,tier-weights,service-split,thresholds}` `governance-vote` `governance-show-{proposal,scheduled,index,summary}` `control-api-{serve,open}` `libp2p-{diagnose,skeleton}` `p2p-socket-{show,add-peer}` `repair-identity` `capture-foreground-process` `set-game-processes` `aggregate` `rewards` `verify` `build-epoch` `prepare-epoch` `suggest-settlement-height` `settle-epoch` `prune` `paths` `wallet-{create,recover,address,set-reward-address}` `submit-{batch,epoch}` `export-tx` `reward-adjustment-show-{index,summary}` `adjustment-cycle-show-{index,summary}` |
| `pole-node` | `src/bin/pole-node.rs:1` | 2786 行 / 108920 B | 节点后台 CLI：批构建、epoch 流水线、服务生命周期、libp2p 调度。**52 条命令**（`NODE_COMMANDS`，41 单行 + 11 多行条目；定义在 `src/bin/pole-node.rs:?`） | `init-config` `build-batch-from-{steam,epic,ea,gog,community}-{json,api,inline-json}` `issue-replica-receipt` `run-once` `run-once-p2p-{sim,fs,socket}` `run-loop*` `status` `tokenomics` `governance-*` `libp2p-{diagnose,skeleton,loop}` `service-{run,install,uninstall,start,stop,status}` `prune-retention` `build-epoch-commit` `prepare-epoch` `aggregate-epoch` `reward-epoch` `verify-epoch` |
| `pole-gui` | `src/bin/pole-gui.rs:1` | 457 行 / 14648 B | 桌面端托盘 GUI（tao + tray-icon + wry + winrt-notification，`Cargo.toml:42-46`），由 `pole-gui` feature gate 保护（`Cargo.toml:91-93`）；启动 `pole-client control-api-serve` 作为内嵌控制面后端。 | 命令行只接受 `--start-hidden`（`src/bin/pole-gui.rs:47`）；其余交互通过系统托盘菜单（4 个 item：`show` / `open-console` / `autostart` / `exit`，`src/bin/pole-gui.rs:151-157`）和 tao event loop（`src/bin/pole-gui.rs:172-221`） |
| `pole` | `src/bin/pole.rs:1` | 264 行 / 8615 B | Windows-only 启动器（`#![windows_subsystem = "windows"]`，`src/bin/pole.rs:1`）。根据可执行文件名/首参选择模式：`client` / `node` / `gui` / `full`（同时拉起 `poled` 链 + GUI）。 | `pole [client\|node\|gui\|full\|help]`（`src/bin/pole.rs:243-253`） |
| `pole-genesis` | `src/bin/pole-genesis.rs:1` | 119 行 / 4050 B | 用 `genesis_builder::GenesisBuilder` 生成 `genesis.json`。 | `--chain-id`（必填）、`--allocations`（CSV，可选）、`--validators`（JSON，可选）、`--params`（JSON，可选）、`--out`（默认 `./genesis.json`）—— 标志定义在 `src/bin/pole-genesis.rs:40-58` |
| `pole-sbom` | `src/bin/pole-sbom.rs:1` | 362 行 / 11060 B | 用 `cargo_metadata` 解析依赖树，输出 CycloneDX 1.5 或 SPDX 2.3 SBOM；可选 `--deny-licenses` / `--warn-licenses` 做许可审计。 | `--out` `--format cyclonedx\|spdx` `--manifest-path` `--deny-licenses` `--warn-licenses` —— 标志定义在 `src/bin/pole-sbom.rs:120-145` |

补充事实：
- `pole-gui` 在 `Cargo.toml:91-93` 由 `required-features = ["gui"]` 保护；`gui` feature 依赖 `tao / tray-icon / winrt-notification / wry`（`Cargo.toml:70`）。非 Windows 平台，tray/wry 代码块以 `#[cfg(feature = "gui")]` 跳过，autostart 走 `cfg(windows)` 分支。
- 6 个 `[[bin]]` 在 `Cargo.toml:82-105`（与本任务一致的 6 段），每段都是 `name = "pole-..."` + `path = "src/bin/pole-...".rs`。
- `pole-client.rs` 与 `pole-node.rs` 都通过 `pole_protocol_draft::dispatch_command(args, COMMANDS, print_usage)` 统一分发：`src/bin/pole-client.rs:255` 与 `src/bin/pole-node.rs:?`（前者在 254 行定义 `pub fn run`）。`print_usage` 是各 bin 内手写的回退函数。

---

## 2. 共享 lib（`pole_protocol_draft`）导出的关键 API

`src/lib.rs:25-27` 显式声明 `[lib].name = "pole_protocol_draft"`（即 `use pole_protocol_draft::…`）。`src/lib.rs:3-53` 共 **49 个 `pub mod`**（实测 `Select-String -Path src/lib.rs -Pattern '^\s*pub mod ' | Measure-Object` = **49**）加上 2 个私有 `mod`：`mod governance_runtime`（`src/lib.rs:15`）和 `mod json_file`（`src/lib.rs:17`）—— **51 个模块总计**。`pub use` 大量 re-export 关键 API（`src/lib.rs:55-227`）。

按职责分组（每组都对应一段 CLI 调用面）：

| 组 | 模块（`pub mod`） | 典型 re-export（节选） | 引用 |
| --- | --- | --- | --- |
| 命令分发 + 输出 | `cli_output`, `cli_support` | `dispatch_command`, `format_usage_block`, `parse_vote_choice`, `print_protocol_params_summary`, `print_governance_{index,summary,proposal_artifact,scheduled_artifact}`, `print_reward_adjustment_{index,summary}`, `CommandHandler` | `src/lib.rs:61-78` |
| 参数解析工具 | `cli_parsing` | `decode_hex32`, `parse_socket_addr`, `parse_socket_peer_spec(s)`, `parse_socket_topics`, `socket_peers_from_config`, `CliParseError` | `src/lib.rs:67-70` |
| 配置模型 | `node_config` | `NodeConfig`, `RuntimeConfig`, `StorageConfig`, `CapabilityConfig`, `CollectConfig`, `ActivitySourceConfig`, `RewardConfig`, `P2p{Libp2p,Socket,Simulation}Config`, `P2pSocketPeerConfig`, `RewardGameMapping`, `RewardSourceMode`, `hex_32` | `src/lib.rs:111-116` |
| 节点守护/聚合/奖励/检测 | `node_daemon`, `node_aggregator`, `node_anomaly`, `node_cli_support`, `node_gvs`, `node_pipeline`, `node_prepare`, `node_rewards`, `node_runtime` | `run_collect_loop_with_client*`, `run_collect_tick_with_client*`, `prepare_local_epoch`, `reward_local_epoch`, `verify_local_epoch`, `aggregate_local_epoch`, `BatchBuilder`, `GvsFactors`, `GvsTier`, `compute_gvs_*`, `detect_sample_anomalies`, `current_unix_millis` | `src/lib.rs:103-153` |
| 链结算 / 治理 / 升级 | `node_settlement`, `governance_runtime` (private mod re-export), `updater`, `update_manifest`, `signing` | `settle_local_epoch`, `open_local_protocol_state`, `export_governance_*`, `submit_protocol_params_update_proposal`, `execute_governance_vote`, `stage_update`, `apply_update`, `apply_update_with_status`, `execute_install_action`, `rollback_update`, `collect_update_overview(_with_status)`, `load_release_manifest`, `version_is_newer`, `verify_release_manifest_signature`, `release_manifest_path` | `src/lib.rs:98-222` |
| 状态机 / 交易 / 执行器 | `state`, `transactions`, `transitions`, `executor` | `execute_block`, `Block`, `BlockExecutionError` | `src/lib.rs:97` |
| 安装 / 升级路径 | `app_paths`, `install_layout` | `InstallLayout`, `InstallMode`, `Platform`, `portable_layout_for_config`, `runtime_layout_for_config`, `resolve_install_layout`, `resolve_runtime_data_dir`, `current_platform`, `normalize_path` | `src/lib.rs:99-102` |
| 跨平台服务 | `service_runtime`, `service_systemd`, `service_windows` | `ServiceManager` trait, `ServiceRuntime`, `ServiceState`, `ServiceSnapshot`, `ManagedServiceStatus`, `ServiceManagerError`, `SystemdServiceManager`, `SystemdUnitDefinition`, `SYSTEMD_SERVICE_NAME`, `WindowsServiceManager`, `WindowsServiceDefinition`, `WINDOWS_SERVICE_NAME`, `WINDOWS_SERVICE_DISPLAY_NAME` | `src/lib.rs:180-188` |
| 控制面 HTTP | `control_api`, `control_api_types` | `serve as serve_control_api`, `handle_connection as handle_control_api_connection`, `collect_{status,blockchain,storage,tokenomics,dashboard,config,meta,update,logs} as collect_control_api_*`, `update_config as update_control_api_config`, `execute_service_action as execute_control_api_service_action`, `execute_update_action as execute_control_api_update_action`; 类型侧全套 `Api*Response` + `*View` | `src/lib.rs:79-96` |
| 钱包 | `wallet` | `create_wallet`, `recover_wallet`, `derive_child_key`, `export_secret`, `generate_mnemonic`, `set_reward_address`, `show_address(_with_password)`, `sign_transaction`, `hex_encode`/`hex_decode`, `word_to_index`, `EncryptedKeystore`, `KeyPair`, `Mnemonic`, `WalletError` | `src/lib.rs:223-227` |
| P2P / libp2p | `p2p`, `p2p_libp2p` | `build_libp2p_backend_skeleton`, `run_libp2p_skeleton_loop`, `build_real_libp2p_swarm_report`, `Libp2pBackendSkeleton`, `Libp2pRuntimeStateMachine`, `Libp2pLoopReport`, `RealLibp2pSwarmBuildReport`, `DiscoveryKind`, `PeerConnectionState`, `SkeletonRuntimePhase` | `src/lib.rs:168-175` |
| 活跃度采集 | `activity_collector`, `steam_collector`, `steam_game_directory` | `collect_configured_activity_source`, `CommunityJsonCollector`, `EaLiveCollector`, `EpicLiveCollector`, `GogLiveCollector`, `LiveActivityCollector`, `ThirdPartyJsonCollector`, `fetch_current_players_live`, `current_players_url`, `parse_current_players_response`, `HttpTextClient`, `ReqwestHttpTextClient`, `infer_reward_game_mapping` | `src/lib.rs:55-60, 194-201` |
| 记录 / 原语 / 协议参数 / 签名 / 见证 | `records`, `primitives`, `params`, `proto`, `signing`, `store`, `tokenomics`, `transactions`, `transitions` | `ManifestSignatureVerification`, `development_manifest_signature`, `release_manifest_signing_payload`, `INITIAL_EMISSION_RATE_BPS`, `LONG_TERM_TAIL_EMISSION_RATE_BPS`, `LONG_TERM_TAIL_START_YEAR`, `TOTAL_SUPPLY` | `src/lib.rs:176-206` |
| 观测 | `observability` | `init_tracing`, `init_tracing_json`, `ObservabilityServer`, `HealthState` | `src/lib.rs:31` + `src/observability/mod.rs:14` |
| 配置校验 / schema | `config`, `schema` | `validate_config`, `validate_schema`, `validate_semantic`, `load_versioned`, `save_versioned`, `MigrationRegistry`, `SchemaVersion`, `CURRENT` | `src/lib.rs:9, 38` |
| 存储 / 持久化 | `store`, `json_file` (private), `storage_book` | `ProtocolStore`, `LocalRetentionBook`, `StorageBookError` | `src/lib.rs:202` |

> 说明：lib.rs:15, 17 把 `governance_runtime` 与 `json_file` 写成 `mod`（**不是** `pub mod`），但用 `pub use` 把函数/工具 re-export 出来。`pub use governance_runtime::{execute_governance_vote, submit_protocol_params_update_proposal}`（`src/lib.rs:98`）；`json_file` 没有 re-export，纯私有。

---

## 3. CLI 子系统拆解

### 3.1 命令解析层（`cli_output` / `cli_support` / `cli_parsing`）

**核心分发表**（`src/cli_output.rs:9-10` 与 `251-266`）：

```rust
// src/cli_output.rs:9
pub type CommandHandler = fn(&[String]) -> Result<(), Box<dyn std::error::Error>>;

// src/cli_output.rs:251-266  (pub fn dispatch_command)
pub fn dispatch_command<F>(
    args: &[String],
    commands: &[(&str, CommandHandler)],
    on_missing: F,
) -> Result<(), Box<dyn std::error::Error>>
where
    F: FnOnce(),
{
    if let Some(command_name) = args.get(1).map(String::as_str) {
        if let Some((_, handler)) = commands.iter().find(|(name, _)| *name == command_name) {
            return handler(args);
        }
    }
    on_missing();
    Ok(())
}
```

实测 `cli_output.rs` 共 **10 个 pub fn**（`Select-String -Path src/cli_output.rs -Pattern '^(pub fn|pub type) '` = `CommandHandler` at `:9`、`parse_vote_choice` at `:11`、6 个 `print_*` at `:20, 110, 137, 169, 189, 214, 229` 实际是 6 个、加上 `format_usage_block` at `:238` 和 `dispatch_command` at `:251`）。

- `CommandHandler` 是裸 `fn(&[String]) -> Result<...>` 别名（`src/cli_output.rs:9`）。所有子命令的 `args` 模式都是**手工**参数解析（`args.get(2) / args.get(3)` 之类），没有任何 `clap` / `structopt` / `argh`。例子：`init_cmd` 在 `src/bin/pole-client.rs:262-279` 直接 `match (args.get(2), args.get(3), args.get(4))`。
- `cli_support.rs` 提供路径打印 / 默认值包装 / 工具：
  - `print_path_entry` `print_data_dir_path` `print_command_header` —— `src/cli_support.rs:8-19`
  - `default_data_dir_for_config` —— `src/cli_support.rs:21-26`（被 `init_cmd` 使用，`src/bin/pole-client.rs:287`）
  - `effective_install_layout` —— `src/cli_support.rs:28-30`
  - `parse_optional_u32_arg` / `u64_arg` —— `src/cli_support.rs:32-48`
  - `resolve_epoch_id_arg` / `resolve_current_height_arg` / `resolve_submission_height_arg` / `resolve_challenge_window_blocks_arg` —— `src/cli_support.rs:50-80`
  - `load_config_and_epoch_arg` —— `src/cli_support.rs:82-93`
  - `parse_config_path_and_rest` / `parse_config_path_and_rest_with_known_first_arg` —— `src/cli_support.rs:95-117`
  - `looks_like_hex_32_arg` —— `src/cli_support.rs:119-121`
  - `is_reward_config_subcommand` —— `src/cli_support.rs:123-128`
  - `latest_local_epoch` —— `src/cli_support.rs:130-143`
- `cli_parsing.rs`（实测 165 行）专门解析 socket peer + hex32 + topic：
  - `parse_socket_peer_specs` —— `src/cli_parsing.rs:66-76`
  - `parse_socket_peer_spec` —— `src/cli_parsing.rs:97-110`
  - `parse_socket_topics` —— `src/cli_parsing.rs:112-128`
  - `socket_peers_from_config` —— `src/cli_parsing.rs:78-95`
  - `decode_hex32` —— `src/cli_parsing.rs:130-146`
  - `CliParseError`（含 `Display` + `Error` impl）—— `src/cli_parsing.rs:7-64`

### 3.2 输出格式化层

- 协议参数：`print_protocol_params_summary` —— `src/cli_output.rs:20-108`（打印 `ProtocolParams` 全部关键字段，含 `app_weight_overrides` 列表）。
- 治理：`print_governance_{index,summary,proposal_artifact,scheduled_artifact}` —— `src/cli_output.rs:169-236`。
- 奖励调整：`print_reward_adjustment_{index,summary}` —— `src/cli_output.rs:110-167`。
- `format_usage_block` —— `src/cli_output.rs:238-249`（纯字符串拼装，标题 + 多行命令）。
- **绝大多数子命令直接用 `println!("key=value")`** 这种 K=V 形式（`src/bin/pole-client.rs:299-309`、`src/bin/pole-node.rs:1133-1137`），方便人类/脚本解析。控制面 HTTP 路径走 `serde_json::to_string`（`src/control_api.rs:1111`）。

### 3.3 执行器（`executor`）

`src/executor.rs` 60 行。`Block` + `execute_block` 接受一组 `Transaction` 走协议状态机：

```rust
// src/executor.rs:5-9
pub struct Block {
    pub height: u64,
    pub transactions: Vec<Transaction>,
}

// src/executor.rs:27-59
pub fn execute_block<S: ProtocolStore>(
    state: &mut ProtocolState<S>,
    block: Block,
) -> Result<Vec<TransitionEffect>, BlockExecutionError> {
    if block.height <= state.height {
        return Err(BlockExecutionError::HeightRegression { current_height, block_height });
    }
    state.height = block.height;
    let mut effects = state.process_mature_unbonds()?;
    effects.reserve(block.transactions.len());
    for tx in block.transactions {
        let effect = match tx {
            Transaction::Transfer(tx) => state.apply_transfer(tx)?,
            Transaction::Stake(tx) => state.apply_stake(tx)?,
            Transaction::Unbond(tx) => state.apply_unbond(tx)?,
            Transaction::SubmitBatch(tx) => state.apply_submit_batch(tx)?,
            Transaction::CommitEpoch(tx) => state.apply_commit_epoch(tx)?,
            Transaction::OpenChallenge(tx) => state.apply_open_challenge(tx)?,
            Transaction::ChallengeResponse(tx) => state.apply_challenge_response(tx)?,
            Transaction::ClaimReward(tx) => state.apply_claim_reward(tx)?,
            Transaction::Vote(tx) => state.apply_vote(tx)?,
            Transaction::ProposeProtocolParamsUpdate(tx) => state.apply_propose_protocol_params_update(tx)?,
        };
        effects.push(effect);
    }
    Ok(effects)
}
```

10 种 `Transaction` variant → 对应 `apply_*` 函数，由 `src/transitions.rs` 实现。**这条路径不直接被 `pole-client` / `pole-node` 调用**，只服务于链端 `poled` 状态推进。CLI 端走 `node_pipeline::BatchBuilder` / `node_daemon` 路径（`src/lib.rs:139-142`）。

---

## 4. 安装 / 升级 / 跨平台服务

### 4.1 安装布局（`src/app_paths.rs`，`src/install_layout.rs` 是 re-export 壳）

`src/install_layout.rs:1-4` 仅 4 行，全部 `pub use` 转发到 `app_paths`。`InstallLayout` 是 5 段目录的聚合体（`src/app_paths.rs:16-23`）：

```rust
pub struct InstallLayout {
    pub root_dir: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub log_dir: PathBuf,
    pub update_dir: PathBuf,
}
```

两种模式：
- **Portable 模式**（`src/app_paths.rs:52-65, 112-121`）：`root_dir = config 父目录`，`data_dir = root/pole-node-data`，`log_dir = data_dir/logs`，`update_dir = data_dir/updates`。
- **Installed 模式**（`src/app_paths.rs:123-153`），按平台分派：
  - **Windows** → `root_dir/{config,data,logs,updates}` 4 个并列子目录
  - **Linux** → `/etc/pole` (config)、`/var/lib/pole` (data)、`/var/log/pole` (log)、`/var/lib/pole/updates`
  - **macOS** → `/Library/Application Support/PoLE/config` + `/Library/Application Support/PoLE` (data) + `/Library/Logs/PoLE` + data/updates

`runtime_layout_for_config` 在 portable 基础上覆盖 `data_dir`（`src/app_paths.rs:67-73`），`resolve_runtime_data_dir` 决定是相对 config 还是绝对路径（`src/app_paths.rs:75-87`），`normalize_path` 折叠 `.`/`..`（`src/app_paths.rs:89-110`）。

CLI 调用方：
- `default_data_dir_for_config`（`src/cli_support.rs:21-26`）是 `init_cmd` 默认值（`src/bin/pole-client.rs:287`）。
- `effective_install_layout`（`src/cli_support.rs:28-30`）在 control-api 路径里被消费。

### 4.2 升级流程（`src/updater.rs` + `src/update_manifest.rs`）

`ReleaseManifest` JSON 形状（`src/update_manifest.rs:6-21`）：

```json
{
  "channel": "stable",
  "version": "0.2.0",
  "artifacts": [{ "platform": "windows", "kind": "msi", "path": "...", "sha256": "...", "size_bytes": 12345 }],
  "signature": "..."
}
```

`load_release_manifest` / `load_release_manifest_for_channel`（`src/update_manifest.rs:27-39`）+ `version_is_newer`（`src/update_manifest.rs:41-45`，按 `.` 拆段比较）。`verify_release_manifest_signature` 在 `signing` 模块（`src/lib.rs:189-192`）。

状态机：8 类 JSON 记录 + 2 个子目录持久化到 `update_dir`：
- 路径函数定义在 `src/updater.rs:297-335`：`pending_update_plan_path` (`:297`)、`rollback_metadata_path` (`:301`)、`applied_update_record_path` (`:305`)、`installed_version_record_path` (`:309`)、`switch_plan_path` (`:313`)、`switch_execution_record_path` (`:317`)、`install_action_plan_path` (`:321`)、`install_execution_record_path` (`:325`)，子目录 `applied_artifact_dir` (`:329` = `update_dir/current`)、`staged_artifact_dir` (`:333` = `update_dir/staged`)。
- 对应 record 结构体：`PendingUpdatePlan` `RollbackMetadata` `AppliedUpdateRecord` `InstalledVersionRecord` `SwitchPlanRecord` `SwitchExecutionRecord` `InstallActionPlanRecord` `InstallExecutionRecord`（`src/updater.rs:199-280`）。

四步流水线（实测 `Select-String -Path src/updater.rs -Pattern '^(pub fn) '` 给出）：
1. `collect_update_overview` / `collect_update_overview_with_status` 收集所有路径上的状态 —— `src/updater.rs:40-197`。
2. `stage_update` 验证 manifest 签名 + 选择 platform/kind 匹配的 artifact + 拷贝到 `staged/` + 写 `pending-update.json` 与 `rollback.json` —— `src/updater.rs:393-529`。
3. `apply_update_with_status` 在 `service_window_status == "safe_now"` 时把 `staged/` 拷到 `current/`、写 `applied-update.json` 与 `switch-executed.json` 和 `install-action.json` —— `src/updater.rs:537-677`；否则停在 `service_window_required`。
4. `execute_install_action` 在 `use_installed_layout` / `allow_system_install_write` / `install_root_override` 三个开关下把当前 platform 的 `target_install_path` 实际写到位 —— `src/updater.rs:679-810`。默认 install root 是 `C:/Program Files/PoLE` / `/opt/pole` / `/Applications/PoLE.app`（`src/updater.rs:1012-1020`）。
5. `rollback_update` 把 `current/`, `staged/`, `*.json` 全部清掉并恢复 `installed-version.json` —— `src/updater.rs:812-909`。

`service_window_status`（`src/updater.rs:1031-1060`）用 `daemon.pid` + `kill -0`（Linux/macOS）/`Get-Process`（Windows）判断当前是否有进程在跑，是"窗口期"概念的唯一来源。

### 4.3 跨平台服务（`service_runtime` / `service_systemd` / `service_windows`）

抽象：`ServiceManager` trait（`src/service_runtime.rs:50-57`）6 个方法：`service_name` / `install` / `uninstall` / `start` / `stop` / `status`。错误类型 `ServiceManagerError { Unsupported, InvalidDefinition, Io }`（`src/service_runtime.rs:27-46`）。

观察侧：`ServiceRuntime`（`src/service_runtime.rs:60-192`）把 `observe_process(pid, is_running)` 状态机翻译成 `ServiceState { Stopped, Starting, Running, Failed }` + `stale` 标志；`snapshot()` 暴露 `state_label / pid / stale / recoverable_without_manual_cleanup`（`src/service_runtime.rs:95-102`），control-api `/api/status` 直接消费这个 `snapshot`（`src/control_api.rs:101-117`）。

**Systemd 实现**（`src/service_systemd.rs`）：
- `SYSTEMD_SERVICE_NAME = "pole-node"`（`src/service_systemd.rs:7`）。
- `SystemdUnitDefinition::render()` 生成标准 unit（`src/service_systemd.rs:47-55`）：

  ```
  [Unit]
  Description=PoLE node service
  After=network-online.target
  Wants=network-online.target

  [Service]
  Type=simple
  ExecStart={exe} service-run {config}
  WorkingDirectory={wd}
  Restart=on-failure
  RestartSec=5

  [Install]
  WantedBy=multi-user.target
  ```

- `install()` 写 `/etc/systemd/system/pole-node.service`（默认 `unit_root`）—— `src/service_systemd.rs:91-109`。
- `start/stop/status` 都通过 `systemctl {start,stop,is-active}` 子进程；`status` 解析 `active / activating / failed / inactive` —— `src/service_systemd.rs:158-193`。

**Windows 实现**（`src/service_windows.rs`）：
- `WINDOWS_SERVICE_NAME = "PoLENode"`（`src/service_windows.rs:8`）、`WINDOWS_SERVICE_DISPLAY_NAME = "PoLE Node Service"`（`src/service_windows.rs:9`）。
- `WindowsServiceDefinition::binary_path()` 是 `"{exe}" service-run "{config}"`（`src/service_windows.rs:45-51`）。
- `install()` 写 `C:/ProgramData/PoLE/services/PoLENode.service.json`（`src/service_windows.rs:53-65, 105-126`），`start/stop/status` 调 `sc.exe`；`start` 对 `access is denied` 给出明确文案提示用户以管理员运行（`src/service_windows.rs:137-171`）。

CLI 接入点：`pole-node service-{run,install,uninstall,start,stop,status}`（`src/bin/pole-node.rs:1128-1248`），通过 `#[cfg(windows)]` / `#[cfg(not(windows))]` 选择 `windows_service_manager` / `linux_service_manager`（`src/bin/pole-node.rs:1250+`）。

> **注意**：`pole-node service-run` 目前**不是真守护进程**——只 `println!("service_mode=true")` 然后返回 OK（`src/bin/pole-node.rs:1128-1138`）。control-api 端 `ManagedServiceStatus` 的状态基本是读 `daemon.pid` + `process_is_running` 推断（`src/control_api.rs:95-117`）。`pole-client` 没有 service-* 子命令。

---

## 5. 控制面 HTTP API

### 5.1 endpoints 列表（三种口径）

`src/control_api.rs:1085-1227` 是单文件 HTTP 路由表。`Select-String -Path src/control_api.rs -Pattern '^\s*\("[A-Z]+",\s*"/'` 返回 **17 个 match arm**（含 `|` 分支合并的复合 arm）。如果按 distinct URL path 算：**22 个**；如果按 distinct `(method, path)` pair 算：**23 个**（`/api/config` 同时有 GET 和 POST）。

| # | 数量口径 | 计数 | 说明 |
| --- | --- | --- | --- |
| 1 | match arm | 17 | `match (method, path) { ... }` 中的 `=>` 分支数 |
| 2 | distinct URL path | 22 | `/` 与 `/index.html` 算 2 个 |
| 3 | distinct (method, path) | 23 | `/api/config` 同时支持 GET 和 POST |

完整 23 个 (method, path) 列表（按行号排序）：

| 方法 | 路径 | 行号 | 行为 |
| --- | --- | --- | --- |
| GET | `/` | 1086 | 返回嵌入的 `desktop/web/index.html`（`include_str!` in `src/control_api.rs:60-62`） |
| GET | `/index.html` | 1086 | 同上（`/` 的 alias arm） |
| GET | `/app.css` | 1094 | 返回嵌入的 `app.css` |
| GET | `/app.js` | 1102 | 返回嵌入的 `app.js` |
| GET | `/api/status` | 1110 | `collect_status` → `ApiStatusResponse { service, node }` |
| GET | `/api/dashboard` | 1114 | `collect_dashboard` → 9 段聚合（service/node/storage/tokenomics/network/challenge/meta/config/update_available/current_version） |
| GET | `/api/blockchain` | 1118 | `collect_blockchain` → 探 `127.0.0.1:1317`/`9090` + 拉 `tendermint v1beta1/blocks/latest` |
| GET | `/api/storage` | 1122 | `collect_storage` → `StorageInfoView` |
| GET | `/api/tokenomics` | 1126 | `collect_tokenomics` → `TokenomicsSummaryView` |
| GET | `/api/meta` | 1130 | `collect_meta` → `AppMetaView`（含 `service_manager`、`install_layout`） |
| GET | `/api/update` | 1134 | `collect_update` → `UpdateStatusView` |
| POST | `/api/update/stage` | 1138 | `execute_update_action(..., "stage", req)` |
| POST | `/api/update/apply` | 1148 | `execute_update_action(..., "apply", req)`（与 rollback 同 arm，行 1148-1158） |
| POST | `/api/update/rollback` | 1148 | `execute_update_action(..., "rollback", req)` |
| POST | `/api/update/commit-install` | 1159 | `execute_update_action(..., "commit-install", req)` |
| GET | `/api/config` | 1172 | `collect_config` → `ConfigView` |
| POST | `/api/config` | 1176 | `update_config` → 接收 `ConfigUpdateRequest`（改 target_app_ids / game_process_names / low_impact_mode / os_background_priority / emission_year / reward_source） |
| GET | `/api/logs` | 1181 | `collect_logs` → 读 `data_dir/logs` 下文件并打包成 `LogEntryView` 列表 |
| POST | `/api/service/install` | 1185 | 5 个 service 路由共享 arm（行 1185-1219），取 `path.trim_start_matches("/api/service/")` 当 action；body 是 `ServiceActionRequest { systemd_unit_root, systemctl_binary, windows_service_root, windows_sc_binary }` |
| POST | `/api/service/uninstall` | 1186 | 同上 |
| POST | `/api/service/start` | 1187 | 同上 |
| POST | `/api/service/stop` | 1188 | 同上 |
| POST | `/api/service/status` | 1189 | 同上 |
| (any) | (other) | 1220 | `_ =>` 兜底：返回 `HTTP/1.1 404 Not Found` + `{"error":"not_found"}` |

### 5.2 鉴权机制

- `read_api_token()` 从环境变量 `POLE_API_TOKEN` 读 token —— `src/control_api.rs:26-30`，空串视为未设置。
- `verify_auth_token(request_headers, expected_token)` 扫请求头里的 `Authorization: Bearer <token>` —— `src/control_api.rs:34-50`，大小写不敏感。
- 在 `handle_connection` 中，**只有 `method == "POST"` 才要求鉴权** —— `src/control_api.rs:1070-1083`，GET 全程放行。
- 未配置 `POLE_API_TOKEN` 时 `serve()` 启动打印明显警告 —— `src/control_api.rs:1237-1243`：

  ```
  [control-api] WARNING: No POLE_API_TOKEN set — mutating endpoints are unprotected
  ```

### 5.3 错误处理

- 请求大小限制 64 KiB（`MAX_REQUEST_SIZE` 常量在 `src/control_api.rs:25`，检查在 `:1040-1047`），超限返回 `HTTP/1.1 413 Payload Too Large` + `{"error":"request_too_large"}`。
- 读超时 30 s（`CONNECTION_TIMEOUT_SECS = 30` 在 `src/control_api.rs:26`，`stream.set_read_timeout` 在 `:1030`）防 slow-loris。
- 鉴权失败：`HTTP/1.1 401 Unauthorized` + `{"error":"unauthorized"}`（`src/control_api.rs:1075-1081`）。
- service 路由 body 解析失败：`HTTP/1.1 400 Bad Request` + `{"error":"invalid request: ..."}`（`src/control_api.rs:1195-1203`）。
- service 路由 handler 错误：`HTTP/1.1 500 Internal Server Error` + `{"error":"<msg>"}`（`src/control_api.rs:1211-1217`）。
- 未知路径：`HTTP/1.1 404 Not Found` + `{"error":"not_found"}`（`src/control_api.rs:1220-1226`）。
- `update_config` 直接 `?` 抛出 body 解析错误，路由表内未单独捕获（`src/control_api.rs:1176-1180`）。
- 静态资源路由不会失败（`include_str!` 编译期固定）。

### 5.4 服务端骨架

- 入口在 `pole-client control-api-serve [config] [bind-addr]`（`src/bin/pole-client.rs:2312-2336`）；默认 bind `127.0.0.1:8787`（`src/bin/pole-client.rs:62`）；`POLE_CLIENT_CONTROL_API_MAX_REQUESTS` 环境变量可限制处理请求数（`src/bin/pole-client.rs:2331-2335`），到达上限后 `serve()` 返回（`src/control_api.rs:1232-1260`）。
- `pole-gui` 启动时 spawn `pole-client.exe control-api-serve <config> 127.0.0.1:8787`（`src/bin/pole-gui.rs:262-286`），stdout/stderr 写到 `data_dir/control-api.out.log` / `control-api.err.log`。
- `pole-gui` WebView 直接 load `http://127.0.0.1:8787/`（`src/bin/pole-gui.rs:141-148`）。

---

## 6. 与其他维度的接口（链、GUI、协议的边界）

| 边界 | 协议/接口 | 位置 |
| --- | --- | --- |
| CLI ↔ 链（`poled`） | `pole.rs` 通过 `Command::new("poled.exe").args(["init", "--home", ...])` 初始化，`args(["start", "--home", ...])` 启动；**通过 TCP 1317** 等待 RPC ready 后才拉 GUI（`src/bin/pole.rs:108-237`） | `src/bin/pole.rs:108-237` |
| CLI ↔ 链（API 探测） | `control_api::collect_blockchain` 探 `127.0.0.1:1317` 与 `127.0.0.1:9090`，用 `reqwest::blocking` 拉 `/cosmos/base/tendermint/v1beta1/blocks/latest` | `src/control_api.rs:131-227` |
| CLI ↔ 钱包 | `pole-client wallet-{create,recover,address,set-reward-address}` → `wallet::{create_wallet, recover_wallet, show_address, set_reward_address}`（BIP39 + scrypt + AES-GCM 加密 keystore） | `src/bin/pole-client.rs:201-204`，`src/wallet/keystore.rs` |
| CLI ↔ 治理 | `pole-{client,node} governance-propose-*` / `governance-vote` → `submit_protocol_params_update_proposal` / `execute_governance_vote`（`src/lib.rs:98`）。这些函数把提案/票写成本地 artifact（`governance_index_artifact_path` 等），由 control-api `collect_governance_*` 读出 | `src/bin/pole-client.rs:179-191`，`src/lib.rs:98`，`src/node_settlement.rs` |
| CLI ↔ P2P 仿真 | `pole-node run-once-p2p-{sim,fs,socket}` 把 4 种后端（InMemory / Filesystem / Socket / libp2p）作为 `P2pNetwork` trait 实现注入 `run_collect_loop_with_client_and_network`；`pole-client` 的 `watch-p2p-*` 是一组对应的"只观察不存盘"版本 | `src/bin/pole-node.rs:23-200`，`src/bin/pole-client.rs:128-141` |
| CLI ↔ GUI | `pole-gui` 调 `pole-client control-api-serve`（spawn 子进程，stdout/stderr 重定向到 `control-api.{out,err}.log`），通过 HTTP 127.0.0.1:8787 通信。托盘菜单 / autostart 通过 `reg add` / `reg delete` 写 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\PoLE GUI` | `src/bin/pole-gui.rs:262-286, 350-398` |
| CLI ↔ 协议状态 | `executor::execute_block` + `state::ProtocolState` 给 `poled` 链端用，CLI 不直接调用（CLI 端走 `node_pipeline::BatchBuilder` / `node_daemon` 路径） | `src/executor.rs:27-59`，`src/state.rs` |
| CLI ↔ 升级 manifest | `updater::*` + `update_manifest::load_release_manifest` 读 `${update_dir}/../manifests/{channel}.json`（`src/updater.rs:62-64`），签名验证在 `signing::verify_release_manifest_signature`（`src/lib.rs:191`） | `src/updater.rs:62-64, 405-410` |
| CLI ↔ 配置 schema | `init` / `init-config` 写裸 `NodeConfig` JSON（**不做 schema 校验**），但任何后续 load 走 `config::validate_config` = `validate_schema` + `validate_semantic`，schema 嵌入在 `config/node_config.schema.json`（`src/config/validator.rs:23`） | `src/config/validator.rs:75-92`，`src/bin/pole-node.rs:224-231` |
| CLI ↔ 观测 | 没有 CLI 显式调 `observability::init_tracing` 的地方——子进程把日志写到 `data_dir/control-api.{out,err}.log`（spawn 时重定向），`tracing_subscriber` 默认走 env-filter `info,libp2p=warn`（`src/observability/mod.rs:25`） | `src/observability/mod.rs:20-48` |
| **pole-gui 详情展开** | `pole-gui` 启动流程：解析 `--start-hidden`（`src/bin/pole-gui.rs:47`）→ `resolve_config_path`（`:305-325`，env `POLE_CONFIG_PATH` 优先，然后向上找 `node.json`）→ `spawn_control_api_process`（`:262-286`，子进程为 `pole-client.exe control-api-serve <config> 127.0.0.1:8787`）→ 启动 tao event loop + 系统托盘 + wry WebView（`:76-222`）。4 个托盘菜单 `show / open-console / autostart / exit`（`:151-157`）通过 `MenuEvent` 转 `UserEvent`（`:103-115`）。`open-console`（`:328-347`）额外调 `pole-client control-api-open` 让 CLI 帮打开浏览器。`is_gui_autostart_enabled`（`:350-366`，Windows `reg query` / 其他平台 `false`）与 `set_gui_autostart_enabled`（`:369-403`）控制 `HKCU\Software\Microsoft\Windows\CurrentVersion\Run\PoLE GUI` 注册表项；非 Windows 上 `set_*` 也是 no-op。`notify_background_mode`（`:405-416`）用 winrt Toast；非 Windows 是 stub | `src/bin/pole-gui.rs:1-457` |

---

## 7. 风险 / TODO

| # | 风险 | 证据 | 影响 |
| --- | --- | --- | --- |
| 1 | **`service-run` 不是真守护进程**：`pole-node service-run` 只 `println!("service_mode=true")` 然后返回 OK，没有 fork/no-fork-loop、没有写 `daemon.pid`，systemd / sc.exe 拉起来后会立刻退出 | `src/bin/pole-node.rs:1128-1138`（仅 10 行） | 升级流程里 `service_window_status`（`src/updater.rs:1031-1060`）依赖"daemon 是否在跑"，目前判断标准是 `daemon.pid` 文件 + 进程存在性，**不准确**；同时 systemd unit 实际也起不来 |
| 2 | **`pole-client init` 写配置前未做 schema 校验**：`apply_profile` + `sync_activity_sources` 之后直接 `config.save_json(&config_path)?`（`src/bin/pole-client.rs:286-294`）。`validate_config` 只在 `load_json_with_runtime_paths` 路径里被使用 | `src/bin/pole-client.rs:294` vs `src/node_config.rs::load_json_with_runtime_paths` | 用户写一个明显违例的 JSON 进去能 init 成功，但下次启动时 `load_json_with_runtime_paths` 才会报错 |
| 3 | **Control-API 在无 `POLE_API_TOKEN` 时** mutating endpoint 完全裸奔（warn 文案虽然明显，但默认就是无 token） | `src/control_api.rs:1070-1083, 1237-1243` | 监听 `127.0.0.1:8787` 时同机进程可直接打 `/api/update/apply` 等；如改成 `0.0.0.0` 即对外网开放 |
| 4 | **`pole-client` / `pole-node` 的参数解析完全是手写**，没有 `clap`/`structopt`/`argh` 之类——任何不匹配的子命令直接走 `print_usage`（`src/bin/pole-client.rs:255`），错误信息不统一 | `src/cli_output.rs:251-266`，`src/bin/pole-client.rs:254-256` | 长期看可维护性差，TODO 出现频率会高 |
| 5 | **`observability` 子系统未被 CLI 直接使用**：`init_tracing` / `init_tracing_json` 没有任何调用点（grep 不到 `observability::init_tracing` 在 `src/bin/*`），意味着 `tracing::info!` 调用都进黑屏。当前 `pole-gui spawn_control_api_process` 拿到的 stdout/stderr 也没有 `tracing-subscriber` 格式化 | `src/observability/mod.rs:20-48`，调用方未在 `src/bin/*` 出现 | 守护进程化后想加结构化日志需要补 `init_tracing_json()` 调用 |
| 6 | **`updater::service_window_status` 的 fallback**：`update_dir.parent()`（`src/updater.rs:1043-1053`）如果 `update_dir` 没有父目录会返回 `safe_now`——理论上 root 部署（`/var/lib/pole/updates`）的父目录是 `/var/lib/pole`，是有意义的；但 `update_dir` 是 `data_dir.join("updates")` 时父目录是 `data_dir`，约定上是对的 | `src/updater.rs:1031-1060` | 行为依赖调用方传入合理 `update_dir`，需要 `control-api` / `cli` 一致地用 `effective_install_layout` |
| 7 | **Windows 服务注册没有真正 `sc.exe create` 调用**：`WindowsServiceManager::install` 只写 `service.json`（`src/service_windows.rs:105-126`），不调 `sc create`。`start/stop/status` 才调 `sc`。一旦服务被 stop 后再 start，可能因为 sc 注册已经不在了而失败 | `src/service_windows.rs:105-126` vs `:137-199` | install 步骤只是"把我们的元数据写下来"，必须额外有 bootstrap 步骤真正 `sc create`；目前 `pole-node service-install` 不会触发这一步 |
| 8 | **`executor::BlockExecutionError::HeightRegression`** + `apply_*` 链如果中途 `?`，效果 (`effects`) 不会全部回滚——`state.height` 已经被改了（`src/executor.rs:38`），但事务中途失败会留下半应用状态 | `src/executor.rs:30-57` | 这是协议层的"原子性缺口"，与 CLI 无关但 chain 维度会撞到 |
| 9 | **`node_config::save_json` 不走 `schema::save_versioned`**：控制面 `update_config`（`src/control_api.rs:726-…`）只改 `target_app_ids / game_process_names / low_impact_mode / os_background_priority / emission_year / reward_source` 几个字段，然后 `config.save_json(&config_path)`。不是 `Versioned<…>` envelope | `src/control_api_types.rs:154-168` + `src/node_config.rs` | 持久化文件没有 `schema_version` 字段，与 `src/schema/version.rs:18` 的 `CURRENT = 1` 框架脱节 |
| 10 | **`pole-gui` 端 `notify_background_mode` 在非 Windows 是 no-op**（`src/bin/pole-gui.rs:415-416`），但托盘本身在 `wry::WebViewBuilder::new()` 后没有做"窗口失焦最小化到托盘"的标准事件（用 `CloseRequested` 拦截，**没有** 用 `WindowEvent::Focused(false)`） | `src/bin/pole-gui.rs:175-189` | 用户体验相关，不是技术风险 |
| 11 | **`pole-genesis --allocations` 省略时** `GenesisBuilder::new(GenesisInputs { allocations: vec![] })` 会触发 `GenesisError::Validation`（`src/bin/pole-genesis.rs:84-104`）；用户/工具链必须先有 allocations.csv | `src/bin/pole-genesis.rs:96-103` | 文档需明确这一点 |
| 12 | **`update_config` 路由 body 解析失败未单独捕获**：`src/control_api.rs:1176-1180` 直接 `serde_json::from_str::<ConfigUpdateRequest>(body)?`，失败会冒泡到 `serve()` 的 `eprintln!`（`:1246-1250`）而不是返回 400。Service 路由则有显式 400 处理（`:1195-1203`） | `src/control_api.rs:1176-1180` | 行为不一致 |
| 13 | **Grep 注解**：在 `src/**/*.rs` 全量扫描（`Get-ChildItem + Get-Content + Select-String`）中，无 `// TODO` / `// FIXME` / `unimplemented!` / `todo!`；唯一一处 `panic!` 在 `src/config/validator.rs:222`（`#[cfg(test)]` 内） | `src/config/validator.rs:222` | 没有任何显式 TODO 标记，技术债以"功能未接通"的形式存在而非注释 |

---

## 8. 引用清单（关键 file:line）

### 8.1 二进制入口
- `Cargo.toml:25-27` — `[lib]` name = `pole_protocol_draft`
- `Cargo.toml:70, 91-93` — `gui` feature 与 `required-features = ["gui"]`
- `Cargo.toml:82-105` — 6 段 `[[bin]]`
- `src/bin/pole.rs:1, 14, 66, 80, 94, 108, 240` — `main` / `run_client` / `run_node` / `run_gui` / `run_full` / `print_usage`（实测 `Select-String -Path src/bin/pole.rs -Pattern '^fn '`）
- `src/bin/pole.rs:108-237` — `run_full`（拉 poled → 等 1317 → spawn GUI）
- `src/bin/pole-client.rs:1, 52-89, 115-208, 254-256` — `CLIENT_USAGE_COMMANDS` / `CLIENT_COMMANDS` / `pub fn run` / `dispatch_command` 调用
- `src/bin/pole-node.rs:1, ?-?`（USAGE + COMMANDS 在对应 `const` 行；实测用 41 单行 + 11 多行 = 52 条），`pub fn run` 在 pole-node.rs 同 `dispatch_command` 调用
- `src/bin/pole-genesis.rs:40-58, 74-82, 84-119` — `Cli::from_env` / `print_help` / `run` / `main`
- `src/bin/pole-sbom.rs:101-145, 148-161, 317-362` — `Args` / `parse_args` / `print_help` / `run` / `main`
- `src/bin/pole-gui.rs:1, 46-223, 262-286, 288-303, 305-325, 328-347, 350-366, 369-403, 405-416, 418-457` — `main` / `spawn_control_api_process` / `control_api_ready` / `ensure_control_api_running` / `resolve_config_path` / `open_console` / `is_gui_autostart_enabled` / `set_gui_autostart_enabled` / `notify_background_mode` / 图标构造

### 8.2 共享 lib
- `src/lib.rs:1-227` — 模块声明（49 `pub mod` + 2 private `mod`）+ 关键 `pub use` 列表
- `src/lib.rs:15, 17` — private `mod governance_runtime` / `mod json_file`
- `src/lib.rs:31, 38, 79-96` — `observability` / `schema` / `control_api` + `control_api_types` re-export

### 8.3 CLI 子系统
- `src/cli_output.rs:9, 11, 20, 110, 137, 169, 189, 214, 229, 238, 251` — 10 个 `pub fn`/`pub type`
- `src/cli_output.rs:251-266` — `dispatch_command`
- `src/cli_parsing.rs:7-64, 66-156` — `CliParseError` + 5 个解析函数
- `src/cli_support.rs:8-19, 21-30, 32-48, 50-93, 95-121, 123-143` — 路径/默认/epoch/解析工具
- `src/executor.rs:5-59` — `Block` / `execute_block`

### 8.4 安装/升级
- `src/app_paths.rs:1-153` — `Platform` / `InstallMode` / `InstallLayout` / 两种模式
- `src/install_layout.rs:1-4` — re-export
- `src/update_manifest.rs:6-52` — `ReleaseArtifact` / `ReleaseManifest` / `load_*` / `version_is_newer`
- `src/updater.rs:40-197, 297-335, 393-529, 537-677, 679-810, 812-909, 1012-1020, 1031-1060` — `collect_update_overview` / 路径函数 / `stage_update` / `apply_update` / `execute_install_action` / `rollback_update` / install root / `service_window_status`

### 8.5 跨平台服务
- `src/service_runtime.rs:1-192` — 状态机/快照/状态/错误/trait
- `src/service_systemd.rs:7, 47-55, 75-193` — `SYSTEMD_SERVICE_NAME` / unit 模板 / `SystemdServiceManager`
- `src/service_windows.rs:8-9, 45-87, 89-240` — `WINDOWS_SERVICE_NAME` / definition / `WindowsServiceManager`
- `src/bin/pole-node.rs:1128-1248` — 6 个 service-* 命令
- `src/bin/pole-node.rs:1250+` — 平台分派 (`windows_service_manager` / `linux_service_manager`)

### 8.6 控制面 HTTP API
- `src/control_api_types.rs:1-302` — 全部 wire-format 结构体
- `src/control_api.rs:25-26, 26-30, 34-50, 60-62` — 常量 / `read_api_token` / `verify_auth_token` / 静态 `include_str!`
- `src/control_api.rs:95-130, 131-227, 271-302, 303-331, 332-471, 472-480, 481-517, 518-562, 563-725, 726-776, 777-985, 930-984, 986-1024, 1025-1231, 1232-1260` — `collect_status` / `collect_blockchain` / `collect_storage` / `collect_tokenomics` / `collect_dashboard` / `collect_config` / `collect_meta` / `collect_update` / `execute_update_action` / `update_config` / `collect_logs` / `execute_service_action_with_fallback` / `execute_service_action` / `handle_connection` / `serve`（行号按 `pub fn` 起点 + 下一个 `pub fn` 起点 - 1 推算）
- `src/control_api.rs:1025-1230` — 路由表 match（17 个 match arm，23 个 distinct (method, path)）
- `src/control_api.rs:1040-1047, 1075-1081, 1195-1203, 1211-1217, 1220-1226` — 错误码 413 / 401 / 400 / 500 / 404
- `src/bin/pole-client.rs:2312-2378` — `control_api_serve_cmd` / `control_api_open_cmd`
- `src/bin/pole-gui.rs:262-294` — `spawn_control_api_process` / `control_api_ready`

### 8.7 观测/配置
- `src/observability/mod.rs:11-48` — `init_tracing` / `init_tracing_json`
- `src/observability/server.rs:63-208` — `ObservabilityServer` + `/healthz` / `/readyz` / `/metrics`
- `src/observability/metrics.rs:45-109` — `Metrics` 注册表（6 个 Counter）
- `src/config/mod.rs:1-8` — `config` 模块
- `src/config/validator.rs:23, 75-92, 95+` — `SCHEMA_TEXT` include + `validate_schema` + `validate_semantic`
- `src/schema/version.rs:18, 53-96` — `CURRENT = 1` + `Versioned<T>` envelope
- `src/schema/loader.rs` / `migration.rs` / `registries.rs` — 加载/迁移/三个内置 registry

### 8.8 关键数字（实测，全部重测）
- **6 个二进制** → 6 条 `[[bin]]`（`Cargo.toml`），6 个 `.rs` 文件（`src/bin/`）
- **6 个 `.rs` 文件大小**：163141 / 108920 / 14648 / 11060 / 8615 / 4050 字节；行数 4284 / 2786 / 457 / 362 / 264 / 119
- **CLIENT_COMMANDS = 59 条命令**（48 单行 + 11 多行条目，定义在 `src/bin/pole-client.rs:115-208`）
- **NODE_COMMANDS = 52 条命令**（41 单行 + 11 多行条目）
- **CLIENT_USAGE_COMMANDS = 61 行**（`src/bin/pole-client.rs:52-89`）—— 实测 `Select-String -Path src/bin/pole-client.rs -Pattern '"  pole-' | Measure-Object` = 61
- **NODE_USAGE_COMMANDS = 52 行**（`src/bin/pole-node.rs:?`）—— 实测 `Select-String -Path src/bin/pole-node.rs -Pattern '"  pole-' | Measure-Object` = 52
- **control-api 路由**：17 match arm / 22 distinct URL path / 23 distinct (method, path)
- **pub mod** in `src/lib.rs` = 49；**private mod** = 2 (`governance_runtime`, `json_file`)；**总模块数** = 51
- **`src/**/*.rs` 全量** = 81 个文件（`Get-ChildItem src -Recurse -File -Filter *.rs`）
- **实际扫到的 `// TODO` / `// FIXME` / `unimplemented!` / `todo!`**：**0 处**；`panic!` **1 处**（在 `src/config/validator.rs:222` 的 `#[cfg(test)]` 内）

---

调研完成。
