# PoLE Cosmos SDK 重写

这个目录是 PoLE 的链上重写工作区，目标是把当前 Rust 原型中的链上状态机部分迁到 Cosmos SDK。

当前仓库会分成两层：

1. Rust 继续负责链下采集、证据保留、本地复核和临时网络联调。
2. Cosmos SDK 链负责链上承诺、奖励根、Challenge、Finalize、Claim 和治理参数。

这和白皮书定义的最小可信边界一致：

- 链下：采集、传播、批次整理、证据保留
- 链上：承诺、结算、争议、惩罚、领取、治理

## 设计边界

PoLE 不是通用智能合约平台。

白皮书给出的固定流程是：

`采集 -> 批次整理 -> 链上承诺 -> 聚合 -> 奖励根生成 -> Challenge -> Finalize -> 领取`

因此 Cosmos 版本不会先做一套通用 VM，而是复用 Cosmos 自带能力，再补一个 PoLE 专用模块。

## 模块划分

计划复用的 Cosmos 模块：

- `x/auth`：账户与交易认证
- `x/bank`：POLE 资产转账与余额
- `x/staking`：验证者/运营者质押关系
- `x/slashing`：基础惩罚机制
- `x/gov`：未来生效的治理参数更新
- `x/mint`：长期发行曲线
- `x/epochs`：小时奖励区块与更长调节周期

PoLE 自定义模块：

- `x/pole`

`x/pole` 负责承接白皮书和当前 Rust 原型中的协议对象：

- `BatchCommit`
- `EpochCommit`
- `RewardRecord`
- `Challenge`
- `AvailabilityRecord`
- `GameWeightEntry`
- 协议参数与奖励调节公式

## 当前状态

目前已经落地：

- `x/pole` 的手写协议类型
- 参数默认值与校验逻辑
- 玩家权重、小时奖励、跨周期负反馈调节公式
- collections 驱动的真实 keeper 存储
- `module.go` 骨架
- 最小 `app.go`，可把 `x/pole` 挂进 BaseApp
- `proto` 源文件：`tx/query/state/genesis`

## 目前还没做

- `proto` 代码生成
- `MsgServer` / `QueryServer` 的真实注册
- CLI / daemon / node 二进制
- Rust 采集端到 Cosmos tx 的桥接
- `x/auth`、`x/bank`、`x/staking`、`x/gov` 的完整接线

## 为什么先这样拆

白皮书明确要求 V1 是固定功能状态机，不是“先上链再慢慢想协议对象”。

所以这里先做四件事：

1. 把白皮书对象稳定成链上状态结构。
2. 把 Rust 原型里的参数和奖励公式迁过来。
3. 把 keeper 从内存模型切到 Cosmos collections 存储。
4. 让 `x/pole` 至少能被一个最小 Cosmos app 正常挂载。

## 目录说明

- `proto/pole/chain/pole/v1/`：PoLE 的 protobuf 源文件
- `x/pole/types/`：当前手写领域类型、参数、奖励公式、collections codec
- `x/pole/keeper/`：真实链上存储层
- `x/pole/module.go`：Cosmos 模块入口
- `app/app.go`：最小链装配入口

## 下一步

1. 加 `buf.yaml` / `buf.gen.yaml` 并生成 Go 代码。
2. 把 `RegisterServices` 从占位实现替换成真实 `MsgServer` / `QueryServer` 注册。
3. 用生成的 proto 类型替换手写状态类型，减少双重定义。
4. 把 Rust 采集端产出的本地 artifact 改成链上消息提交。
