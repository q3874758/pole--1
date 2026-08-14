# PoLE 主网

> 去中心化 Layer 1 区块链 - 游戏玩家价值证明

## 🚀 快速开始

### 运行主网节点

```bash
# Linux/macOS
./scripts/start-mainnet.sh

# Windows
.\scripts\start-mainnet.bat
```

### Docker

```bash
docker run -d -p 30303:30303 -p 8545:8545 -v pole-data:/data pole/node:mainnet
```

## 🔧 配置

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `--datadir` | `./data` | 数据目录 |
| `--http.port` | `8545` | HTTP RPC 端口 |
| `--ws.port` | `8546` | WebSocket 端口 |
| `--maxpeers` | `50` | 最大对等节点数 |
| `--bootnodes` | - | 引导节点 |

## 🌐 网络端点

- **HTTP RPC**: http://localhost:8545
- **WebSocket**: ws://localhost:8546
- **P2P**: /ip4/0.0.0.0/tcp/30303

## 📡 API

### 标准 Ethereum JSON-RPC

```bash
# 查询区块
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

### PoLE 特有方法

```bash
# 获取 GVS 分数
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"pole_getGVS","params":["game_id"],"id":1}'

# 提交游戏数据
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"pole_submitGameData","params":["game_id","data"],"id":1}'

# 质押
curl -X POST http://localhost:8545 \
  -H "Content-Type: application/json" \
  -d '{"jsonrpc":"2.0","method":"pole_stake","params":["1000000"],"id":1}'
```

## 🔐 密钥管理

```bash
# 生成新钱包
./pole-node account new

# 导入私钥
./pole-node account import <keyfile>
```

## ⛓️ 主网参数

| 参数 | 值 |
|------|-----|
| ChainID | 1 |
| NetworkID | 1 |
| 代币符号 | POLE |
| 最小质押 | 10000 POLE |
| 出块时间 | 3 秒 |
| 最大验证者 | 21 |

## 📱 钱包

- **Web 钱包**: http://localhost:8545/wallet
- **CLI 钱包**: `./pole-node wallet`

## 🤝 加入主网

1. 运行节点
2. 等待同步完成
3. 质押 POLE 成为验证者

引导节点:
```
/ip4/<你的IP>/tcp/30303/p2p/<peer-id>
```

## 📄 许可证

MIT License
