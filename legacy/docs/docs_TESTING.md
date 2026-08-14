# PoLE 测试说明

本文侧重**测试流程、锁仓验证、E2E 脚本**。运行方式概览见 [README](README_CN.md)、[文档索引](docs_INDEX.md)；主网启动与清单见 [主网上线操作指南](docs_MAINNET_LAUNCH.md)。

---

## 一、环境准备

- 已安装 Go，项目可编译（见下方）。
- 当前钱包地址为团队地址：`dfdb4bdd50f5fa6c499461c78bfc69aa645a281e`（与 `wallet.json` 一致）。

## 二、如何直观知道节点在运行

- **钱包页面**：打开 http://localhost:9090（或节点 RPC 地址），看右上角：
  - **绿点 +「节点运行中」**：节点正常，后面会显示区块高度和链 ID。
  - **红点 +「节点未连接」**：连不上节点，请确认节点已启动且端口正确。
  - 首次加载会显示「检查节点…」和黄点，几秒内会变为上述两种之一。
- **命令行**：在项目根目录执行  
  `.\scripts\check_node.ps1`  
  输出 **Node is RUNNING** 并打印 chain_id、height 即表示节点在运行；否则会提示不可达。

## 三、软件运行程序（启动器）

提供统一入口，选网络、自动编译并启动节点，可选自动打开钱包页面。

### 方式一：双击运行（推荐）

- **run.bat**：默认以 **Test（锁仓测试）** 配置启动节点，约 8 秒后自动打开钱包页面。双击即可。
- **run-mainnet.bat**：以 **Mainnet** 配置启动并打开钱包。
- **run-mainnet-mining.bat**：以 **Mainnet + 挖矿** 配置启动并打开钱包（Play-to-Earn 自动采集与奖励发放）。

运行前请确保已安装 Go，且本机可执行 `go build`。

### 方式二：命令行启动器

在项目根目录执行：

```powershell
.\scripts\run.ps1
```

会进入**交互菜单**，选择 1=Mainnet / 2=Testnet / 3=Test，再启动节点。

带参数示例：

```powershell
.\scripts\run.ps1 -Profile test -OpenBrowser    # Test 配置并打开浏览器
.\scripts\run.ps1 -Profile mainnet             # 仅启动主网节点
.\scripts\run.ps1 -Profile test -Background    # 后台启动，不占当前窗口
.\scripts\run.ps1 -Profile test -NoBuild      # 不重新编译，直接运行已有 pole-node.exe
.\scripts\run.ps1 -Profile test -Mining -OpenBrowser   # 启用挖矿并打开钱包
```

| 参数 | 说明 |
|------|------|
| `-Profile mainnet\|testnet\|test` | 网络配置：主网 / 测试网 / 锁仓测试 |
| `-Mining` | 启用挖矿（Play-to-Earn：自动采集游戏数据、链上发放奖励；奖励每 5 分钟自动发放，也可在钱包或 RPC 手动领取） |
| `-OpenBrowser` | 启动后约 8 秒打开 http://localhost:9090 |
| `-Background` | 后台运行，不阻塞当前终端 |
| `-NoBuild` | 跳过编译，使用已有 pole-node.exe |
| `-DataDir <路径>` | 覆盖默认数据目录 |
| `-RpcPort <端口>` | RPC 端口，默认 9090 |

### 运行流程简述

1. 选择或指定 Profile → 2. 检查创世文件 → 3. 编译节点（除非 `-NoBuild`）→ 4. 启动 `pole-node.exe`（或后台）→ 5. 若 `-OpenBrowser` 则延时打开钱包。

## 四、编译

```powershell
cd d:\PoLE
go build -o pole-node.exe ./cmd/node
```

若全量编译报错，可只编译节点与依赖：

```powershell
go build -o pole-node.exe ./cmd/node/...
```

## 五、测试锁仓领取（推荐流程）

使用**测试创世**，锁仓期为 0，启动后即可在钱包里看到可领取并点击领取。

### 1. 清空数据目录（首次测试必须）

确保用测试创世重新初始化，避免沿用旧状态：

```powershell
Remove-Item -Recurse -Force data\vesting_test -ErrorAction SilentlyContinue
```

### 2. 启动节点（使用测试创世 + 独立数据目录）

```powershell
.\pole-node.exe --genesis config\genesis_vesting_test.json --data-dir data\vesting_test
```

或若用 `go run`：

```powershell
go run ./cmd/node --genesis config/genesis_vesting_test.json --data-dir data/vesting_test
```

节点会：

- 使用创世 `config/genesis_vesting_test.json`（链 ID：`pole-test-vesting`）
- 团队 3% 进入锁仓池，受益人为主网团队地址，**锁仓 0 月、24 月线性释放**
- 因 `genesis_time=0`、`lock_months=0`，锁仓已视为结束，按整月计算会有可领取额

### 3. 打开钱包

浏览器访问：

```
https://localhost:9090
```

（若未开 TLS，可先尝试 `http://localhost:9090`，需与节点实际配置一致。）

- 钱包会拉取当前节点上的账户（即本机钱包），并显示余额与「锁仓领取」区块。

### 4. 在钱包里验证锁仓与领取

1. **看锁仓状态**  
   在「锁仓领取」卡片中应看到：
   - 锁仓总量（约 30,000,000 POLE）
   - 已领取、可领取
   - 锁仓期为 0 时，应显示有可领取金额

2. **点击「领取已解锁」**  
   - 若可领取 > 0，按钮可点；点击后调用 `POST /vesting/claim`
   - 成功后提示「已领取 X POLE」，总余额增加，可领取减少

3. **再次领取**  
   - 测试创世下可能一次就领完 24 个月累计；再点会提示无可用领取或按钮禁用

### 5. 用接口直接测（可选）

- 查询锁仓状态（将 `address` 换成你的团队地址）：

```powershell
curl "http://localhost:9090/vesting/status?address=dfdb4bdd50f5fa6c499461c78bfc69aa645a281e"
```

- 领取（同上，确保钱包/节点使用该地址）：

```powershell
curl -X POST http://localhost:9090/vesting/claim -H "Content-Type: application/json" -d "{\"address\":\"dfdb4bdd50f5fa6c499461c78bfc69aa645a281e\"}"
```

返回中应包含 `"claimed": "..."`。

## 六、测试主网创世（仅验证 UI/接口，不能领）

主网创世 `config/mainnet/genesis.json` 中团队锁仓为 **12 个月**，锁仓期内可领取为 0：

1. 清空数据并改用主网创世启动：

```powershell
Remove-Item -Recurse -Force data\mainnet_test -ErrorAction SilentlyContinue
.\pole-node.exe --network mainnet --genesis config\mainnet\genesis.json --data-dir data\mainnet_test
```

2. 打开钱包，在「锁仓领取」中应看到：
   - 锁仓总量、已领取、可领取（应为 0）
   - 解锁时间约为创世时间 + 12 个月
   - 「领取已解锁」为灰色不可点

用于确认：主网配置下锁仓期内无法领取，仅锁仓期满后才能在钱包领取。

## 七、防止钱包地址丢失

钱包文件（含地址与私钥）保存在节点工作目录下的 **wallet.json**。丢失则无法找回资产，请务必做好备份。

### 1. 在钱包页面备份（推荐）

- 打开钱包后，在「我的地址」下方点击 **「📥 备份钱包（防丢失）」**。
- 浏览器会下载一份带日期的 JSON 文件（如 `pole-wallet-backup-2025-02-14.json`），**内含私钥**。
- 将文件保存到 U 盘、加密盘或可信云盘，**不要分享给任何人**。

### 2. 手动复制 wallet.json

- 节点运行目录下的 **wallet.json** 即完整钱包。
- 定期复制到安全位置（另一块盘、加密备份等），重装系统或换机后可将该文件放回节点目录并重启节点即可恢复。

### 3. 恢复钱包

- 将备份的 JSON 文件重命名为 **wallet.json**，放到节点程序的工作目录（与 pole-node.exe 同级），然后重启节点。
- 或在未启动节点时，用备份文件覆盖原 wallet.json 再启动节点。

### 4. 安全提示

- 备份文件 = 私钥，泄露即资产可被转走；勿上传公开网盘、勿通过聊天工具发送。
- 建议多处备份（如本地 + 离线 U 盘），防止单点损坏或丢失。

## 八、常见问题

| 现象 | 处理 |
|------|------|
| 钱包显示「暂无锁仓计划」 | 确认当前钱包地址为创世中配置的团队受益人地址（如 `dfdb4bdd50f5fa6c499461c78bfc69aa645a281e`），且节点用的是带 `vesting` 的创世。 |
| 领取后余额没变 | 确认节点 RPC 正常、未报错；刷新页面或再次查询余额。 |
| 想重新测一遍领取 | 删掉 `data\vesting_test`，重新用 `config\genesis_vesting_test.json` 启动，会重新创世，可再次领取。 |

总结：**锁仓期满以后可以在钱包领取**；用 `config/genesis_vesting_test.json` + 独立数据目录可快速验证整条「查询 → 领取 → 余额更新」流程。

---

## 九、测试整个系统（自动化脚本）

脚本 `scripts\test_system.ps1` 会：编译节点 → 清空测试数据 → 启动节点 → 等待 RPC 就绪 → 校验 `/status`、`/health`、`/metrics`、`/vesting/status`、`/vesting/claim`、`/wallet/accounts` → 停止节点，并输出全部通过或失败项数。

### 方式一：全自动（脚本负责启动节点）

在项目根目录执行：

```powershell
cd d:\PoLE
.\scripts\test_system.ps1
```

- 若本机 9090 端口未被占用且节点能正常启动，约 1 分钟内会跑完并显示 **All passed**。
- 若出现 **RPC not ready**，可改用方式二。

### 方式二：先启节点，再只跑校验（推荐）

1. **终端 1**：清空测试数据并启动节点（测试创世）

```powershell
cd d:\PoLE
Remove-Item -Recurse -Force data\e2e_test -ErrorAction SilentlyContinue
.\pole-node.exe --genesis config\genesis_vesting_test.json --data-dir data\e2e_test --network testnet
```

2. 等待控制台出现 `RPC: http://localhost:9090/` 或 `节点初始化完成`（约 10–15 秒）。

3. **终端 2**：只跑接口与业务校验（不编译、不启停节点）

```powershell
cd d:\PoLE
.\scripts\test_system.ps1 -ChecksOnly
```

应输出一列 `GET /status OK`、`GET /health OK`、`GET /metrics OK`、`GET /vesting/status OK`、`POST /vesting/claim OK`、`GET /wallet/accounts OK`，最后 **All passed**。

4. 回到终端 1 按 `Ctrl+C` 停止节点。

### 校验项说明

| 步骤 | 说明 |
|------|------|
| GET /status | 链 ID、区块高度 |
| GET /health | 健康（height=0 时可能返回 503，脚本会按 503 视为可接受） |
| GET /metrics | Prometheus 文本含 `pole_block_height` |
| GET /vesting/status | 团队地址锁仓计划与可领取额 |
| POST /vesting/claim | 领取一次，并写回状态 |
| GET /wallet/accounts | 钱包账户列表 |
| GET /mining/status | 挖矿状态（未启用时也可返回成功） |
| GET /mining/balance | 指定地址挖矿待领余额（未启用时 pending=0） |
