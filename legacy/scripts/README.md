# PoLE 脚本说明

## 主网/测试网启动

1. **生成创世文件**（首次或重置时）  
   ```powershell
   .\scripts\init-genesis.ps1
   ```  
   输出：`config/genesis.json`（含白皮书代币分配：60% 节点奖励池、20% 生态、15% 社区、5% 团队与投资人）

2. **启动节点**  
   ```powershell
   .\scripts\start-node.ps1
   ```  
   会先检查是否存在 `config/genesis.json`。

3. **统一启动器（推荐）**  
   - `.\scripts\run.ps1`：交互菜单选 Mainnet / Testnet / Test，支持 `-Profile mainnet|testnet|test`、`-Mining`、`-OpenBrowser`。  
   - 项目根目录 **run.bat** / **run-mainnet.bat** / **run-mainnet-mining.bat** 会调用本脚本；挖矿模式即 Play-to-Earn 自动采集与奖励发放。

## 仅生成创世（指定路径）

```bash
go run ./cmd/genesis [输出路径]
# 默认: config/genesis.json
```

## 合约与创世

- 代币/质押/治理合约：`contracts/`（token、staking、governance、genesis）
- 创世配置类型与默认比例：`contracts/genesis.go`（可 JSON 序列化后给链上或 CLI 使用）
