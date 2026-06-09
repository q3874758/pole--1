# PoLE 用 aardio 重写方案评估

> 评估日期：2026-04-18  
> 评估结论：**可行，推荐「混合方案」—— Rust 保留核心计算，aardio 接管桥接层 + 生产基础设施**

---

## 一、核心发现

### 1.1 ed25519 签名：✅ 无阻碍

aardio 通过 **BouncyCastle 扩展库**（自动安装，基于 .NET）提供完整的 ed25519 支持：

| 功能 | 验证结果 |
|------|---------|
| 密钥生成 | ✅ 32 字节私钥 + 32 字节公钥 |
| 签名 (Sign) | ✅ 64 字节签名 |
| 验签 (Verify) | ✅ 通过 |
| GetEncoded() | ✅ 正确导出原始密钥字节 |

```aardio
import BouncyCastle;
var keyPair = BouncyCastle.Crypto.Generators.Ed25519KeyPairGenerator();
keyPair.Init(BouncyCastle.Crypto.KeyGenerationParameters(
    BouncyCastle.Security.SecureRandom(), 256
));
var keys = keyPair.GenerateKeyPair();
var privateKey = keys.Private;  // Ed25519PrivateKeyParameters
var publicKey = keys.Public;    // Ed25519PublicKeyParameters
var privBytes = privateKey.GetEncoded();  // 32 字节
var pubBytes = publicKey.GetEncoded();    // 32 字节

// 签名
var signer = BouncyCastle.Crypto.Signers.Ed25519Signer();
signer.Init(true, privateKey);
signer.BlockUpdate(raw.buffer(payload), 0, #payload);
var signature = signer.GenerateSignature(); // 64 字节
```

> 对比 Rust 原型：`signing.rs` 中的 ed25519 是**占位实现**（用 SHA256 模拟，非真实曲线运算）。aardio 使用 BouncyCastle 提供的是**工业级 Ed25519**，比 Rust 原型更可靠。

### 1.2 HTTP / REST 通信：✅ 无阻碍

```aardio
import web.rest.jsonClient;
var http = web.rest.jsonClient();

// Cosmos SDK REST API (端口 1317)
var nodeInfo = http.get("http://localhost:1317/cosmos/base/tendermint/v1beta1/node_info");

// 广播交易
var result = http.api("http://localhost:1317/").cosmos.tx.v1beta1.txs.post({
    tx_bytes = base64EncodedTx,
    mode = "BROADCAST_MODE_SYNC"
});
```

### 1.3 SHA256 / Merkle 树：✅ 原生支持

```aardio
import crypt;
var hash = crypt.sha256(data, "raw");  // 返回 32 字节二进制哈希
```

### 1.4 bech32 地址编码：⚠️ 需自实现

Cosmos 使用 bech32 地址（如 `pole1abc...`）。aardio 没有现成的 bech32 库，但 bech32 算法简单（约 100 行纯算法），可直接用 aardio 实现。

> 也可以调用 `dotNet.NBitcoin`（需额外安装 NuGet 包）或直接用 golang 扩展库的 bech32 实现。

### 1.5 Protobuf 序列化：⚠️ 需绕行

Cosmos SDK 交易最终需要 protobuf 二进制编码。aardio 没有 protobuf 库，但有两个绕行方案：

| 方案 | 描述 | 推荐度 |
|------|------|--------|
| **REST API（推荐）** | Cosmos SDK 提供 JSON REST 接口（端口 1317），无需 protobuf | ⭐⭐⭐⭐⭐ |
| golang 桥接 | 用 `golang` 扩展库调用 Go 的 protobuf 编解码 | ⭐⭐⭐ |
| 手动序列化 | protobuf 编码规则简单，可手写编码器（仅需支持 6 种消息类型） | ⭐⭐ |

> **REST API 是最佳选择**：Cosmos SDK v0.46+ 对 REST 支持完善，`/cosmos/tx/v1beta1/txs` 接受 JSON 格式交易直接广播。即使需要 protobuf，交易体本身也是 Base64 编码的 protobuf 二进制，可从 Rust 原型获取。

### 1.6 P2P 网络：✅ 可用

```aardio
import wsock.tcp.server;
var server = wsock.tcp.server("0.0.0.0", 26656);
```

但 P2P 协议（gossip、peer exchange、区块同步）实现量大。建议保留 Rust 的 P2P 层或使用 libp2p。

---

## 二、方案对比

### 方案 A：完全用 aardio 重写（不推荐）

**工作量**：12-16 周（2 人）  
**风险**：高

| 模块 | 原有 Rust 规模 | aardio 重写难度 |
|------|--------------|----------------|
| 数据采集 (activity/steam collector) | ~2000 行 | 简单 |
| Merkle 树 | ~500 行 | 中等 |
| 批次/聚合 (node_pipeline/aggregator) | ~3000 行 | 中等 |
| 奖励计算 (node_rewards) | ~4000 行 | 复杂 |
| 挑战验证 (node_verifier) | ~2000 行 | 复杂 |
| P2P 网络 | ~1500 行 | 困难 |
| 签名 (已解决) | ~300 行 | 简单 |
| 状态管理 (state/transitions) | ~3000 行 | 中等 |

**优点**：统一技术栈，部署简单（单个 EXE）  
**缺点**：重写量大，放弃已验证的 Rust 代码，P2P 实现困难

---

### 方案 B：aardio 仅写桥接层（推荐）

**工作量**：3-4 周（1 人）  
**风险**：低

```
┌─────────────────────────────────────────────────┐
│                  Rust 原型层                      │
│  ┌──────────┐ ┌──────────┐ ┌──────────────────┐ │
│  │ 采集器    │ │ 批次构建  │ │ 聚合/奖励/挑战    │ │
│  │Collector │ │ Pipeline │ │ Aggregator/Reward│ │
│  └──────────┘ └──────────┘ └──────────────────┘ │
│                      │                           │
│               epoch_artifacts.json                │
│                      │                           │
├──────────────────────┼───────────────────────────┤
│                  aardio 桥接层                    │
│  ┌───────────────────────────────────────────┐  │
│  │ 🔑 TxSigner    → ed25519 签名 (BouncyCastle)│  │
│  │ 📡 RpcClient   → REST API 广播到链         │  │
│  │ 🔗 ChainBridge → 6 种交易构造 + 签名        │  │
│  │ 🖥 GUI         → 桌面界面 (winform)        │  │
│  │ 📦 Installer   → MSI / 系统服务            │  │
│  │ 🔄 AutoUpdate  → 自动更新                  │  │
│  └───────────────────────────────────────────┘  │
│                      │                           │
│              REST API (port 1317)                 │
│                      │                           │
├──────────────────────┼───────────────────────────┤
│               Cosmos SDK 链 (Go)                  │
│  ┌──────────────────────────────────────────┐   │
│  │ MsgSubmitBatch / MsgCommitEpoch           │   │
│  │ MsgOpenChallenge / MsgFinalizeEpoch       │   │
│  │ MsgClaimReward / MsgVote                  │   │
│  └──────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
```

**优点**：
- 保留 Rust 已验证的核心计算（305+ 测试全绿）
- aardio 擅长 Glue Code：签名、HTTP、GUI、安装器
- 工作量最小（3-4 周 vs 12-16 周）
- Rust 通过 CLI / JSON 输出 epoch artifacts，aardio 读取后签名广播

**缺点**：需要维护两种语言（但各自职责清晰，互不干扰）

---

### 方案 C：混合方案（最佳）

**工作量**：5-6 周（1 人）  
**风险**：低-中

在方案 B 基础上，逐步将 Rust 中不适合的部分迁移到 aardio：

| 阶段 | 内容 | 工期 |
|------|------|------|
| Phase 1 | aardio 桥接层（TxSigner + RpcClient + GUI） | 2-3 周 |
| Phase 2 | 用 aardio 重写采集器（Steam/Epic 等 HTTP 采集） | 1 周 |
| Phase 3 | 用 aardio 重写轻量模块（参数管理、配置） | 1 周 |
| Phase 4 | 保留 Rust 的核心：Merkle、奖励计算、挑战验证 | 不变 |

---

## 三、推荐方案：混合方案详情

### 3.1 架构图

```
┌──────────────────────────────────────────────────────┐
│                   aardio 进程                          │
│                                                        │
│  ┌─────────────────┐  ┌──────────────────────────┐   │
│  │   GUI 界面       │  │   系统服务 / 后台守护     │   │
│  │  (winform)       │  │   (Windows Service)      │   │
│  └─────────────────┘  └──────────────────────────┘   │
│                                                        │
│  ┌─────────────────────────────────────────────────┐ │
│  │              aardio 桥接引擎                      │ │
│  │                                                   │ │
│  │  ┌──────────────┐  ┌────────────────────────┐   │ │
│  │  │ 数据采集器    │  │  TxSigner (ed25519)     │   │ │
│  │  │ (HTTP/Steam) │  │  BouncyCastle .NET      │   │ │
│  │  └──────────────┘  └────────────────────────┘   │ │
│  │                                                   │ │
│  │  ┌──────────────┐  ┌────────────────────────┐   │ │
│  │  │ 批次构建器    │  │  RpcClient (REST API)   │   │ │
│  │  │ (轻量)       │  │  → Cosmos 链            │   │ │
│  │  └──────────────┘  └────────────────────────┘   │ │
│  │                                                   │ │
│  │  ┌──────────────────────────────────────────┐   │ │
│  │  │  Rust 核心调用器                           │   │ │
│  │  │  → 调用 Rust DLL 或 CLI 执行：             │   │ │
│  │  │     Merkle 树 / 聚合 / 奖励计算 / 挑战验证 │   │ │
│  │  └──────────────────────────────────────────┘   │ │
│  └─────────────────────────────────────────────────┘ │
│                                                        │
│  ┌─────────────────────────────────────────────────┐ │
│  │              Cosmos 链交互层                      │ │
│  │  REST API: http://localhost:1317/                 │ │
│  │  MsgSubmitBatch / MsgCommitEpoch / ...            │ │
│  └─────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────┘
```

### 3.2 模块划分

| aardio 模块 | 功能 | 对应 Rust 模块 | 实现难度 |
|------------|------|---------------|---------|
| `pole.collector` | Steam/Epic/GOG/EA 游戏活动采集 | `activity_collector`, `steam_collector` | ⭐ 简单 |
| `pole.batchBuilder` | 批次整理、观测数据打包 | `node_pipeline::BatchBuilder` | ⭐⭐ 中等 |
| `pole.merkle` | Merkle 树构建与证明 | `records::MerkleTree` | ⭐⭐ 中等 |
| `pole.txSigner` | ed25519 签名 + 交易构造 | `signing`, `chain_bridge` | ⭐⭐ 中等 |
| `pole.rpcClient` | Cosmos REST API 交互 | (Rust 中缺失) | ⭐ 简单 |
| `pole.rewardEngine` | 奖励计算（**保留 Rust**） | `node_rewards` | — |
| `pole.verifier` | 挑战验证（**保留 Rust**） | `node_verifier` | — |
| `pole.aggregator` | 聚合引擎（**保留 Rust**） | `node_aggregator` | — |
| `pole.gui` | 桌面 GUI | — | ⭐⭐ 中等 |
| `pole.service` | Windows 服务 / 后台守护 | — | ⭐⭐ 中等 |

### 3.3 关键技术实现

#### 3.3.1 ed25519 签名（BouncyCastle）

```aardio
import BouncyCastle;

class Ed25519Signer {
    ctor(privKeyHex = null) {
        if(privKeyHex) {
            // 从十六进制私钥恢复
            var privBytes = raw.buffer(string.unhex(privKeyHex), 32);
            this.privateKey = BouncyCastle.Crypto.Parameters.Ed25519PrivateKeyParameters(privBytes, 0);
            this.publicKey = this.privateKey.GeneratePublicKey();
        } else {
            // 生成新密钥对
            var keyPair = BouncyCastle.Crypto.Generators.Ed25519KeyPairGenerator();
            keyPair.Init(BouncyCastle.Crypto.KeyGenerationParameters(
                BouncyCastle.Security.SecureRandom(), 256
            ));
            var result = keyPair.GenerateKeyPair();
            this.privateKey = result.Private;
            this.publicKey = result.Public;
        }
    };
    
    sign = function(payload) {
        var signer = BouncyCastle.Crypto.Signers.Ed25519Signer();
        signer.Init(true, this.privateKey);
        var msg = raw.buffer(payload);
        signer.BlockUpdate(msg, 0, #msg);
        return signer.GenerateSignature();  // 64 字节
    };
    
    verify = function(payload, signature) {
        var verifier = BouncyCastle.Crypto.Signers.Ed25519Signer();
        verifier.Init(false, this.publicKey);
        var msg = raw.buffer(payload);
        verifier.BlockUpdate(msg, 0, #msg);
        return verifier.VerifySignature(raw.buffer(signature));
    };
    
    getPublicKeyBytes = function() {
        return this.publicKey.GetEncoded();  // 32 字节
    };
    
    getPrivateKeyBytes = function() {
        return this.privateKey.GetEncoded();  // 32 字节
    };
}
```

#### 3.3.2 bech32 地址编码

```aardio
// 纯 aardio 实现 bech32（约 100 行）
// 参考：https://github.com/bitcoin/bips/blob/master/bip-0173.mediawiki

class Bech32 {
    CHARSET = "qpzry9x8gf2tvdw0s3jn54khce6mua7l";
    
    polymod = function(values) {
        var generator = {0x3b6a57b2, 0x26508e6d, 0x1ea119fa, 0x3d4233dd, 0x2a1462b3};
        var chk = 1;
        for(i=1;#values;1) {
            var top = chk >> 25;
            chk = (chk & 0x1ffffff) << 5 ^ values[i];
            for(j=1;5;1) {
                if((top >> (j-1)) & 1) chk ^= generator[j];
            }
        }
        return chk;
    };
    
    encode = function(hrp, data) {
        // hrp: human-readable part (e.g., "pole")
        // data: array of 5-bit values
        var combined = {};
        // Expand HRP
        for(i=1;#hrp;1) table.push(combined, hrp[i] >> 5);
        table.push(combined, 0);
        for(i=1;#hrp;1) table.push(combined, hrp[i] & 31);
        // Append data + checksum
        for(i=1;#data;1) table.push(combined, data[i]);
        for(i=1;6;1) table.push(combined, 0);
        var checksum = polymod(combined) ^ 1;
        // ... (构建最终字符串)
    };
}

// 或者：调用 Rust CLI 获取地址
var addr = process.popen("pole-cli", "address", "cosmos1...").read(-1);
```

#### 3.3.3 Cosmos 交易构造 + 广播

```aardio
import web.rest.jsonClient;

class CosmosRpcClient {
    ctor(chainId, rpcUrl = "http://localhost:1317") {
        this.chainId = chainId;
        this.rpc = web.rest.jsonClient();
        this.api = this.rpc.api(rpcUrl);
    };
    
    // 获取账户信息
    getAccount = function(address) {
        return this.api.cosmos.auth.v1beta1.accounts(address).get();
    };
    
    // 广播交易
    broadcastTx = function(txBytes) {
        return this.api.cosmos.tx.v1beta1.txs.post({
            tx_bytes = crypt.encodeBin(txBytes),
            mode = "BROADCAST_MODE_SYNC"
        });
    };
    
    // 构造签名文档
    buildSignDoc = function(msgs, accountNumber, sequence, memo = "") {
        return {
            chain_id = this.chainId,
            account_number = tostring(accountNumber),
            sequence = tostring(sequence),
            fee = { gas = "200000"; amount = {} },
            msgs = msgs,
            memo = memo
        };
    };
}
```

#### 3.3.4 调用 Rust 核心（DLL / CLI）

```aardio
// 方案 1：通过 CLI 调用
var result = process.popen(
    "pole-core", "compute-rewards",
    "--epoch-artifacts", "/path/to/artifacts.json",
    "--output", "/path/to/rewards.json"
);

// 方案 2：通过 DLL 调用（如果 Rust 编译为 cdylib）
var dll = raw.loadDll("/pole_core.dll");
var rewards = dll.computeEpochRewards(artifactsBuffer);
```

### 3.4 与现有代码的关系

| 现有 Rust 模块 | 处理方式 |
|---------------|---------|
| `src/lib.rs` | 保留，编译为 DLL 或 CLI |
| `src/signing.rs` | **废弃**（用 aardio BouncyCastle 替代） |
| `src/chain_bridge.rs` | **废弃**（用 aardio CosmosRpcClient 替代） |
| `src/node_rewards.rs` | 保留核心算法 |
| `src/node_verifier.rs` | 保留核心算法 |
| `src/records.rs` | 保留 Merkle 树 |
| `src/primitives.rs` | 保留基本类型 |
| `src/activity_collector.rs` | 可选迁移到 aardio |
| `src/node_pipeline.rs` | 可选迁移到 aardio |

---

## 四、实施计划

### 总工期：5-6 周（1 人）

| 周次 | 任务 | 产出 |
|------|------|------|
| **W1** | 搭建 aardio 工程骨架；实现 bech32；验证 Rust→CLI 调用 | 可运行的工程框架 |
| **W2** | 实现 Ed25519Signer + CosmosRpcClient；完成第一条测试网交易广播 | 第一笔交易上链 |
| **W3** | 实现 6 种交易的完整构造+签名+广播；连接 Rust 核心计算 | 完整交易流水线 |
| **W4** | 端到端集成测试（采集→批次→聚合→奖励→签名→广播→链确认） | 10 场景全部通过 |
| **W5** | GUI 界面（节点状态、交易历史、余额）；服务安装器 | 可交付的桌面应用 |
| **W6** | 文档、打包、压力测试、安全加固 | 生产候选版本 |

---

## 五、风险评估

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| BouncyCastle 性能不足 | 低 | 中 | ed25519 签名极快，.NET 实现毫秒级 |
| Cosmos REST API 不完整 | 低 | 中 | 可降级使用 gRPC（通过 golang 扩展库） |
| Rust DLL 调用不稳定 | 中 | 高 | 优先使用 CLI 模式（进程隔离）；DLL 作为优化 |
| bech32 实现有 bug | 低 | 中 | 与 Cosmos SDK 官方测试向量交叉验证 |
| .NET 依赖在 Linux 不工作 | 中 | 高 | Linux 节点可保留 Rust 签名方案；GUI 仅 Windows |

---

## 六、结论

**aardio 完全可以胜任 PoLE 的桥接层开发**，关键能力全部验证通过：

| 需求 | aardio 能力 | 状态 |
|------|------------|------|
| ed25519 签名 | BouncyCastle (.NET) | ✅ 已验证 |
| HTTP/REST | web.rest.jsonClient | ✅ 原生支持 |
| SHA256 | crypt.sha256 | ✅ 原生支持 |
| bech32 | 自实现 (~100行) | ⚠️ 需开发 |
| GUI | winform / plus 控件 | ✅ 原生支持 |
| 系统服务 | Windows Service | ✅ 标准库 |
| 调用 Rust | CLI 或 DLL | ✅ 原生支持 |
| Protobuf | REST API 绕行 | ✅ 可行 |
| P2P 网络 | wsock.tcp | ⚠️ 保留 Rust |

**推荐路径**：
1. 立即开始用 aardio 实现桥接层（Ed25519Signer + CosmosRpcClient）
2. Rust 编译为 CLI 工具，保持核心计算不变
3. 逐步将轻量模块（采集器、GUI、服务管理）迁移到 aardio
4. 6 周内可达到生产候选版本
