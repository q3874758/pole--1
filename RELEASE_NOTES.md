# PoLE 发布说明

**版本:** 0.1.2（V1 协议）
**发布日期:** 2026-10-02
**项目:** PoLE (Proof of Live Engagement)

---

## 概述

PoLE 是围绕 PC 游戏真实参与信号构建的专用应用型网络。

本版本实现了白皮书定义的完整协议流程，采用"链下采集与复核，链上承诺、结算与 Challenge"的最小可信结构，并新增节点互证模型：游玩声明由节点自身发起、由至少两个独立见证节点证明，链上据心跳覆盖、见证数与独立观测数判定有效性。

---

## 核心功能

### 1. 小时奖励区块
- 以 1 小时作为最小奖励结算单元
- 玩家权重 = 有效游玩时长 × 游戏权重
- 奖励按个人权重占全网总权重比例分配
- 玩家奖励收款人就是节点自身（不引入独立 `player_address`）

### 2. 节点互证（本版本新增）
- `PlaySession` / `PlayHeartbeat` / `WitnessAttestation` 链下构造（`src/mutual_proof.rs`）
- 链上判定（`chain/x/pole/keeper/session.go`）：见证不得是玩家本人或该会话采集者、必须有自身独立观测、观测不得复用会话载荷且偏离在容差内
- 有效会话权重才进入分账与活动度信号

### 3. 跨周期调节与发行
- 根据上一调节周期全网总权重，用平方根负反馈函数调节额度，±cap 夹逼
- 发行无总量硬上限：按衰减参考曲线持续铸造，已确认奖励超出参考量时补铸精确短缺额
- 长期约束来自三项可实测机制：单位奖励负反馈、单调衰减曲线、随活跃度浮动的销毁

### 4. 五通道销毁
- `fee_burn_bps`(ante)、`reward_burn_bps`(claim)、`governance_burn_bps`、`challenge_bond_burn_bps`、`session_slash_bps`
- 全部由 `chain/x/pole/keeper/burn.go` 实现，一律按模块余额有界

### 5. Challenge 机制
- 挑战窗口内可对承诺结果提出争议
- 支持批承诺、聚合根、奖励根、数据可用性、虚假游玩声明等多种挑战类型
- 验证失败将触发惩罚和奖励调整

### 6. 治理功能
- 协议参数可通过治理提案更新
- 玩家和服务节点均可参与治理
- 参数变更仅对未来时段生效

---

## 组件清单

### Rust 客户端与节点

| 组件 | 版本 | 说明 |
|------|------|------|
| pole-client | 0.1.2 | 玩家和运维 CLI |
| pole-node | 0.1.2 | 节点服务 CLI |
| pole | 0.1.2 | 统一调度器：`pole [client\|node\|genesis\|sbom] <cmd>` |
| pole-genesis | 0.1.2 | 创世构建（兼容 shim） |
| pole-sbom | 0.1.2 | SBOM 与许可证审计（兼容 shim） |

### Cosmos SDK 链

| 组件 | 版本 | 说明 |
|------|------|------|
| poled | 0.1.2 | 链节点守护进程（`chain/cmd/poled`） |
| x/pole | 0.1.2 | PoLE 自定义模块 |

---

## 平台支持

### Windows
- ✅ 服务安装/卸载脚本与布局（`packaging/windows/`）
- ⬜ x64 安装包：构建脚本尚未提交，暂无产物与校验和

### Linux
- ✅ deb 构建脚本（`packaging/linux/deb/build-package.sh`）与 systemd unit
- ⬜ deb 产物：需在目标平台构建后生成

---

## 构建与验证

### Rust

```bash
cargo build --release
cargo test --all-targets
```

实测结果：433 收集 / 432 passed / 1 ignored / 0 failed（含 lib 200 项）。

### Go 链

```bash
cd chain
go build -o poled.exe ./cmd/poled
go test ./... -count=1
```

实测结果：

```
ok  pole/chain/app
ok  pole/chain/x/pole/keeper
ok  pole/chain/x/pole/types
```

覆盖发行曲线、五通道销毁、互证结算、见证奖励切分，以及 20 年/30 年长期供给模拟。

### 端到端集成

```bash
cd chain && go build -o poled.exe ./cmd/poled   # 并把 chain/ 加入 PATH
cargo test --features integration -- --test-threads=1
```

实测结果：4 passed / 0 failed（真实 `poled` 链，逐场景串行）。

---

## 文档

| 文档 | 说明 |
|------|------|
| `docs_PoLE_Whitepaper.md` | 正式版白皮书（含互证口径与无上限发行验证） |
| `IMPLEMENTATION_PLAN.md` | 实施计划与当前状态 |
| `TRACEABILITY.md` | 白皮书到代码的映射 |
| `chain/README.md` | 链的构建、测试与关键约定 |
| `docs/operations/install.md` | 安装指南 |
| `docs/operations/service-management.md` | 服务管理 |
| `docs/operations/troubleshooting.md` | 故障排查 |

---

## 已知限制

1. **见证激励偏弱:** 见证奖励复用 `verify_reward_bps`（1500 bps）后实际占发行约 1.5%，是已知参数敏感点，需治理上调
2. **见证合谋:** `min_distinct_observations` 无法阻止多个节点串通伪造独立观测，V1 依赖挑战窗口与 `session_slash_bps` 事后纠错
3. **永久归档:** PoLE 不保证在挑战窗口后永久保留所有原始数据
4. **跨链:** V1 不包含跨链桥功能
5. **安装包产物:** Windows 安装包构建脚本尚未提交；Linux deb 需在目标平台自行构建

---

## 安全说明

- 本软件按"原样"提供，不提供任何明示或暗示的保证
- 建议在正式运行前进行完整的安全审计
- 参与奖励前请充分了解 PoLE 协议机制和风险

---

## 后续计划

- 补齐 Windows 安装包构建脚本与产物校验和
- 治理参数标定：见证激励占比、容差与心跳参数实测后调优
- 增强数据可用性证明和更多信号源支持

---

**PoLE Team**
2026 年 10 月 2 日
