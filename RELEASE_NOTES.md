# PoLE V1.0.0 发布说明

**版本:** V1.0.0
**发布日期:** 2026-04-26
**项目:** PoLE (Proof of Live Engagement)

---

## 概述

PoLE V1.0.0 是围绕 PC 游戏真实参与信号构建的专用应用型网络的正式发布版本。

本版本实现了白皮书定义的完整协议流程，采用"链下采集与复核，链上承诺、结算与 Challenge"的最小可信结构。

---

## 核心功能

### 1. 小时奖励区块
- 以 1 小时作为最小奖励结算单元
- 玩家权重 = 有效游玩时长 × 游戏权重
- 奖励按个人权重占全网总权重比例分配

### 2. 跨周期调节
- 根据上一调节周期全网总权重调节下一周期固定玩家奖励
- 采用平方根负反馈函数，防止权重剧烈波动

### 3. Challenge 机制
- 挑战窗口内可对承诺结果提出争议
- 支持批承诺、聚合根、奖励根、数据可用性等多种挑战类型
- 验证失败将触发惩罚和奖励调整

### 4. 治理功能
- 协议参数可通过治理提案更新
- 玩家和服务节点均可参与治理
- 参数变更仅对未来时段生效

---

## 组件清单

### Rust 原型

| 组件 | 版本 | 说明 |
|------|------|------|
| pole-client | V1.0.0 | 玩家和运维 CLI |
| pole-node | V1.0.0 | 节点服务 CLI |
| pole-gui | V1.0.0 | 桌面 GUI 入口 |

### Cosmos SDK 链

| 组件 | 版本 | 说明 |
|------|------|------|
| poled | V1.0.0 | 链节点守护进程 |
| x/pole | V1.0.0 | PoLE 自定义模块 |

---

## 平台支持

### Windows
- ✅ Windows x64 MSI 安装包
- ✅ Windows x64 便携版

### Linux
- 🔄 Linux deb 包（构建脚本已就绪）

---

## 校验信息

### Windows MSI 安装包
- 文件: `PoLE-Desktop-0.1.0-x64.msi`
- SHA256: `MSI 未包含在本次提交，需通过 packaging/windows/build-package.cmd 构建`

### Windows 便携版
- pole-client.exe: `ceb0b28e2eca7a9c1b0d059569de1210b7704824998041e6aa7d81060124b22d`
- pole-node.exe: `223334f14c2bcc29e780420359243930f59d574a706aae14034f51622d1ba8d6`
- pole-gui.exe: `4849c0766a733e58f8eda9c72a97697575cd9fdacf7cd8d96e3a55870a89d642`

### Rust 测试结果
```
cargo test: 280+ 测试全部通过
  - 单元测试: 42 passed
  - 集成测试: 238 passed

go test: 所有模块通过
  - pole/chain/app: ok
  - pole/chain/x/pole/keeper: ok
  - pole/chain/x/pole/types: ok
```

---

## 文档

| 文档 | 说明 |
|------|------|
| `docs_PoLE_Whitepaper.md` | 正式版白皮书 |
| `IMPLEMENTATION_PLAN.md` | 实施计划 |
| `TRACEABILITY.md` | 白皮书到代码的映射 |
| `docs/operations/install.md` | 安装指南 |
| `docs/operations/service-management.md` | 服务管理 |
| `docs/operations/troubleshooting.md` | 故障排查 |

---

## 已知限制

1. **Rust-to-Cosmos 桥接:** 本版本尚未实现将 Rust 原型生成的 artifact 直接提交到 Cosmos 链的功能
2. **永久归档:** PoLE 不保证在挑战窗口后永久保留所有原始数据
3. **跨链:** V1 不包含跨链桥功能

---

## 安全说明

- 本软件按"原样"提供，不提供任何明示或暗示的保证
- 建议在正式运行前进行完整的安全审计
- 参与奖励前请充分了解 PoLE 协议机制和风险

---

## 后续计划

- V1.1: 实现 Rust 到 Cosmos 的桥接
- V1.2: 完成端到端集成测试
- V2.0: 增强数据可用性证明和更多信号源支持

---

**PoLE Team**
2026 年 4 月 26 日