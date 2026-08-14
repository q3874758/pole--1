# PoLE API 参考文档

## 概述

PoLE 提供 RESTful HTTP API 用于与区块链交互。默认端口为 `:9090`。

## 基础信息

- **Base URL**: `http://localhost:9090`
- **Content-Type**: `application/json`
- **认证**: 无需认证（本地节点）

## API 端点

### 区块相关

#### 获取最新区块

```http
GET /block/latest
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "height": 12345
  }
}
```

#### 获取指定区块

```http
GET /block/{height}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "height": 12345,
    "chain_id": "pole-mainnet"
  }
}
```

### 交易相关

#### 广播交易

```http
POST /tx/broadcast
```

**请求体**:
```json
{
  "type": "transfer",
  "from": "pole1...",
  "to": "pole1...",
  "amount": "1000000000000000000",
  "fee": "1000000000000000",
  "signature": "0x..."
}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "tx_hash": "0xabc123..."
  }
}
```

#### 查询交易

```http
GET /tx/{tx_hash}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "tx_hash": "0xabc123...",
    "status": "confirmed"
  }
}
```

### 账户相关

#### 获取账户余额

```http
GET /account/balance?address={address}
```

**参数**:
- `address` (required): 账户地址

**响应示例**:
```json
{
  "success": true,
  "data": {
    "address": "pole1...",
    "balance": "1000000000000000000"
  }
}
```

#### 获取账户信息

```http
GET /account/{address}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "address": "pole1...",
    "balance": "1000000000000000000",
    "is_validator": false
  }
}
```

#### 列出所有账户

```http
GET /account/list
```

**响应示例**:
```json
{
  "success": true,
  "data": [
    {
      "address": "pole1qql8ag4cluz6r4dz28p3w00dnc9w8ueulg2gmc",
      "label": "NodeRewardPool (60%)",
      "balance": "600000000000000000000000000",
      "locked": true
    }
  ]
}
```

### 验证者相关

#### 获取验证者列表

```http
GET /validators
```

**响应示例**:
```json
{
  "success": true,
  "data": [
    {
      "address": "pole1...",
      "stake": "1000000000000000000",
      "commission": 10,
      "status": "active"
    }
  ]
}
```

### 治理相关

#### 获取提案

```http
GET /governance/proposal/{proposal_id}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "id": 1,
    "title": "Increase Block Size",
    "description": "...",
    "status": "active",
    "votes_yes": "1000000",
    "votes_no": "500000"
  }
}
```

#### 获取所有提案

```http
GET /governance/proposals
```

**响应示例**:
```json
{
  "success": true,
  "data": []
}
```

#### 投票

```http
POST /governance/vote
```

**请求体**:
```json
{
  "proposal_id": 1,
  "voter": "pole1...",
  "vote_option": 1,
  "weight": "1000000"
}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "message": "vote recorded"
  }
}
```

### 国库相关

#### 获取国库余额

```http
GET /treasury/balance
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "balance": "200000000000000000000000000"
  }
}
```

#### 获取国库提案

```http
GET /treasury/proposals
```

**响应示例**:
```json
{
  "success": true,
  "data": [
    {
      "id": 1,
      "proposer": "pole1...",
      "recipient": "pole1...",
      "amount": "1000000",
      "description": "Development funding",
      "status": "pending"
    }
  ]
}
```

### 锁仓释放相关

#### 查询锁仓状态

```http
GET /vesting/status?address={address}
```

**参数**:
- `address` (optional): 账户地址，不提供则使用钱包首个地址

**响应示例**:
```json
{
  "success": true,
  "data": {
    "has_schedule": true,
    "total": "50000000000000000000000000",
    "claimed": "10000000000000000000000000",
    "claimable": "5000000000000000000000000",
    "lock_until": 1735689600,
    "vesting_months": 24
  }
}
```

#### 领取解锁代币

```http
POST /vesting/claim
```

**请求体**:
```json
{
  "address": "pole1..."
}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "claimed": "5000000000000000000000000"
  }
}
```

### 紧急暂停相关

#### 获取暂停状态

```http
GET /emergency/status
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "is_paused": false
  }
}
```

#### 触发紧急暂停

```http
POST /emergency/pause
```

**请求体**:
```json
{
  "scope": "Full",
  "reason": "Security",
  "duration": 86400,
  "operator": "pole1..."
}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "message": "emergency pause triggered"
  }
}
```

#### 恢复网络

```http
POST /emergency/resume
```

**请求体**:
```json
{
  "operator": "pole1...",
  "approved_proposal_id": "123"
}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "message": "network resumed"
  }
}
```

### 钱包相关

#### 获取钱包账户

```http
GET /wallet/accounts
```

**响应示例**:
```json
{
  "success": true,
  "data": [
    {
      "address": "pole1...",
      "publicKey": "0x..."
    }
  ]
}
```

#### 创建新账户

```http
POST /wallet/create
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "address": "pole1...",
    "publicKey": "0x..."
  }
}
```

#### 签名交易

```http
POST /wallet/sign
```

**请求体**:
```json
{
  "type": "transfer",
  "from": "pole1...",
  "to": "pole1...",
  "amount": "1000000000000000000"
}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "signed_tx": {...},
    "signature": "0x..."
  }
}
```

#### 导出钱包备份

```http
GET /wallet/backup
```

**响应**: 下载 JSON 文件

### 挖矿相关

#### 获取挖矿状态

```http
GET /mining/status
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "enabled": true,
    "description": "Play-to-Earn 挖矿模式",
    "note": "挖矿已启用，奖励每 5 分钟自动发放"
  }
}
```

#### 获取采集游戏列表

```http
GET /mining/games
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "games": ["730", "570", "440"],
    "note": "默认采集的热门游戏列表"
  }
}
```

#### 提交游戏数据

```http
POST /mining/submit
```

**请求体**:
```json
{
  "game_id": "730",
  "value": 1000000
}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "game_id": "730",
    "value": 1000000,
    "submitted": true
  }
}
```

#### 领取挖矿奖励

```http
POST /mining/claim
```

**请求体**:
```json
{
  "address": "pole1..."
}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "address": "pole1...",
    "claimed": "1000000000000000000"
  }
}
```

#### 查询挖矿余额

```http
GET /mining/balance?address={address}
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "address": "pole1...",
    "pending": "5000000000000000000",
    "pending_count": 3
  }
}
```

#### 获取检测到的游戏

```http
GET /mining/detected
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "enabled": true,
    "detected": ["Counter-Strike 2", "Dota 2"]
  }
}
```

### 链状态相关

#### 获取链状态

```http
GET /status
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "chain_id": "pole-mainnet",
    "height": 12345,
    "app_hash": "0xabc123..."
  }
}
```

#### 获取详细链状态

```http
GET /status/chain
```

**响应示例**:
```json
{
  "success": true,
  "data": {
    "chain_id": "pole-mainnet",
    "height": 12345,
    "app_hash": "0xabc123...",
    "total_supply": "1000000000",
    "inflation": "20%",
    "bonded": "65%"
  }
}
```

### 监控相关

#### Prometheus 指标

```http
GET /metrics
```

**响应**: Prometheus 格式的指标数据

#### 健康检查

```http
GET /health
```

**响应示例**:
```json
{
  "healthy": true,
  "chain_id": "pole-mainnet",
  "height": 12345
}
```

## 错误响应

所有错误响应遵循以下格式:

```json
{
  "success": false,
  "error": "错误描述"
}
```

常见 HTTP 状态码:
- `200 OK`: 请求成功
- `400 Bad Request`: 请求参数错误
- `404 Not Found`: 资源不存在
- `405 Method Not Allowed`: HTTP 方法不允许
- `500 Internal Server Error`: 服务器内部错误
- `503 Service Unavailable`: 服务不可用

## 代币单位

所有代币金额使用最小单位（10^-18 POLE）表示。

例如:
- 1 POLE = `1000000000000000000` (10^18)
- 0.1 POLE = `100000000000000000` (10^17)

## 速率限制

默认速率限制: 1000 请求/秒

## WebSocket 支持

WebSocket 端点（计划中）:
- `/ws/blocks`: 实时区块订阅
- `/ws/txs`: 实时交易订阅
- `/ws/events`: 实时事件订阅
