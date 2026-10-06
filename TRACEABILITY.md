# PoLE 追踪矩阵

**项目:** PoLE (Proof of Live Engagement)  
**版本:** V1.0  
**最后更新:** 2026-06-10

本文档将白皮书概念映射到代码库中的实现位置。

## 目的

- 建立白皮书概念与代码模块之间的映射
- 确保实现符合协议规范
- 作为代码审查和回归测试的参考

---

## 第二章: 整体架构

### 2.1 协议流程

| 白皮书步骤 | Rust 实现 | Cosmos 实现 |
|-----------|-----------|-------------|
| 采集 | `activity_collector.rs`, `steam_collector.rs` | 不适用（链下） |
| 批次整理 | `node_pipeline.rs` - `BatchBuilder` | 不适用（链下） |
| 链上承诺 | `node_daemon.rs` - artifact 生成 | `MsgSubmitBatch`, `MsgCommitEpoch` |
| 聚合 | `node_aggregator.rs` - `aggregate_local_epoch` | 不适用（链下） |
| 奖励根生成 | `node_rewards.rs` - `reward_local_epoch` | `keeper.ComputeEpochCommitments` |
| Challenge | `node_verifier.rs` - `verify_local_epoch` | `MsgOpenChallenge`, `MsgResolveChallenge` |
| Finalize | `node_settlement.rs` - `settle_local_epoch` | `MsgFinalizeEpoch` |
| 领取 | `transactions.rs` - `ClaimRewardTx` | `MsgClaimReward` |

### 2.2 数据承诺对象

| 白皮书对象 | Rust 位置 | Cosmos 位置 |
|-----------|-----------|-------------|
| `BatchCommit` | `records.rs` - `BatchCommit` | `types/state.pb.go` - `BatchCommit` |
| `EpochCommit` | `records.rs` - `EpochCommit` | `types/state.pb.go` - `EpochCommit` |
| `AggregateRoot` | `node_pipeline.rs` - `merkle_root` | `types/state.pb.go` - `MerkleCommitment` |
| `RewardRoot` | `node_rewards.rs` - `reward_record_root` | `types/state.pb.go` - `MerkleCommitment` |
| `RetentionClaim` | `records.rs` - `ReplicaReceipt` | `types/state.pb.go` - `ReplicaReceipt` + `AvailabilityRecord` |

### 2.3 共识与最终性

| 概念 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| 挑战窗口 | `params.rs` - `challenge_window_blocks` | `types/state.pb.go` - `Params.challenge_window_blocks` |
| 最终确认 | `node_settlement.rs` - `epoch_finalized` | `types/state.pb.go` - `EpochCommit.finalized` |
| 惩罚执行 | `node_storage_audit.rs` - `run_local_storage_challenge` | `keeper.ApplyValidatorSlash` |

### 2.4 Rust→Cosmos 桥接层

Rust 链下节点通过 `src/cosmos/` 把链下 artifact 构造为 Cosmos SDK 交易并广播到链（已接通全部 17 种 Msg）：

| 组件 | 位置 | 说明 |
|------|------|------|
| 消息编码 | `src/cosmos/pole_msgs.rs` | 17 种 Msg 的 proto3 wire encoder |
| wire 类型 | `src/cosmos/wire_types.rs` | 桥接专用 wire-only 类型 |
| 交易构造 | `src/cosmos/tx_builder.rs` | CosmosTxBuilder（protobuf 序列化） |
| 交易签名 | `src/cosmos/tx_signer.rs` | SIGN_MODE_DIRECT 签名 |
| RPC 广播 | `src/cosmos/rpc_client.rs` | Tendermint `broadcast_tx_sync` |
| 查询客户端 | `src/cosmos/query_client.rs` | account / sequence / height 查询 |
| 地址转换 | `src/cosmos/address.rs` | hex ↔ bech32 |
| EIP-712 | `src/cosmos/eip712.rs` | typed-data 签名 helper |
| CLI 提交命令 | `src/cli_client.rs` | submit-batch / submit-epoch / export-tx（真实 proto3 + 广播） |
| 互证记录构造 | `src/mutual_proof.rs` | PlaySession / PlayHeartbeat / WitnessAttestation 构造与 wire 投影 |
| 互证 CLI | `src/cli_node.rs` | play-session / play-heartbeat / attest-session / settle-session / submit-reward-records |
| 互证链上逻辑 | 不适用（链下构造） | `chain/x/pole/keeper/session.go` |

### 2.5 原生低开销静默后台运行时 (`os_support`)

| 机制 | Rust 位置 | 平台与规范 |
|------|-----------|-----------|
| 进程空闲优先级 | `os_support.rs` - `apply_process_background_priority` | Win32 `IDLE_PRIORITY_CLASS` (0x40)，所有 CPU 算力优先供给 3D 游戏 |
| 硬件能效调度 | `os_support.rs` - `apply_process_background_priority` | Windows EcoQoS (`PROCESS_POWER_THROTTLING_EXECUTION_SPEED`)，强制调度在 E-Core |
| 物理内存动态裁剪 | `os_support.rs` - `trim_process_working_set` | Win32 `SetProcessWorkingSetSize`，间歇主动释放非活动页，常驻物理内存 < 15MB |
| 零损耗前台游戏嗅探 | `os_support.rs` - `detect_foreground_process_name` | `user32.dll` 纳秒直调，探测开销 < 0.01ms，完全无外部子进程微卡顿 |
| 游戏进程快照遍历 | `os_support.rs` - `detect_active_process_names` | `CreateToolhelp32Snapshot` 快照匹配，无 PowerShell 拉起抖动 |

---

## 第三章: 核心机制

### 3.1 小时奖励区块

| 概念 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| 区块定义 | `primitives.rs` - `RewardBlock` | `types/state.proto` - 奖励定义 |
| 奖励计算 | `node_rewards.rs` - `adjusted_player_block_reward` | `types/reward_math.go` |

### 3.2 玩家权重定义

| 概念 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| `Player_Hour_Weight` | `node_rewards.rs` - `effective_player_block_reward` | `types/reward_math.go` |
| `Effective_Play_Time` | `activity_collector.rs` - `ActivityCollector` | 不适用（链下） |
| `Game_Weight` | `node_gvs.rs` - `compute_gvs_microunits` | `types/state.pb.go` - `GameWeightEntry` |
| 游玩声明 | `mutual_proof.rs` - `build_play_session` | `MsgSubmitPlaySession` → `keeper.SettlePlaySession` |
| 心跳 | `mutual_proof.rs` - `build_play_heartbeat` | `MsgSubmitPlayHeartbeat` → `heartbeatCoverageBPS` |
| 见证证明 | `mutual_proof.rs` - `build_witness_attestation` | `MsgAttestSession` → `validateWitnessIndependence` |
| 会话结算 | `mutual_proof.rs` - `session_id_from_parts` | `MsgSettleSession` → `SettlePlaySession` |
| 有效会话权重 | `node_rewards.rs` - `local_witness_attestation_count` | `session.go` - `ValidSessionWeightUnitsForEpoch` |

### 3.3 小时奖励分账

| 概念 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| `Player_Hour_Reward` | `node_rewards.rs` - `reward_local_epoch` 公式 | `keeper.CalcPlayerReward` |
| `Hourly_Reward_Pool` | `tokenomics.rs` - `PLAYER_REWARD_ALLOCATION_BPS` | `x/mint` 模块 |
| `Total_Hour_Weight` | `node_aggregator.rs` - `aggregate_record_root` | `keeper.ComputeEpochCommitments` |
| 见证奖励二次分配 | `node_rewards.rs` - `local_witness_attestation_count` | `types/witness_reward.go` - `AllocateWitnessRewards` |
| 见证信用 | 不适用（链上计） | `session.go` - `WitnessCreditsForEpoch` |

### 3.4 跨周期调节

| 概念 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| 调节公式 | `node_rewards.rs` - `adjusted_player_block_reward` | `types/reward_math.go` - `Adjust` |
| `Target_Network_Weight` | `params.rs` - `target_network_weight_units` | `types/state.pb.go` - `Params.target_network_weight_units` |

---

## 第四章: 游戏价值分数 (GVS)

| 概念 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| GVS 计算 | `node_gvs.rs` - `compute_gvs_factors`, `compute_gvs_microunits` | 不适用 |
| 层级分类 | `node_gvs.rs` - `classify_tier` | 不适用 |
| 覆盖奖励 | `node_gvs.rs` - `compute_coverage_bonus_ppm` | 不适用 |
| 时间衰减 | `node_gvs.rs` - `compute_time_decay_ppm` | 不适用 |
| 游戏权重条目 | 不适用 | `types/state.pb.go` - `GameWeightEntry` |
| 游戏权重更新 | 不适用 | `MsgUpsertGameWeight` |

---

## 第五章: 治理

| 概念 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| 提案提交 | `governance_runtime.rs` | `x/gov` 模块 |
| 投票执行 | `governance_runtime.rs` - `execute_governance_vote` | `x/gov` 模块 |
| 参数更新 | `governance_runtime.rs` - `submit_protocol_params_update_proposal` | `MsgUpdateParams` |
| 治理参数 | `params.rs` - `GovernanceParams` | `types/state.pb.go` - `GovernanceParams` |

---

## 第六章: 安全性

| 概念 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| 女巫攻击抵抗 | `wallet/` - 质押要求 | `x/staking` 模块 |
| 证据取回 | `storage_book.rs` - `LocalRetentionBook` | `MsgSubmitReplicaReceipt` |
| 惩罚机制 | `node_storage_audit.rs` | `MsgResolveChallenge` 带 `slash_fraction_bps` |
| 挑战验证 | `node_verifier.rs` | `keeper.validateChallengeEvidence` |
| 虚假游玩惩罚 | `mutual_proof.rs` - `WitnessAttestation` 独立性 | `keeper.SettlePlaySession` + `applyChallengeRewardEffects` |
| 销毁五通道 | `tokenomics.rs` - 销毁参数 | `chain/x/pole/keeper/burn.go` |

---

## 附录 A: 核心类型

| 类型 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| `EpochId` | `primitives.rs` - `EpochId` | `types/state.pb.go` - `uint64` |
| `Height` | `primitives.rs` - `Height` | `abci/types` - `int64` |
| `NodeId` | `primitives.rs` - `NodeId` | `types/state.pb.go` - `string` (Bech32) |
| `Address` | `primitives.rs` - `Address` | `types/state.pb.go` - `string` (Bech32) |
| `Amount` | `primitives.rs` - `Amount` | `types/state.pb.go` - `uint64` |
| `Capability` | `primitives.rs` - `Capability` | `types/state.pb.go` - `NodeCapabilitySet` |

---

## 附录 B: 核心公式

| 公式 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| `Player_Hour_Weight = Effective_Play_Time × Game_Weight` | `node_rewards.rs:adjusted_player_block_reward` | `types/reward_math.go:CalcPlayerWeight` |
| `Player_Hour_Reward = Hourly_Reward_Pool × Player_Hour_Weight / Total_Hour_Weight` | `node_rewards.rs:reward_local_epoch` | `types/reward_math.go:CalcPlayerReward` |
| `Next_Period_Player_Reward = Adjust(Base, Target, Previous)` | `node_rewards.rs:adjusted_player_block_reward` | `types/reward_math.go:AdjustReward` |
| `Annual_Issuance(year) = Annual_Emission_Amount(year) × Activity_Factor` | `tokenomics.rs` - `annual_emission` | `types/emission.go:AnnualAdjustedEmission` |
| `Witness_Reward = AllocateWitnessRewards(credits, verify_pool)` | `node_rewards.rs` - `local_witness_attestation_count` | `types/witness_reward.go:AllocateWitnessRewards` |
| `Net Supply Change = Gross Emission − Gross Burn` | 不适用（链上账本） | `chain/app/supply_simulation_test.go` + `keeper/burn.go` |

---

## 文件映射总结

### Rust 源文件

| 文件 | 白皮书覆盖 |
|------|-----------|
| `src/lib.rs` | 模块导出和 API 表面 |
| `src/primitives.rs` | 核心类型: EpochId, Height, NodeId, Hash32 |
| `src/records.rs` | 协议对象: BatchCommit, EpochCommit, Challenge |
| `src/activity_collector.rs` | 活动信号采集 |
| `src/steam_collector.rs` | Steam 平台采集 |
| `src/node_pipeline.rs` | 批次组装, 默克尔树 |
| `src/node_aggregator.rs` | Epoch 聚合 |
| `src/node_rewards.rs` | 奖励计算 |
| `src/node_settlement.rs` | Epoch 最终确认 |
| `src/node_verifier.rs` | 挑战验证 |
| `src/node_storage_audit.rs` | 存储挑战 |
| `src/node_daemon.rs` | 节点运行时 |
| `src/p2p.rs` | P2P 网络 |
| `src/governance_runtime.rs` | 治理执行 |
| `src/wallet/` | 密钥管理和签名 |
| `src/tokenomics.rs` | 代币经济参数 |
| `src/params.rs` | 协议参数（含 `MutualProofParams`） |
| `src/mutual_proof.rs` | 互证记录：PlaySession / PlayHeartbeat / WitnessAttestation 构造、wire 投影与持久化 |
| `src/proof/` | 证明层重构: L1 物理身份/Authenticode、L2 GPU 3D 渲染、L3 硬件信任根及多层综合证明 (Issue #1, #2) |
| `src/cosmos/` | 链交互：proto3 编码 / 签名 / 广播 / 查询 / 地址 |

### Cosmos 链文件

| 文件 | 白皮书覆盖 |
|------|-----------|
| `chain/x/pole/types/state.proto` | 链上状态类型 |
| `chain/x/pole/types/tx.proto` | 消息类型 |
| `chain/x/pole/types/query.proto` | 查询类型 |
| `chain/x/pole/keeper/keeper.go` | 状态持久化和业务逻辑 |
| `chain/x/pole/keeper/session.go` | 互证：见证独立性、心跳覆盖、会话结算、见证信用 |
| `chain/x/pole/keeper/burn.go` | 销毁五通道：奖励池 / 手续费 / 保证金托管与结算 |
| `chain/x/pole/keeper/emission.go` | 无上限发行：衰减参考曲线 + 活跃度挂钩调节 |
| `chain/x/pole/types/witness_reward.go` | 见证奖励精确切分（最大余数法 + 字典序 tie-break） |
| `chain/x/pole/keeper/msg_server.go` | 消息处理器 |
| `chain/x/pole/keeper/query_server.go` | 查询处理器 |
| `chain/x/pole/module.go` | 模块集成 |
| `chain/app/app.go` | 应用连接 |
| `chain/app/ante.go` | 手续费销毁装饰器（`fee_burn_bps`） |
| `chain/app/params/encoding.go` | 编码配置 |
| `chain/cmd/poled/main.go` | CLI 入口 |
| `chain/cmd/poled/cmd/root.go` | 命令脚手架 |

---

## 验证清单

验证白皮书合规性与安全架构（2026-10 全量审计修复对齐）:

1. ✅ **小时奖励区块:** `node_rewards.rs` 使用 `reward_block_secs = 3600`
2. ✅ **玩家权重公式:** `node_rewards.rs:adjusted_player_block_reward` 计算 `weight = time * game_weight`
3. ✅ **挑战窗口:** `params.rs` 定义了 `challenge_window_blocks` 默认值
4. ✅ **最终确认:** `node_settlement.rs` 在 finalize 后设置 `epoch_finalized = true`
5. ✅ **跨周期调节:** `node_rewards.rs` 通过 `adjusted_player_block_reward` 实现负反馈
6. ✅ **GVS 层级:** `node_gvs.rs:classify_tier` 将分数映射到 ppm 范围层级
7. ✅ **服务奖励分配:** `tokenomics.rs` 定义了 `SERVICE_REWARD_ALLOCATION_BPS`
8. ✅ **信号分层定义 (P0-1):** 明确「微观游玩事实」（客户端自证，受心跳与进程约束）与「宏观一致性信号」（比对受信任源规模偏离）的界限，语义无循环论证。
9. ✅ **受信任源白名单 (P0-2):** `activity_collector.rs` 强制官方 HTTPS 域名白名单，非白名单端点直接拒绝，社区源置信度上限 500,000 ppm。
10. ✅ **女巫攻击与质押门槛 (P0-3):** 见证节点需质押 `RequiredBondedTokensForNode`，禁止见证人与玩家共享奖励地址，结算要求不同见证人奖励地址相互隔离。
11. ✅ **游玩时长心跳强绑定 (P0-4):** `keeper/session.go:SettlePlaySession` 结算时长强截断于 `min(session.PlaySeconds, heartbeats * bucketSeconds)`，杜绝挂名空转。
12. ✅ **后门剔除与二进制校验 (P0-5):** 调试环境变量覆盖仅限测试构建，正式构建物理剥离；增加 PE 头部 MZ/PE 指纹与 SHA256 校验，CI 门禁脚本阻断后门泄露。
13. ✅ **观测容差收紧 (P1-2):** 默认容差从 500‰ 收紧至 50‰（50,000 ppm），杜绝虚假规模套利。
14. ✅ **P2P 自动背书节流 (P1-3):** 添加背书幂等防重入、单 Epoch 64 次配额上限与单对端滑动窗口频控。
15. ✅ **供给压力测试与软护栏 (P1-1):** 跨 30 年多情景（熊/基准/牛/极限）净供给仿真矩阵验证，`ensureRewardPool` 内嵌年度总预算软护栏。
16. ✅ **复合前缀索引范围查询 (P2):** `keeper.go` / `session.go` 全量采用 `NewPrefixedTripleRange` 与 `NewPrefixedPairRange` 消除 O(n) 全表扫。
17. ✅ **证明层重构 L1 二进制物理身份与签名验证 (Issue #1):** `src/proof/l1_binary.rs` 引入物理可执行文件路径解析 (`QueryFullProcessImageNameW`)、PE DOS/NT 结构校验、SHA-256 指纹计算、Win32 离线 Authenticode 数字签名验证 (`WinVerifyTrust` 禁用 CRL 在线拉取防卡顿)、发行商证书主题提取与白名单比对，以及 Steam `appmanifest_{appid}.acf` 物理匹配优雅降级。
18. ✅ **证明层重构 L2 GPU 3D 渲染与图形后端活跃度采样 (Issue #1):** `src/proof/l2_render.rs` 引入运行期模块快照枚举 (`K32EnumProcessModules`)，识别 DirectX 11/12、Vulkan、OpenGL 与 DXGI 交换链运行库加载状态；在 `os_support.rs:evaluate_game_engagement` 与 `find_process_id_by_name` 中无缝集成 L1/L2 验证，杜绝无渲染控制台/脚本挂名欺骗。
19. ✅ **证明层重构 L3 硬件信任根与反虚拟化女巫证明 (Issue #1):** `src/proof/l3_hardware.rs` 引入 Windows TPM Base Services (TBS 2.0) 与 CNG Platform Crypto Provider (`ncrypt.dll`) 硬件密钥提供者探测，利用 x86 CPUID (Leaf 1 ECX.31 + Leaf 0x40000000) 硬件指令识别虚拟化 Hypervisor 签名 (KVM/Hyper-V/VMware/Xen)，物理阻断 VPS/Docker 批量女巫农场。
20. ✅ **证明层重构 L4 术语澄清与综合证明评级模型 (Issue #1, #2):** `src/proof/composite.rs` 融合 L1 物理身份、L2 GPU 渲染与 L3 硬件环境，构建分层置信阶梯（Gold 100 / Silver 80 / Bronze 50 / Degraded Fallback 20）；明确 L4 专属于人体在场与物理边界证明（软件层面无法完全证明真实人类物理在场，诚实披露硬件宏与外挂物理边界），降级评级规范命名为 `TierDegradedFallback`，并在缺失 TPM 或处于非签名/非 Steam 环境时透明披露具体降级原因。
21. ✅ **证明层 L1 架构修正与精确叶子证书匹配 (Issue #2):** 解耦客户端原始客观证据采集 (`RawBinaryEvidence`) 与见证者/验证节点评级计算 (`evaluate_binary_tier`)，杜绝客户端自评自报；在 Win32 PKCS#7 解构中使用 `CMSG_SIGNER_INFO_PARAM` 提取主签名者 `Issuer` 与 `SerialNumber`，通过 `CERT_FIND_SUBJECT_CERT` 精确匹配叶子实体证书，杜绝中间 CA / 时间戳响应者误判；将裸 PE 二进制 (`L1BareBinary`) 奖励权重清零 (`reward_weight_bps = 0`, `is_reward_eligible = false`)，严防任意进程 (如 notepad.exe) 伪造获取收益；强化 Steam 库目录物理包含性校验 (`is_exe_within_steam_installdir`)，严格锁定 `common/<installdir>` 子树。
22. ✅ **虚拟机玩家激励与物理 GPU 直通识别支持:** 在 `src/proof/l2_render.rs` 中引入 DXGI 适配器枚举 (`query_dxgi_primary_adapter`) 与 GPU 厂商用户态驱动 (UMD) 模块识别 (`detect_gpu_device`)，精准辨识物理硬件 GPU (NVIDIA/AMD/Intel) 与软件虚拟光栅化器 (WARP/SVGA/VirtualBox)；在 `src/proof/composite.rs` 中优化虚拟机游玩机制：(1) 语义中立化，明确告知虚拟机环境受支持且享有出块收益；(2) 针对配置了真实物理独显直通（PCIe Passthrough / VFIO / 云游戏主机）且运行合法游戏的虚拟机，放行晋升至 **Tier 2 (Silver)**，在保障真实高规格玩家权益与防范廉价机房女巫套利间取得完美平衡。