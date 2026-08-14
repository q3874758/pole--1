# PoLE 故障排查指南

## 目录

1. [编译问题](#编译问题)
2. [启动问题](#启动问题)
3. [网络问题](#网络问题)
4. [同步问题](#同步问题)
5. [性能问题](#性能问题)
6. [挖矿问题](#挖矿问题)
7. [钱包问题](#钱包问题)
8. [数据库问题](#数据库问题)

## 编译问题

### 问题: Go 模块下载失败

**症状**:
```
go: github.com/...: Get "https://proxy.golang.org/...": dial tcp: i/o timeout
```

**解决方案**:
```bash
# 使用国内镜像
go env -w GOPROXY=https://goproxy.cn,direct
go env -w GOSUMDB=sum.golang.google.cn

# 重新下载
go mod download
```

### 问题: Rust 编译失败

**症状**:
```
error: linking with `cc` failed: exit status: 1
```

**解决方案**:
```bash
# 安装必要的构建工具
# Ubuntu/Debian
sudo apt-get install build-essential

# macOS
xcode-select --install

# Windows
# 安装 Visual Studio Build Tools
```

### 问题: 类型导入错误

**症状**:
```
error[E0432]: unresolved import `crate::types`
```

**解决方案**:
```bash
# 检查 Cargo.toml 依赖
cd data/availability
cat Cargo.toml

# 应该包含:
# pole-types = { path = "../../core/types" }

# 清理并重新构建
cargo clean
cargo build
```

## 启动问题

### 问题: 端口已被占用

**症状**:
```
Error: listen tcp :9090: bind: address already in use
```

**解决方案**:
```bash
# 查找占用端口的进程
# Windows
netstat -ano | findstr :9090
taskkill /PID <PID> /F

# Linux/macOS
lsof -i :9090
kill -9 <PID>

# 或修改配置使用其他端口
POLE_RPC_PORT=:9091 ./pole-node
```

### 问题: 配置文件未找到

**症状**:
```
Error: config file not found: ~/.pole/config/config.toml
```

**解决方案**:
```bash
# 创建配置目录
mkdir -p ~/.pole/config

# 复制示例配置
cp config/genesis.json ~/.pole/config/

# 或指定配置文件路径
./pole-node --config /path/to/config.toml
```

### 问题: 权限不足

**症状**:
```
Error: permission denied: ~/.pole/data
```

**解决方案**:
```bash
# 修改目录权限
chmod -R 755 ~/.pole

# 或使用 sudo (不推荐)
sudo ./pole-node
```

## 网络问题

### 问题: 无法连接到种子节点

**症状**:
```
Error: failed to connect to seed: dial tcp: i/o timeout
```

**解决方案**:
```bash
# 1. 检查网络连接
ping seed1.pole.network

# 2. 测试端口连通性
telnet seed1.pole.network 26656
# 或
nc -zv seed1.pole.network 26656

# 3. 检查防火墙
# Windows
netsh advfirewall firewall add rule name="PoLE P2P" dir=in action=allow protocol=TCP localport=26656

# Linux (ufw)
sudo ufw allow 26656/tcp

# Linux (iptables)
sudo iptables -A INPUT -p tcp --dport 26656 -j ACCEPT

# 4. 更新种子节点列表
POLE_P2P_SEEDS=seed2.pole.network:26656,seed3.pole.network:26656 ./pole-node
```

### 问题: NAT 穿透失败

**症状**:
```
Warning: external address not reachable
```

**解决方案**:
```bash
# 1. 启用 UPnP
POLE_UPNP_ENABLED=true ./pole-node

# 2. 手动配置外部地址
POLE_EXTERNAL_ADDRESS=<your-public-ip>:26656 ./pole-node

# 3. 配置端口转发
# 在路由器上转发 26656 端口到本机
```

### 问题: 对等节点数量为 0

**症状**:
```
{"peers": 0, "syncing": false}
```

**解决方案**:
```bash
# 1. 检查 P2P 配置
cat ~/.pole/config/config.toml | grep -A 5 "\[p2p\]"

# 2. 增加最大对等节点数
POLE_P2P_MAX_PEERS=100 ./pole-node

# 3. 添加持久对等节点
POLE_P2P_PERSISTENT_PEERS=peer1@ip:26656,peer2@ip:26656 ./pole-node

# 4. 重启节点
```

## 同步问题

### 问题: 同步速度慢

**症状**:
```
Syncing: height 1000/100000 (1%)
```

**解决方案**:
```bash
# 1. 增加对等节点数
POLE_P2P_MAX_PEERS=100 ./pole-node

# 2. 优化网络参数
# 在 config.toml 中:
[p2p]
recv_rate = 5242880  # 5 MB/s
send_rate = 5242880  # 5 MB/s

# 3. 使用快照同步
# 下载最新快照
wget https://snapshots.pole.network/latest.tar.gz
tar -xzf latest.tar.gz -C ~/.pole/data

# 4. 增加数据库缓存
POLE_DB_CACHE_SIZE=4096 ./pole-node
```

### 问题: 同步卡住

**症状**:
```
Syncing: height 50000/100000 (stuck for 10 minutes)
```

**解决方案**:
```bash
# 1. 重启节点
systemctl restart pole-node

# 2. 清理对等节点
rm ~/.pole/data/addrbook.json
./pole-node

# 3. 检查磁盘空间
df -h ~/.pole

# 4. 验证数据完整性
./pole-node verify-db
```

## 性能问题

### 问题: CPU 使用率过高

**症状**:
```
CPU: 100%, Memory: 2GB
```

**解决方案**:
```bash
# 1. 限制 CPU 核心数
POLE_MAX_CPU_CORES=2 ./pole-node

# 2. 降低日志级别
POLE_LOG_LEVEL=warn ./pole-node

# 3. 禁用不必要的功能
POLE_METRICS_ENABLED=false ./pole-node

# 4. 使用 nice 降低优先级
nice -n 10 ./pole-node
```

### 问题: 内存使用过高

**症状**:
```
Memory: 8GB/8GB (OOM risk)
```

**解决方案**:
```bash
# 1. 减少数据库缓存
POLE_DB_CACHE_SIZE=1024 ./pole-node

# 2. 限制内存使用
POLE_MEMORY_LIMIT=4096 ./pole-node

# 3. 增加交换空间
sudo fallocate -l 4G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile

# 4. 使用 systemd 限制
# 在 /etc/systemd/system/pole-node.service 中:
[Service]
MemoryLimit=4G
```

### 问题: 磁盘 I/O 过高

**症状**:
```
Disk I/O: 100%, slow block processing
```

**解决方案**:
```bash
# 1. 使用 SSD
# 迁移数据到 SSD

# 2. 优化数据库
POLE_DB_TYPE=rocksdb ./pole-node

# 3. 调整 I/O 调度器
# Linux
echo deadline > /sys/block/sda/queue/scheduler

# 4. 增加文件描述符限制
ulimit -n 65536
```

## 挖矿问题

### 问题: 挖矿未启用

**症状**:
```
{"enabled": false, "note": "使用 --mining 参数启动"}
```

**解决方案**:
```bash
# 启动时添加 --mining 参数
./pole-node --mining

# 或使用环境变量
POLE_MINING_ENABLED=true ./pole-node

# Windows
.\run-mainnet-mining.bat
```

### 问题: 未检测到游戏

**症状**:
```
{"detected": []}
```

**解决方案**:
```bash
# 1. 确认游戏正在运行
# Windows
tasklist | findstr steam

# 2. 检查游戏列表
curl http://localhost:9090/mining/games

# 3. 手动添加游戏
# 编辑配置文件添加游戏 ID

# 4. 检查权限
# 确保节点有权限访问进程信息
```

### 问题: 奖励未到账

**症状**:
```
{"pending": "0", "claimed": "0"}
```

**解决方案**:
```bash
# 1. 检查挖矿余额
curl http://localhost:9090/mining/balance?address=<your-address>

# 2. 等待奖励周期
# 默认每 5 分钟发放一次

# 3. 手动领取
curl -X POST http://localhost:9090/mining/claim \
  -H "Content-Type: application/json" \
  -d '{"address": "<your-address>"}'

# 4. 检查钱包地址
curl http://localhost:9090/wallet/accounts
```

## 钱包问题

### 问题: 钱包文件损坏

**症状**:
```
Error: failed to load wallet: invalid JSON
```

**解决方案**:
```bash
# 1. 从备份恢复
cp ~/.pole/wallet.json.backup ~/.pole/wallet.json

# 2. 创建新钱包
rm ~/.pole/wallet.json
./pole-node

# 3. 导入私钥
# 使用钱包 UI 导入
```

### 问题: 忘记钱包密码

**症状**:
```
Error: incorrect password
```

**解决方案**:
```bash
# 如果有备份的私钥或助记词:
# 1. 创建新钱包
# 2. 导入私钥/助记词

# 如果没有备份:
# 无法恢复，需要创建新钱包
```

### 问题: 交易签名失败

**症状**:
```
Error: sign failed: key not found
```

**解决方案**:
```bash
# 1. 检查钱包账户
curl http://localhost:9090/wallet/accounts

# 2. 确认地址正确
# 使用钱包中存在的地址

# 3. 重新加载钱包
# 重启节点
```

## 数据库问题

### 问题: 数据库损坏

**症状**:
```
Error: database corruption detected
```

**解决方案**:
```bash
# 1. 尝试修复
./pole-node repair-db

# 2. 从备份恢复
rm -rf ~/.pole/data
tar -xzf pole-backup.tar.gz -C ~/.pole/

# 3. 重新同步
rm -rf ~/.pole/data
./pole-node
```

### 问题: 数据库锁定

**症状**:
```
Error: database is locked
```

**解决方案**:
```bash
# 1. 确认没有其他实例运行
ps aux | grep pole-node

# 2. 删除锁文件
rm ~/.pole/data/LOCK

# 3. 重启节点
./pole-node
```

## 日志分析

### 启用调试日志

```bash
POLE_LOG_LEVEL=debug ./pole-node
```

### 查看实时日志

```bash
# 如果输出到文件
tail -f ~/.pole/logs/node.log

# 如果使用 systemd
journalctl -u pole-node -f

# 如果使用 Docker
docker logs -f pole-node
```

### 日志过滤

```bash
# 只看错误
grep "ERROR" ~/.pole/logs/node.log

# 只看特定模块
grep "consensus" ~/.pole/logs/node.log

# 统计错误数量
grep -c "ERROR" ~/.pole/logs/node.log
```

## 获取帮助

如果以上方法都无法解决问题:

1. **查看文档**: https://docs.pole.network
2. **搜索 Issues**: https://github.com/pole-chain/pole/issues
3. **提交 Issue**: 包含以下信息
   - 操作系统和版本
   - PoLE 版本
   - 完整错误日志
   - 复现步骤
4. **加入社区**:
   - Discord: https://discord.gg/pole
   - Telegram: https://t.me/pole_network
5. **联系支持**: support@pole.network

## 诊断工具

### 系统信息收集脚本

```bash
#!/bin/bash
# diagnose.sh

echo "=== System Information ==="
uname -a
echo ""

echo "=== PoLE Version ==="
./pole-node version
echo ""

echo "=== Node Status ==="
curl -s http://localhost:9090/status | jq
echo ""

echo "=== Health Check ==="
curl -s http://localhost:9090/health | jq
echo ""

echo "=== Disk Space ==="
df -h ~/.pole
echo ""

echo "=== Memory Usage ==="
free -h
echo ""

echo "=== Network Connectivity ==="
ping -c 3 seed1.pole.network
echo ""

echo "=== Recent Logs ==="
tail -n 50 ~/.pole/logs/node.log
```

运行诊断:
```bash
chmod +x diagnose.sh
./diagnose.sh > diagnosis.txt
```
