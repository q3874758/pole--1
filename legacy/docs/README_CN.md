# PoLE (Proof of Live Engagement)

一个去中心化的 Layer 1 区块链项目，将 PC 游戏中的真实玩家参与转化为可量化的链上价值。

## 项目概述

PoLE 是一个创新的 Layer 1 区块链项目，通过玩家运行的分布式节点网络，采集游戏在线数据（以 Steam 公开 API 为核心），计算 Game Value Score (GVS)，并以原生代币 $POLE 形式奖励参与者。这一机制实现了"玩游戏即挖矿"的全新经济模型。

## 项目架构

```
PoLE/
├── core/                    # 核心区块链模块
│   ├── types/              # 核心数据类型
│   └── consensus/          # PoS + PoLE 共识机制
├── data/                   # 数据层
│   ├── collector/          # 游戏数据采集
│   └── availability/      # 数据可用性层
├── execution/              # 执行层
│   └── engine/            # GVS 计算引擎
├── token/                  # 代币经济
│   ├── economy/            # 通胀、燃烧机制
│   └── rewards/           # 奖励分发
├── governance/            # 链上治理
└── network/               # P2P 网络
    └── p2p/               # libp2p 实现
```

## 核心特性

- **混合共识**: PoS（权益证明）+ PoLE（活跃度证明）
- **GVS 计算**: 基于玩家参与度的游戏价值评分
- **三层分级**: Tier 1/2/3 游戏分类系统
- **代币经济**: 通胀、燃烧、奖励分发机制
- **链上治理**: DAO 风格提案和投票
- **P2P 网络**: 基于 libp2p 的去中心化网络

## 构建项目

```bash
# 克隆仓库
git clone https://github.com/pole-chain/pole.git
cd pole

# 构建节点（需 Go 1.19+）
go build -o pole-node.exe ./cmd/node

# 运行测试
go test ./...
```

## 运行节点（Windows）

- **run.bat**：测试网/锁仓测试，自动打开钱包。
- **run-mainnet.bat**：主网节点。
- **run-mainnet-mining.bat**：主网 + 挖矿（Play-to-Earn 自动采集与奖励发放）。
- 命令行：`.\scripts\run.ps1 -Profile mainnet -Mining -OpenBrowser`。详见 [测试说明](./docs_TESTING.md)、[主网启动](./docs_MAINNET_LAUNCH.md)。

## 文档

- [文档索引](./docs_INDEX.md)（按用途选读）
- [白皮书](./docs_PoLE_Whitepaper.md)
- [技术规格](./PoLE_Technical_Specification.md)
- [测试说明](./docs_TESTING.md) · [主网启动](./docs_MAINNET_LAUNCH.md)

## 路线图

| 阶段 | 时间 | 描述 |
|------|------|------|
| Phase 0 | 2026 Q1 | 概念验证 |
| Phase 1 | 2026 Q2-Q3 | 测试网上线 |
| Phase 2 | 2026 Q4 | 主网 Alpha |
| Phase 3 | 2027+ | 生态扩展 |

## 代币经济学

- **总供应量**: 10 亿枚 $POLE
- **初始通胀率**: 20% 每年
- **代币分配**:
  - 节点奖励: 60%
  - 生态基金: 20%
  - 社区激励: 15%
  - 团队: 5%

## 贡献

欢迎贡献代码！请在提交 PR 前阅读贡献指南。

## 许可证

MIT 许可证 - 详见 LICENSE 文件。
