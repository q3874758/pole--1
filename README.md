# PoLE (Proof of Live Engagement)

> 首个面向全球 PC 游戏生态的去中心化真实游玩证明与双边价值分配网络。

PoLE（Proof of Live Engagement）通过对等节点间的**相互证明与反作弊见证**，确认玩家真实在线游玩且未作假；协议为**玩游戏的节点（玩家主奖励）**和**做证明的节点（见证服务奖励）**发放双边激励。系统采用**无上限弹性发行与全场景动态活动销毁（Mint & Burn Equilibrium）**模型，并内嵌硬件调度级**原生零开销静默后台运行时（EcoQoS）**，在完全不影响前台 3D 游戏性能的前提下持续稳定运行。

---

## 核心机制一览

- **去中心化相互见证 (Mutual Proof)**：节点间互发微片段心跳与随机挑战，采集防挂机特征熵值；会话需满足门限见证（$\ge 2$ 独立见证人）与时间桶心跳覆盖（$\ge 50\%$ 覆盖率）方可通过链上结算，杜绝时长作假与自证作弊。
- **双边激励分配 (Dual Rewards)**：每个 1 小时奖励结算周期，80% 分配给真实游玩的玩家节点，10% 按有效见证背书贡献占比（`AllocateWitnessRewards` 最大余额法）发放给见证节点。
- **无上限弹性发行与五通道销毁 (Mint & Burn)**：取消固定硬顶，发行跟踪实际活跃度与确认结算；通过交易手续费销毁（`fee_burn_bps`）、超额提现阶梯税（`reward_burn_bps`）、治理提案销毁（`governance_burn_bps`）、挑战押金销毁（`challenge_bond_burn_bps`）与作弊罚没（`session_slash_bps`）构成通缩黑洞，维持代币内在价值。
- **游戏级静默低耗运行时 (`os_support`)**：深度集成 Win32 / POSIX 原生系统调用，启用 `IDLE_PRIORITY_CLASS` 与 Windows EcoQoS（强制调度在能效小核 E-Core，释放性能大核给游戏），具备自动物理工作集内存裁剪（空闲驻留内存 **< 15MB**，CPU 占用 **< 0.1%**），彻底根除游戏微卡顿（Micro-stuttering）。

---

## 项目组件与技术栈

| 组件 | 技术栈 | 职责与能力 |
|---|---|---|
| **去中心化节点与客户端** | Rust (Tokio, Borsh, Ed25519) | 游戏进程感知、前台焦点探测、心跳分桶、本地验证与原生后台调度 (`pole-client` / `pole-node` / `pole`) |
| **专用结算与验证公链** | Cosmos SDK v0.54, CometBFT (Go) | 节点质押、批次承诺、互证判定 (`session.go`)、奖励根校验、按需铸币与五通道销毁 (`x/pole`, `poled`) |
| **桌面控制面与仪表盘** | Rust (HTTP / REST) + HTML5 / JS | 节点状态自检、P2P 拓扑监控、奖励收益查看、服务启停与自动更新 (`desktop/web/`) |

---

## 文档导航

### 核心规划与协议规范
- [PoLE 正式白皮书](docs_PoLE_Whitepaper.md)：协议愿景、博弈论假设、代币经济学与 30 年长期供给推导。
- [PoLE 项目总体实施规划书](docs/PoLE_Project_Plan.md)：整体商业叙事、相互见证技术规范、双边激励公式、销毁场景与分阶段 Roadmap。
- [PoLE 开发文档规范](DEVELOPMENT_DOCS.md)：文档边界约定与工程维护规则。
- [实施计划与状态矩阵](IMPLEMENTATION_PLAN.md)：功能模块分解、完成度对照与退出条件。
- [代码追踪矩阵 (Traceability)](TRACEABILITY.md)：白皮书协议公式到 Rust / Go 源码实现的文件映射。

### 运维与操作指南
- [节点安装指引](docs/operations/install.md)：各平台便携包与系统依赖部署。
- [后台服务管理](docs/operations/service-management.md)：Windows Service (`sc`) 与 Linux systemd 服务注册、开机自启与配置。
- [测试网与联调](docs/operations/testnet.md)：本地全节点集群联调与跨节点网络验证。
- [故障排查手册](docs/operations/troubleshooting.md)：常见问题排查与节点诊断。
- [自动更新机制](docs/operations/update.md)：签名清单拉取、增量更新与安全回滚。

---

## 本地构建与全量验证

### 1. Rust 节点与工具链构建
```bash
# 运行单元测试 (212 项测试全部通过)
cargo test --lib

# 运行全目标测试 (含客户端 CLI 与控制面)
cargo test --all-targets

# 代码静态分析与格式检查
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

### 2. Cosmos SDK 应用链构建
```bash
cd chain
go build -o poled.exe ./cmd/poled
go test -count=1 ./...
```

### 3. 链上链下跨系统端到端集成测试
```bash
# 启动真实 poled 节点并在集成环境中执行 8 套互证、挑战与双边发放全流程
cargo test --features integration --test integration
```

---

## 交付与打包布局

项目默认采用便携式免安装（Portable）运行目录布局：
- **可执行文件**：`pole-client.exe` / `pole-node.exe` / `poled.exe`
- **配置文件**：`node.json` 或 `client.json`
- **数据目录**：`pole-node-data/`（含本地身份加密密钥库 `identity.json`）
- **日志目录**：`pole-node-data/logs/`
