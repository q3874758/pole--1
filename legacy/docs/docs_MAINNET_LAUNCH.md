# PoLE 主网上线操作指南

本文档说明如何上线 PoLE 主网（pole-mainnet-1）。文档索引见 [docs_INDEX.md](docs_INDEX.md)。

---

## 一、上线前准备

### 1. 环境要求

- Go 1.19+
- 创世文件：`config/mainnet/genesis.json`
- 数据目录：建议独立磁盘，如 `data/mainnet`

### 2. 创世文件

主网创世文件路径：`config/mainnet/genesis.json`。

- **Chain ID**：`pole-mainnet-1`
- **代币分配**：按白皮书 60% 节点奖励池、20% 生态、15% 社区、5% 团队与投资人
- **创世验证者**：在 `validators` 中配置初始验证者地址、公钥、质押量

若文件不存在，可复制示例并修改：

```bash
mkdir -p config/mainnet
# 使用项目中的 config/mainnet/genesis.json 或通过 cmd/genesis 生成
```

### 3. Bootnode（可选）

主网至少提供一个 bootnode，供新节点发现网络。格式示例：

```
/ip4/公网IP/tcp/26656/p2p/节点ID
```

多个 bootnode 用逗号分隔，启动时通过 `--bootnodes` 传入。

---

## 二、正式启动主网节点

### 快速启动（Windows）

- **双击**：运行项目根目录下的 `run-mainnet.bat`，会自动编译并启动主网节点，并打开钱包页面。
- **命令行**：在项目根目录执行  
  `.\scripts\run.ps1 -Profile mainnet -OpenBrowser`  
  或仅启动不打开浏览器：  
  `.\scripts\run.ps1 -Profile mainnet`

### 方式一：命令行（推荐）

```bash
# 进入项目根目录
cd d:\PoLE

# 主网启动（使用 config/mainnet/genesis.json）
go run ./cmd/node/main.go --network mainnet

# 指定数据目录
go run ./cmd/node/main.go --network mainnet --data-dir data/mainnet

# 完整参数示例
go run ./cmd/node/main.go \
  --network mainnet \
  --genesis config/mainnet/genesis.json \
  --data-dir data/mainnet \
  --rpc-port :9090 \
  --p2p-port :26656 \
  --prometheus-port :9091 \
  --tls \
  --bootnodes "ip1:26656,ip2:26656" \
  --log-level info
```

### 方式二：编译后运行

```bash
# 编译
go build -o pole-node.exe ./cmd/node

# 主网启动
./pole-node.exe --network mainnet --data-dir data/mainnet
```

### 方式三：脚本（Linux/macOS）

```bash
chmod +x scripts/start-mainnet.sh
./scripts/start-mainnet.sh
```

可通过环境变量覆盖默认配置：

- `POLE_DATA_DIR`：数据目录
- `POLE_GENESIS_FILE`：创世文件路径
- `POLE_RPC_PORT`：RPC 端口
- `POLE_P2P_PORT`：P2P 端口
- `POLE_CHAIN_ID`：Chain ID（通常不需改）

### 挖矿模式（Play-to-Earn）

启用后节点会自动采集游戏数据、参与 P2P 确认，并从创世挖矿池向用户发放链上奖励。

- **启动器**：`.\scripts\run.ps1 -Profile mainnet -Mining` 或 `.\scripts\run.ps1 -Profile mainnet -Mining -OpenBrowser`；或双击 **run-mainnet-mining.bat**（主网 + 挖矿 + 打开钱包）。
- **命令行**：`.\pole-node.exe --network mainnet --data-dir data\mainnet --mining`
- **RPC**：`GET /mining/status`、`GET /mining/balance?address=...`、`POST /mining/claim`（Body: `{"address":"..."}`）
- **自动发放**：节点每 5 分钟对「待领且已满 3 确认」的地址执行一次链上转账，无需用户主动领取；也可随时通过 RPC 或钱包「领取挖矿奖励」手动领取。
- **只要是游戏就可以挖矿**：默认列表 + 进程检测到的运行中游戏都会参与采集；若某游戏无法从 Steam 等接口取数（非 Steam 或接口失败），仍会生成通用数据点并参与奖励，按 Tier3 计。
- 待领奖励会持久化到 `data-dir/mining_rewards.json`，重启不丢。

---

## 三、启动后验证

### 1. 控制台输出

正常启动会看到类似输出：

```
========================================
  PoLE Mainnet Node
========================================

[1] 加载创世配置...
    ChainID: pole-mainnet-1
    代币: Proof of Live Engagement (POLE)
[2] 初始化链状态...
    创世初始化完成
...
[11] 启动 RPC 服务器...
    RPC: http://localhost:9090/
========================================
  节点初始化完成!
  网络: 主网 (pole-mainnet-1)
========================================
```

### 2. 接口检查

| 用途       | 地址                          |
|------------|-------------------------------|
| RPC / 钱包 | http://localhost:9090/        |
| 健康检查   | http://localhost:9091/health  |
| Prometheus | http://localhost:9091/metrics |
| P2P API    | http://localhost:26660/       |

```bash
# 链状态
curl http://localhost:9090/status

# 健康检查
curl http://localhost:9091/health

# 验证者列表
curl http://localhost:9090/validators
```

### 3. 钱包与节点监控

- 浏览器访问：`http://localhost:9090/` 使用钱包 UI
- 节点监控页：`http://localhost:9090/dashboard.html`（若已部署）

---

## 四、主网与测试网切换

| 参数              | 主网                    | 测试网                     |
|-------------------|-------------------------|----------------------------|
| `--network`       | `mainnet`               | `testnet`                  |
| 默认创世路径      | config/mainnet/genesis.json | config/testnet/genesis.json |
| 默认 Chain ID    | pole-mainnet-1          | pole-testnet-1             |
| 默认数据目录建议  | data/mainnet            | data/testnet               |

**注意**：主网与测试网应使用**不同数据目录**，避免状态混淆。

```bash
# 测试网
go run ./cmd/node/main.go --network testnet --data-dir data/testnet

# 主网
go run ./cmd/node/main.go --network mainnet --data-dir data/mainnet
```

---

## 五、多节点与 Bootnode

1. **首节点**：按上文启动，不填 `--bootnodes`。
2. **获取首节点 P2P 地址**：从首节点或文档获取 `ip:26656` 或完整 p2p 地址。
3. **其他节点**：启动时加上 `--bootnodes`，例如：
   ```bash
   go run ./cmd/node/main.go --network mainnet --bootnodes "192.168.1.100:26656"
   ```

---

## 六、上线清单速查

- [x] 创世文件已准备并校验（`config/mainnet/genesis.json`）
- [x] Chain ID 为 `pole-mainnet-1`
- [x] 数据目录已规划（如 `data/mainnet`）
- [x] 至少一个节点先启动成功
- [x] RPC、健康检查、Prometheus 可访问
- [ ] （可选）Bootnode 已公布，其他节点可连接
- [ ] （可选）TLS 已配置：`--tls`
- [x] （可选）监控与告警已配置（Prometheus + 告警规则）

---

## 七、常见问题

**Q: 创世文件不存在怎么办？**  
A: 确保 `config/mainnet/genesis.json` 存在；若只有 `config/genesis.json`，可复制到 `config/mainnet/` 并将其中 `chain_id` 改为 `pole-mainnet-1`。

**Q: 端口被占用？**  
A: 用 `--rpc-port`、`--p2p-port`、`--prometheus-port` 指定其他端口。

**Q: 如何确认是主网？**  
A: 看启动日志里的 `ChainID: pole-mainnet-1`，或请求 `curl http://localhost:9090/status` 查看返回的 `chain_id`。

**Q: 主网数据存在哪？**  
A: 在 `--data-dir` 指定目录下（如 `data/mainnet`），状态保存在该目录的 `state.json` 等文件中。

---

## 八、主网上线清单与能力

按白皮书 **Phase 2：主网 Alpha（2026 年 Q4）** 目标整理：启动主网并发行 $POLE、开放验证节点质押、数据采集与 GVS、治理框架。

### 8.1 已有能力（可直接用）

| 模块 | 说明 |
|------|------|
| **代币合约** | `contracts/token.go`：SRC-20 风格，Transfer/Approve/Mint/Burn/快照/创世初始化 |
| **质押合约** | `contracts/staking.go`：验证者注册与委托、Slash/Unjail、奖励分发与领取 |
| **治理合约** | `contracts/governance.go`：提案、投票、统计、执行标记 |
| **创世配置** | `contracts/genesis.go`：白皮书 60/20/15/5 分配、JSON 读写、ApplyGenesis |
| **创世生成** | `cmd/genesis`、`scripts/init-genesis.ps1`，输出 `config/genesis.json` |
| **启动脚本** | `scripts/start-node.ps1`、`scripts/run.ps1`，run.bat / run-mainnet.bat / run-mainnet-mining.bat |
| **共识** | `core/consensus`、`core/tendermint` |
| **数据采集与 GVS** | `data/collector`、`execution/engine`（GVS 计算） |
| **挖矿/Play-to-Earn** | `--mining` 自动采集、P2P 确认、创世挖矿池链上发放；RPC `/mining/status`、`/mining/balance`、`/mining/claim`；持久化 `mining_rewards.json`；奖励每 5 分钟自动发放 |
| **代币经济与奖励** | `token/economy`、`token/rewards` |
| **治理逻辑** | `governance/`（与链上治理合约可对接） |
| **节点程序** | `cmd/node`（独立节点）、`cmd/core`（核心模块演示） |

### 8.2 主网上线还需要的部分

- **链启动与创世/合约集成（必须）** ✅ 已实现：加载创世、初始化 Token/Staking/Governance、`cmd/node` 全节点入口。
- **链上执行与状态（必须）** ✅ 已实现：交易→合约映射、状态持久化、创世 chain_id。
- **测试网/主网区分（必须）**：Chain ID、创世文件与 Bootnode、文档（见本文与测试说明）。
- **RPC / API（必须）**：当前提供 REST 查询与交易接口，供钱包/CLI 使用。
- **运维与监控（建议）**：日志、Prometheus 指标、告警（见本文第三节）。
- **安全与审计（建议）**：主网前合约与关键路径审计、密钥管理、升级与 Slash 可追溯。
- **文档与发布（建议）**：主网公告、节点运行指南（本文）、治理与代币说明（白皮书）。

### 8.3 建议实现顺序

1. 创世与合约接入启动流程 → 2. 状态与执行 → 3. 共识与创世一致 → 4. RPC 接口 → 5. 测试网验证 → 6. 主网准备（创世、bootnode、文档与监控）。

### 8.4 参考

- 白皮书：`docs_PoLE_Whitepaper.md`（代币分配、Phase 2 目标、治理阶段）
- 创世与脚本：`scripts/README.md`、`cmd/genesis`、`config/genesis.json`
- 合约与创世类型：`contracts/`（token、staking、governance、genesis）

---

更多细节见白皮书 `docs_PoLE_Whitepaper.md`。文档索引见 `docs_INDEX.md`。
