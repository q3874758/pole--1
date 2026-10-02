# PoLE Cosmos SDK 链

这个目录是 PoLE 的链上实现：Rust 节点负责链下采集与互证记录构造，本目录的 Cosmos SDK 应用负责链上承诺、互证判定、奖励根、Challenge、Finalize、Claim、发行与销毁。

## 设计边界

PoLE 不是通用智能合约平台。固定流程是：

`采集 -> 批次整理 -> 链上承诺 -> 互证判定 -> 聚合 -> 奖励根生成 -> Challenge -> Finalize -> 领取`

Cosmos 版本复用自带能力，再补一个 PoLE 专用模块 `x/pole`。

## 模块划分

复用的 Cosmos 模块：

- `x/auth`：账户与交易认证
- `x/bank`：POLE 资产转账与余额
- `x/staking`：验证者/运营者质押关系
- `x/slashing`：基础惩罚机制
- `x/gov`：治理参数更新

PoLE 自定义模块 `x/pole` 承接的协议对象：

- `BatchCommit`
- `EpochCommit`
- `RewardRecord`
- `Challenge`
- `AvailabilityRecord`
- `GameWeightEntry`
- `PlaySession` / `PlayHeartbeat` / `WitnessAttestation` / `SessionSettlement`（互证模型）
- 协议参数与奖励调节公式

## 目录说明

- `proto/pole/chain/pole/v1/`：protobuf 源文件（`tx` / `query` / `state` / `genesis`）
- `proto/buf.yaml`、`proto/buf.gen.yaml`、`proto/buf.lock`：代码生成配置
- `x/pole/types/`：生成类型加手写领域逻辑——参数（`helpers.go`）、奖励数学（`reward_math.go`）、发行曲线（`emission.go`）、默克尔根（`merkle.go`）、见证奖励切分（`witness_reward.go`）、签名者（`msg_signers.go`）、collections codec（`codec.go`）
- `x/pole/keeper/`：真实链上存储与业务逻辑（`keeper.go`）、消息处理器（`msg_server.go`）、查询处理器（`query_server.go`）、互证（`session.go`）、销毁五通道（`burn.go`）、无上限发行（`emission.go`）
- `x/pole/module.go`：Cosmos 模块入口与服务注册
- `app/app.go`：链装配入口（含 `BroadcastLocalMsgs`）
- `app/ante.go`：手续费销毁装饰器（`fee_burn_bps`）
- `cmd/poled/`：链节点 CLI 入口

## 构建与测试

在 `E:\pole备用\chain` 下：

```powershell
$env:GOFLAGS='-mod=readonly'
go build -o poled.exe ./cmd/poled
go test ./... -count=1
```

测试覆盖：

- `chain/app/burn_test.go`：销毁五通道与净供给对账
- `chain/app/session_test.go`：互证判定（见证独立性、心跳覆盖、独立观测数、容差）
- `chain/app/supply_simulation_test.go`：无上限发行的长期供给验证（20 年逐年额度、30 年发行率单调衰减、销毁跟进）
- `chain/app/app_test.go`：`FinalizeEpoch` 门禁与根校验
- `chain/x/pole/types/*_test.go`：奖励数学、发行曲线、默克尔根、见证奖励切分
- `chain/x/pole/types/emission_cross_language_test.go`、`merkle_cross_language_test.go`：与 Rust 侧 `src/tokenomics.rs` / 根计算共用 fixtures

## 关键约定

- 链 ID 与 `AppName` 均为 `pole`，与 Rust 侧默认值一致。
- 每条 `Msg` 的签名者由 bech32 地址字段推导（`msg_signers.go`），因此广播密钥必须与 `node_address` / `collector` / `proposer` / `witness` / `settler` 一致；提交批次要求 `msg.Collector == BatchCommitWire.collector_address`，提交 epoch 要求 `msg.Proposer == EpochCommitWire.proposer_address`。
- 地址一律由公钥派生：`cosmos_account_from_pubkey` = `sha256(pubkey)[..20]`，再 bech32 编码；节点 `NodeId`（`stable_hash32`）不是账号，不可互转。
- `FinalizeEpoch` 强制校验奖励根与聚合根，缺失或不等即拒绝。
- 销毁一律按模块余额有界，销毁不足时只销毁实际可销毁量。
