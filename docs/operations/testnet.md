# PoLE Socket Testnet

## Scope

当前仓库里真正可跑通的多节点互联测试网后端是 `socket` 模式。

- `real-libp2p` 目前可用于配置诊断与骨架验证
- 实际多节点采集闭环使用 `watch-p2p-socket` / `run-loop-p2p-socket`

## 1. 初始化两个节点

```bash
pole-client init node-a.json player
pole-client init node-b.json player
```

分别给两个节点设置固定监听地址：

`node-a.json`

```json
"p2p_socket": {
  "bind_addr": "127.0.0.1:4101",
  "peers": []
}
```

`node-b.json`

```json
"p2p_socket": {
  "bind_addr": "127.0.0.1:4102",
  "peers": []
}
```

## 2. 导出本机 peer 信息

```bash
pole-client p2p-socket-show node-a.json
pole-client p2p-socket-show node-b.json
```

输出包含：

- `local_peer_id`
- `bind_addr`
- `local_peer_spec`

## 3. 互相写入对端 peer

把 B 加到 A：

```bash
pole-client p2p-socket-add-peer node-a.json <node-b-peer-id> 127.0.0.1:4102
```

把 A 加到 B：

```bash
pole-client p2p-socket-add-peer node-b.json <node-a-peer-id> 127.0.0.1:4101
```

默认 topics：

`observations,batches,receipts,challenges`

也可以显式传：

```bash
pole-client p2p-socket-add-peer node-a.json <peer-id> 127.0.0.1:4102 batches,receipts,challenges
```

## 4. 启动测试网

终端 1：

```bash
pole-client watch-p2p-socket node-a.json 10
```

终端 2：

```bash
pole-client watch-p2p-socket node-b.json 10
```

或者直接跑节点循环：

```bash
pole-node run-loop-p2p-socket node-a.json 10
pole-node run-loop-p2p-socket node-b.json 10
```

## 5. 验证

检查状态：

```bash
pole-client status node-a.json
pole-client status node-b.json
```

重点看：

- `configured_p2p_batch_listeners`
- `configured_p2p_receipt_listeners`
- `last_p2p_batch_recipients`
- `last_p2p_receipt_recipients`
- `last_p2p_transport`

## Notes

- 种子节点现在允许 `peers=[]` 启动
- `collect` 会自动检测前台游戏并更新 `game_process_names`
- 若节点身份曾由占位符修复过，先清空旧 `data_dir`
