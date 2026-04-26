# PoLE 实施计划

**项目:** PoLE (Proof of Live Engagement)  
**版本:** V1.0  
**最后更新:** 2026-04-26

## 1. 项目概述

PoLE 是一个围绕 PC 游戏真实参与信号构建的专用应用型网络。协议将"某个小时内，谁真实参与了游戏、参与了多少、参与的是哪类游戏"转化为可验证、可分配、可追溯的链上奖励结果。

### 核心原则

1. **玩家优先。** 玩家奖励是主奖励，服务奖励是辅助奖励。
2. **小时结算。** 奖励按 1 小时为最小结算单元。
3. **跨周期调节。** 下一调节周期的固定玩家奖励根据上一调节周期总权重反向调整。
4. **链下大数据，链上关键结果。** 链上只承载承诺、奖励根和治理结果。
5. **可挑战、可纠错。** 错误结果在挑战窗口内可被纠正。

## 2. 架构设计

### 2.1 双层设计

```
┌─────────────────────────────────────────────────────────────┐
│                    Rust 原型层                               │
│  (采集、证据保留、本地验证、P2P 网络)                          │
├─────────────────────────────────────────────────────────────┤
│                    Cosmos SDK 链层                           │
│  (承诺、结算、Challenge、Finalize、领取)                      │
└─────────────────────────────────────────────────────────────┘
```

- **Rust 层:** 链下数据采集、证据保留、本地验证和 P2P 网络。
- **Cosmos 层:** 链上承诺记录、奖励根、挑战裁决、惩罚执行和治理。

### 2.2 协议流程

```
采集 -> 批次整理 -> 链上承诺 -> 聚合 -> 奖励根生成 -> Challenge -> Finalize -> 领取
```

## 3. Rust 原型组件

### 3.1 模块结构

| 模块 | 用途 |
|------|------|
| `activity_collector` | 从各种来源采集游戏活动信号 |
| `steam_collector` | Steam 平台活动采集 |
| `node_daemon` | 节点运行时和采集循环 |
| `node_pipeline` | 批次组装和默克尔树构建 |
| `node_aggregator` | Epoch 聚合和根计算 |
| `node_settlement` | 本地链状态管理和 epoch 最终确认 |
| `node_rewards` | 奖励计算和分发 |
| `node_verifier` | 本地 epoch 验证 |
| `node_storage_audit` | 存储挑战执行 |
| `p2p_libp2p` | LibP2P 网络实现 |
| `wallet` | 密钥管理、密钥库、签名 |
| `governance_runtime` | 治理提案和投票 |

### 3.2 二进制目标

| 二进制 | 特性 | 用途 |
|--------|------|------|
| `pole-client` | default | 玩家和运维 CLI |
| `pole-node` | default | 节点服务 CLI |
| `pole-gui` | gui | 桌面 GUI 入口 |

### 3.3 数据流

1. **采集:** `activity_collector` + `steam_collector` 采集游戏活动
2. **批次组装:** `node_pipeline.BatchBuilder` 组装 `AssembledBatch`
3. **P2P 分发:** `p2p_libp2p` 广播批次和收据
4. **Epoch 聚合:** `node_aggregator` 计算 `EpochAggregationArtifact`
5. **本地结算:** `node_settlement` 管理 epoch 生命周期
6. **奖励计算:** `node_rewards` 计算每个玩家的奖励

## 4. Cosmos 链组件

### 4.1 模块结构

| 组件 | 位置 | 状态 |
|------|------|------|
| `x/pole/types` | `chain/x/pole/types/` | ✅ 完成 |
| `x/pole/keeper` | `chain/x/pole/keeper/` | ✅ 完成 |
| `x/pole/module.go` | `chain/x/pole/module.go` | ✅ 完成 |
| `app/app.go` | `chain/app/app.go` | ✅ 完成 |
| `cmd/poled/main.go` | `chain/cmd/poled/main.go` | ✅ 完成 |

### 4.2 消息类型 (MsgServer)

| 消息 | 处理器 | 用途 |
|------|--------|------|
| `MsgUpsertNode` | `UpsertNode` | 注册/更新节点记录 |
| `MsgSubmitBatch` | `SubmitBatch` | 提交批次承诺 |
| `MsgSubmitReplicaReceipt` | `SubmitReplicaReceipt` | 提交存储收据 |
| `MsgCommitEpoch` | `CommitEpoch` | 提交 epoch 聚合和奖励 |
| `MsgOpenChallenge` | `OpenChallenge` | 对已承诺数据发起挑战 |
| `MsgResolveChallenge` | `ResolveChallenge` | 带 slash/reward 的挑战裁决 |
| `MsgFinalizeEpoch` | `FinalizeEpoch` | 挑战窗口结束后最终确认 epoch |
| `MsgClaimReward` | `ClaimReward` | 领取累积奖励 |
| `MsgUpsertGameWeight` | `UpsertGameWeight` | 更新游戏权重条目 |
| `MsgUpdateParams` | `UpdateParams` | 更新协议参数 |

### 4.3 查询类型 (QueryServer)

| 查询 | 用途 |
|------|------|
| `Params` | 当前协议参数 |
| `Node` | 按操作员地址查询节点记录 |
| `BatchCommit` | 按 epoch/collector/root 查询批次承诺 |
| `AggregateRecord` | 按 epoch/app 查询聚合记录 |
| `ReplicaReceipt` | 按 epoch/storer/payload 查询副本收据 |
| `EpochCommit` | 按 epoch ID 查询 epoch 承诺 |
| `RewardRecord` | 按 epoch/recipient 查询奖励记录 |
| `ClaimedReward` | 查询已领取奖励记录 |
| `Challenge` | 按 ID 查询挑战 |
| `GameWeight` | 查询游戏权重条目 |

### 4.4 Cosmos 模块复用

| 模块 | 用途 |
|------|------|
| `x/auth` | 账户和交易认证 |
| `x/bank` | POLE 代币转账和余额 |
| `x/staking` | 验证者/运营者质押 |
| `x/slashing` | 基本惩罚机制 |
| `x/gov` | 治理参数更新 |
| `x/mint` | 长期发行曲线 |
| `x/epochs` | 小时奖励区块和调节周期 |

## 5. 实施状态

### 5.1 Rust 原型

| 功能 | 状态 | 备注 |
|------|------|------|
| 核心数据类型 | ✅ 完成 | `primitives.rs`, `records.rs` |
| 活动采集 | ✅ 完成 | Steam, Epic, GOG, EA 采集器 |
| 批次组装 | ✅ 完成 | `BatchBuilder`, Merkle 证明 |
| P2P 网络 | ✅ 完成 | libp2p 支持 socket 模式 |
| Epoch 聚合 | ✅ 完成 | `aggregate_local_epoch` |
| 本地结算 | ✅ 完成 | `settle_local_epoch` |
| 奖励计算 | ✅ 完成 | 基于 GVS 的权重 |
| 存储审计 | ✅ 完成 | 保留挑战 |
| CLI 工具 | ✅ 完成 | Client, node, GUI 二进制 |
| 测试 | ✅ 编译通过 | 17 个测试文件 |

### 5.2 Cosmos 链

| 功能 | 状态 | 备注 |
|------|------|------|
| Proto 定义 | ✅ 完成 | `proto/pole/chain/pole/v1/` |
| 类型生成 | ✅ 完成 | `tx.pb.go`, `query.pb.go` |
| MsgServer 实现 | ✅ 完成 | 完整验证和状态转换 |
| QueryServer 实现 | ✅ 完成 | 所有查询端点 |
| Keeper (collections) | ✅ 完成 | Schema, CRUD 操作 |
| App 连接 | ✅ 完成 | 所有模块已连接 |
| CLI daemon | ✅ 完成 | `cmd/poled/main.go` |
| 构建验证 | ✅ 通过 | `go build ./...` |

### 5.3 剩余工作

| 项目 | 优先级 | 备注 |
|------|--------|------|
| Rust-to-Cosmos 桥接 | 高 | 将本地 artifact 转换为 Cosmos 交易 |
| 完整集成测试 | 高 | 端到端流程测试 |
| CI/CD 流水线 | 中 | 自动化构建和测试 |
| 发布打包 | 中 | MSI, deb, 便携版构建 |

## 6. 验证

### 6.1 构建验证

```bash
# Rust 构建
cargo build --release --features gui

# Go 构建
cd chain && go build ./...
```

### 6.2 测试执行

```bash
cargo test
cd chain && go test ./...
```

## 7. 部署目标

### 7.1 V1 发布要求

- [x] Windows MSI 安装包
- [x] Windows 便携版压缩包
- [ ] Linux deb 包
- [ ] 版本发布说明
- [ ] SHA256 校验和

## 8. 白皮书合规

V1 实现了以下白皮书要求:

1. ✅ 1 小时奖励区块作为最小结算单元
2. ✅ 玩家奖励为主，服务奖励为辅
3. ✅ 链下采集验证，链上承诺结算
4. ✅ 挑战窗口内关键证据可取回
5. ✅ 跨周期奖励调节
6. ✅ 基于游戏权重 (GVS) 的分配

## 9. 已知限制

1. **Rust-to-Cosmos 桥接:** 尚未实现，本地 artifact 不能直接提交到 Cosmos 链。
2. **永久归档:** PoLE 不保证在挑战窗口后永久保留所有原始数据。
3. **AI/ZK 证明:** V1 中不使用 AI 或 ZK 作为正确性前提。
4. **跨链:** V1 中没有跨链桥。