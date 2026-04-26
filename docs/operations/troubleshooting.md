# PoLE 故障排查指南

## 常见问题

### 1. 服务无法启动

**症状**: 运行 `pole-node service-start` 或 `systemctl start pole-node` 失败。

**排查步骤**:

1. 检查配置文件存在且格式正确：
   ```bash
   cat /etc/pole/node.json  # Linux
   type "C:\Program Files\PoLE\config\node.json"  # Windows
   ```

2. 检查数据目录权限：
   ```bash
   ls -la /var/lib/pole  # Linux
   # 确保目录存在且可写
   ```

3. 查看详细错误：
   ```bash
   # Linux
   journalctl -u pole-node -n 50

   # Windows - 启动控制台版本查看错误
   pole-node.exe run-once-p2p-sim node.json
   ```

4. 确认端口未被占用：
   ```bash
   # Linux
   ss -tlnp | grep 8787

   # Windows
   netstat -ano | findstr 8787
   ```

### 2. Web 控制台无法访问

**症状**: 浏览器打开 `http://127.0.0.1:8787/` 显示连接被拒绝。

**排查步骤**:

1. 确认服务正在运行：
   ```bash
   # Windows
   sc query PoLENode

   # Linux
   systemctl status pole-node
   ```

2. 检查端口监听：
   ```bash
   ss -tlnp | grep 8787  # Linux
   netstat -ano | findstr 8787  # Windows
   ```

3. 防火墙规则：
   ```bash
   # Linux - 允许端口
   sudo firewall-cmd --add-port=8787/tcp --permanent
   sudo firewall-cmd --reload

   # Windows - 确认防火墙未阻止
   ```

4. 尝试重新启动服务：
   ```bash
   pole-client control-api-serve client-config.json
   ```

### 3. P2P 网络无法连接

**症状**: `last_p2p_known_peer_count` 始终为 0 或 `last_p2p_retrieval_ok` 为 false。

**排查步骤**:

1. 检查网络模式配置：
   ```bash
   pole-client status client-config.json
   # 查看 libp2p_enabled 和 p2p_simulation 字段
   ```

2. 使用模拟模式测试：
   ```bash
   pole-node run-once-p2p-sim node.json
   ```

3. 检查防火墙对 P2P 端口的设置（默认随机端口）。

### 4. 奖励计算不正确

**症状**: 预期收到奖励但余额为 0。

**排查步骤**:

1. 检查奖励配置：
   ```bash
   pole-client reward-config-show client-config.json
   ```

2. 确认游戏进程被正确检测：
   ```bash
   # 查看当前前台进程
   pole-client capture-foreground-process client-config.json
   ```

3. 检查 epoch 状态：
   ```bash
   pole-node status node.json | grep -E "next_epoch|next_slot|ticks_completed"
   ```

4. 查看 epoch 结算：
   ```bash
   pole-client settle-epoch client-config.json
   ```

### 5. Steam 数据未采集

**症状**: `activity_source_count` 为 0 或 `stored_payloads` 不增长。

**排查步骤**:

1. 确认 Steam App ID 已配置：
   ```bash
   pole-client status client-config.json | grep target_app_ids
   ```

2. 添加 Steam 数据源：
   ```bash
   pole-client activity-sources-add client-config.json 730 steam
   ```

3. 手动触发采集：
   ```bash
   pole-client collect client-config.json
   ```

4. 测试 Steam API 连通性：
   ```bash
   pole-node build-batch-from-steam-api node.json 1 1 730
   ```

## 日志位置

| 平台 | 路径 |
|------|------|
| Windows (服务) | `C:\Program Files\PoLE\logs\` |
| Windows (绿色版) | `<安装目录>\pole-node-data\logs\` |
| Linux | `/var/log/pole/` 或 `journalctl -u pole-node` |

## 诊断命令

```bash
# 完整诊断
pole-client doctor client-config.json

# 网络诊断
pole-node libp2p-diagnose node.json

# 节点状态
pole-node status node.json

# 客户端状态
pole-client status client-config.json

# 代币经济信息
pole-client tokenomics client-config.json
```

## 获取帮助

- 查看文档: `docs/`
- 提交 Issue: https://github.com/pole-local/pole/issues
