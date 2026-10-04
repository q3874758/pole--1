# PoLE (Proof of Live Engagement)

> **首个面向全球 PC 游戏生态的去中心化真实游玩证明、双边激励与动态活动销毁网络。**

[![CI](https://github.com/q3874758/pole--1/actions/workflows/ci.yml/badge.svg)](https://github.com/q3874758/pole--1/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)
[![Cosmos SDK](https://img.shields.io/badge/Cosmos_SDK-v0.54-5C6BC0.svg)](https://cosmos.network/)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-DEA584.svg)](https://www.rust-lang.org/)
[![Go](https://img.shields.io/badge/Go-1.22%2B-00ADD8.svg)](https://go.dev/)

---

## 📖 项目愿景与设计哲学

传统 GameFi 与链上游戏激励长期受困于两大痛点：**脚本泛滥/虚假挂机自证作弊**，以及**代币无限通胀导致经济崩盘**。

**PoLE（Proof of Live Engagement）** 彻底重构了游戏价值捕获模型：
1. **真实游玩，相互见证**：无需可信中心化服务器，通过对等节点间的**相互证明与反作弊见证**确认玩家真实在玩游戏且未作假。**玩家挂机睡觉（AFK）在游戏世界中完全认可**，但**停留在启动器（Launcher）、登录界面或主菜单（Title Screen）将被精准甄别并拒绝发奖**。
2. **双边激励，全网发奖**：协议同时为**玩游戏的节点（玩家主奖励）**与**提供见证背书的节点（见证服务奖励）**发放双边激励，形成“玩即挖矿，见证即挖矿”的双边自驱生态。
3. **无上限弹性发行 + 活动销毁保值（通缩飞轮）**：奖励随全网真实游戏活跃度按需铸造，**无固定发行上限**；单位产出通过平方根负反馈平抑（`AdjustedHourlyReward`）。同时，协议开放**应用级活动销毁接口（`MsgActivityBurn`）**（由游戏发行商促销、电竞赛事门票、公会冲榜、赛季战令等自主销毁）及底层 5 大协议销毁通道，活动越繁荣、通缩销毁越强劲，持续支撑代币内在价值。
4. **原生零开销静默后台（EcoQoS）**：硬件调度级后台服务，基于 Win32 原生系统调用，强制绑定能效小核（E-Core）与自动物理内存裁剪（空闲驻留 **< 15MB**，CPU 占用 **< 0.1%**），彻底杜绝游戏掉帧与微卡顿（Micro-stuttering）。

---

## 🔄 核心运转流向图

```mermaid
flowchart TD
    subgraph Client["玩家节点 (Player Node)"]
        G[前台启动 PC 游戏] --> D[os_support 零开销状态评估]
        D -- "启动器 / 登录页 / 主菜单" --> Reject[⏸️ 暂停累计 / 拒绝心跳]
        D -- "进入游戏世界 (含挂机睡觉 AFK)" --> Accept[✅ 记录有效 PlayHeartbeat]
        Accept --> PackSession[📦 本地打包 PlaySession]
    end

    subgraph P2P["P2P Gossip 相互见证网络"]
        PackSession -- "广播 PlaySessionAnnouncement" --> P2PNet((P2P 网络总线))
        P2PNet --> W1[见证节点 A]
        P2PNet --> W2[见证节点 B]
        W1 -- "独立观测比对 (容差合格)" --> Sign1[✍️ 签名 WitnessAttestation]
        W2 -- "独立观测比对 (容差合格)" --> Sign2[✍️ 签名 WitnessAttestation]
        Sign1 & Sign2 -- "回传背书凭证" --> P2PNet
    end

    subgraph Chain["Cosmos SDK 应用链 (x/pole)"]
        P2PNet -- "提交会话与门限见证 (≥2 独立见证人)" --> SessionKeeper[⚖️ 会话裁决与承诺入块]
        SessionKeeper --> Mint[🪙 按需精确铸币 (无上限弹性供给)]
        Mint --> Settle[💰 双边全自动结算与分账]
        Settle --> PlayerReward["🎮 玩家主奖励 (80%~90%)"]
        Settle --> WitnessReward["🛡️ 见证节点奖励 (10%~20% 切分)"]
        
        Burn["🔥 多场景动态活动销毁 (MsgActivityBurn)"] -- "赛事门票 / 公会赞助 / 厂商推广" --> BlackHole[🕳️ 永久打入通缩黑洞]
        FeeBurn["🔥 5大底层销毁通道 (提现税/手续费/罚没)"] --> BlackHole
    end

    classDef success fill:#e8f5e9,stroke:#4caf50,stroke-width:2px;
    classDef warning fill:#fff3e0,stroke:#ff9800,stroke-width:2px;
    classDef primary fill:#e3f2fd,stroke:#2196f3,stroke-width:2px;
    classDef burn fill:#ffebee,stroke:#f44336,stroke-width:2px;

    class Accept,PlayerReward,WitnessReward success;
    class Reject warning;
    class PackSession,Sign1,Sign2,SessionKeeper,Mint,Settle primary;
    class Burn,FeeBurn,BlackHole burn;
```

---

## ⚡ 四大核心机制

### 1. 真实游玩判定（挂机睡觉 AFK 可以，停在主菜单不行）
* **游戏世界判定（In-World vs Main Menu）**：
  - 玩家角色在游戏内挂机、挂机睡觉、建造等**均视作合法有效参与**，全额累计并签名心跳。
  - 仅挂在平台登录页、启动器弹窗、选服列表或游戏主菜单（Title Screen）时，将被识别为 `MainMenu` 状态，停止心跳累计。
* **原生零开销多维感知**：
  - **窗口标题实时过滤**：基于 Win32 `GetWindowText` 过滤带有 `Launcher`、`Login`、`Title Screen` 等字样的主窗口；
  - **物理工作集内存跃迁检测**：区分主菜单（通常 < 120MB UI 内存）与 3D 游戏真实世界资产载入（数 GB 内存阶跃）；
  - **极速免子进程侦测**：直接调用 Win32 Toolhelp32 快照与 FFI，毫秒级比对目标游戏进程，无任何外部脚本或控制台弹出。

### 2. P2P 邻居相互见证自动广播闭环
* **去中心化广播总线**：
  - 节点生成本地 `PlaySession` 后，自动向 P2P 拓扑中的邻居节点广播 `PlaySessionAnnouncement`。
  - 在线见证节点收到广播后，将其与自身在该游戏和时段的独立采集观测数据对比；若偏差比率在容差范围内（`deviation_ppm <= min_witness_observation_tolerance_ppm`），立即通过本地 Ed25519 私钥生成 `WitnessAttestation` 并全网广播回传。
* **铁律门限防作弊**：
  - 链上强制要求 $\ge 2$ 个独立见证节点签名背书；
  - 见证节点不得与游玩节点相同（防止自证作弊）；
  - 时间桶心跳覆盖率需 $\ge 50\%$，彻底杜绝虚报时长与模拟器批量伪造。

### 3. 双边奖励全自动分配与结清
* **双边激励模型**：
  - **玩游戏节点**：获得 80%~90% 的主要游玩激励；
  - **见证节点**：瓜分 10%~20% 的独立验证池（`verify_pool`）。
* **链上最大余额法结算**：
  - 验证者根据周期内链上已结算有效会话的见证背书数量，通过 `AllocateWitnessRewards` 算法精确切分各见证人的收益（`verify_reward`），并随每小时区块全自动打包结清，无需人工中继。

### 4. 无上限弹性发行与动态活动销毁保值
* **无上限发行（按需供给）**：
  - 彻底移除固定的年度或月度配额硬顶，随全网真实玩家的游戏活跃度和结算确认按需铸造。
  - 引入平方根负反馈自适应调控机制（`AdjustedHourlyReward`），在高活跃期平抑单位时间代币产出，防范恶性通胀。
* **活动销毁通道（MsgActivityBurn，强保值）**：
  - 开放通用活动销毁入口，允许游戏发行商促销、电竞赛事主办方、公会冲榜赞助、游戏道具发行等应用场景直接上链销毁 POLE 代币并广播 `activity_burned` 事件。
* **协议底层 5 大常规销毁通道**：
  1. `fee_burn_bps`：交易手续费百分比销毁；
  2. `reward_burn_bps`：大额阶梯提现税销毁；
  3. `governance_burn_bps`：治理提案未通过押金销毁；
  4. `challenge_bond_burn_bps`：恶意挑战失败押金销毁；
  5. `session_slash_bps`：作弊节点罚没本金全额销毁。

---

## 🛠️ 项目组件与技术架构

| 模块组件 | 技术选型 | 运行定位与核心能力 |
|---|---|---|
| **去中心化节点与客户端** | Rust (Tokio, Borsh, Ed25519) | 游戏进程感知、前台焦点探测、心跳分桶、本地验证与原生后台调度 (`pole-client` / `pole-node` / `pole`) |
| **专用结算与验证公链** | Cosmos SDK v0.54, CometBFT (Go) | 节点质押、批次承诺、互证判定 (`session.go`)、奖励根校验、按需铸币与活动销毁 (`x/pole`, `poled`) |
| **桌面控制面与仪表盘** | Rust (HTTP / REST) + HTML5 / CSS3 / JS | 节点状态自检、P2P 拓扑监控、奖励收益查看、服务启停与本地仪表盘 (`desktop/web/`) |
| **原生后台服务驱动** | Win32 API / Windows Service (sc.exe) | Windows EcoQoS 调度、`IDLE_PRIORITY_CLASS`、工作集物理内存精简，零卡顿运行 |

---

## 🚀 快速上手与运行

### 1. 玩家端运行（一键免安装）

普通玩家使用打包发布的单文件客户端即可静默运行：
```powershell
# 运行安装脚本（将 pole-client 安装至用户目录并自动注册开机自启）
.\scripts\install-pole-player.cmd

# 或直接通过命令行启动前台侦测客户端
cargo run --bin pole-client -- start
```

### 2. 见证与全节点运行

见证节点参与 P2P 广播、观测数据采集与见证签名：
```powershell
# 启动独立节点守护进程
cargo run --bin pole-node -- run

# 注册为 Windows 后台系统服务（开机静默运行）
.\packaging\windows\install-service.cmd
```

### 3. 打开本地 Web 仪表盘

启动后访问本地轻量监控界面：
- 地址：`http://127.0.0.1:28789/`
- 可视化查看：当前游玩进程状态、In-World 判定、已累计见证数、待结算收益明细与活动销毁统计。

---

## 🧪 构建与全量测试验证

本仓库配备了工业级跨语言持续集成验证套件（GitHub Actions 全绿）：

```powershell
# 1. 运行所有 Rust 单元测试与端到端测试 (全部通过)
cargo test --all-targets

# 2. 静态分析与代码格式检查 (零警告)
cargo clippy --all-targets --features integration -- -D warnings
cargo fmt -- --check

# 3. 构建与验证 Cosmos SDK 链端
cd chain
go vet ./...
go test -count=1 ./...

# 4. 跨语言端到端全流程集成测试 (启动真实 poled 节点完成会话互证与奖励结清)
cargo test --features integration --test integration
```

---

## 📚 文档索引

- **核心协议与规划**：
  - [PoLE 正式白皮书](docs_PoLE_Whitepaper.md)：协议博弈论假定、代币经济学与 30 年长期供给模型推导。
  - [PoLE 项目总体实施规划书](docs/PoLE_Project_Plan.md)：商业叙事、相互见证技术规范、双边发奖公式与路线图。
  - [PoLE 开发文档规范](DEVELOPMENT_DOCS.md)：工程边界与文档维护准则。
  - [实施计划与状态矩阵](IMPLEMENTATION_PLAN.md)：功能模块分解与退出条件。
  - [代码追踪矩阵 (Traceability)](TRACEABILITY.md)：白皮书协议公式到 Rust / Go 源码实现的文件映射。
- **运维与操作指南**：
  - [节点安装指引](docs/operations/install.md)：各平台便携包与系统依赖部署。
  - [后台服务管理](docs/operations/service-management.md)：Windows Service 注册运维与 EcoQoS 能效配置。
  - [测试网与联调](docs/operations/testnet.md)：本地多节点集群联调与跨网络测试。
  - [故障排查手册](docs/operations/troubleshooting.md)：常见问题排查与节点诊断。
  - [自动更新机制](docs/operations/update.md)：签名清单验证、增量更新与安全回滚。

---

## 📄 开源许可证

本项目基于双重开源协议授权：
- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)
