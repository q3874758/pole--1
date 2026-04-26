# PoLE 追踪矩阵

**项目:** PoLE (Proof of Live Engagement)  
**版本:** V1.0  
**最后更新:** 2026-04-26

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

### 3.3 小时奖励分账

| 概念 | Rust 位置 | Cosmos 位置 |
|------|-----------|-------------|
| `Player_Hour_Reward` | `node_rewards.rs` - `reward_local_epoch` 公式 | `keeper.CalcPlayerReward` |
| `Hourly_Reward_Pool` | `tokenomics.rs` - `PLAYER_REWARD_ALLOCATION_BPS` | `x/mint` 模块 |
| `Total_Hour_Weight` | `node_aggregator.rs` - `aggregate_record_root` | `keeper.ComputeEpochCommitments` |

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
| `src/p2p_libp2p.rs` | P2P 网络 |
| `src/governance_runtime.rs` | 治理执行 |
| `src/wallet/` | 密钥管理和签名 |
| `src/tokenomics.rs` | 代币经济参数 |
| `src/params.rs` | 协议参数 |

### Cosmos 链文件

| 文件 | 白皮书覆盖 |
|------|-----------|
| `chain/x/pole/types/state.proto` | 链上状态类型 |
| `chain/x/pole/types/tx.proto` | 消息类型 |
| `chain/x/pole/types/query.proto` | 查询类型 |
| `chain/x/pole/keeper/keeper.go` | 状态持久化和业务逻辑 |
| `chain/x/pole/keeper/msg_server.go` | 消息处理器 |
| `chain/x/pole/keeper/query_server.go` | 查询处理器 |
| `chain/x/pole/module.go` | 模块集成 |
| `chain/app/app.go` | 应用连接 |
| `chain/cmd/poled/main.go` | CLI 入口 |

---

## 验证清单

验证白皮书合规性:

1. ✅ **小时奖励区块:** `node_rewards.rs` 使用 `reward_block_secs = 3600`
2. ✅ **玩家权重公式:** `node_rewards.rs:adjusted_player_block_reward` 计算 `weight = time * game_weight`
3. ✅ **挑战窗口:** `params.rs` 定义了 `challenge_window_blocks` 默认值
4. ✅ **最终确认:** `node_settlement.rs` 在 finalize 后设置 `epoch_finalized = true`
5. ✅ **跨周期调节:** `node_rewards.rs` 通过 `adjusted_player_block_reward` 实现负反馈
6. ✅ **GVS 层级:** `node_gvs.rs:classify_tier` 将分数映射到 ppm 范围层级
7. ✅ **服务奖励分配:** `tokenomics.rs` 定义了 `SERVICE_REWARD_ALLOCATION_BPS`