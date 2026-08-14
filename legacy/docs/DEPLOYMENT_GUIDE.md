# PoLE 部署指南

## 目录

1. [系统要求](#系统要求)
2. [安装步骤](#安装步骤)
3. [配置说明](#配置说明)
4. [启动节点](#启动节点)
5. [Docker 部署](#docker-部署)
6. [Kubernetes 部署](#kubernetes-部署)
7. [监控和日志](#监控和日志)
8. [备份和恢复](#备份和恢复)
9. [故障排查](#故障排查)

## 系统要求

### 最低配置

- **CPU**: 2 核
- **内存**: 4 GB RAM
- **存储**: 100 GB SSD
- **网络**: 10 Mbps 上下行带宽
- **操作系统**: Windows 10/11, Linux (Ubuntu 20.04+), macOS 11+

### 推荐配置

- **CPU**: 4 核或更多
- **内存**: 8 GB RAM 或更多
- **存储**: 500 GB NVMe SSD
- **网络**: 100 Mbps 上下行带宽
- **操作系统**: Ubuntu 22.04 LTS

### 软件依赖

- **Go**: 1.24.0 或更高
- **Rust**: 1.70 或更高
- **Git**: 最新版本

## 安装步骤

### 1. 克隆仓库

```bash
git clone https://github.com/pole-chain/pole.git
cd pole
```

### 2. 安装 Go 依赖

```bash
go mod download
```

### 3. 构建 Rust 模块

```bash
cd core
cargo build --release
cd ..
```

### 4. 构建节点

```bash
# Windows
go build -o pole-node.exe ./cmd/node

# Linux/macOS
go build -o pole-node ./cmd/node
```

### 5. 初始化配置

```bash
# 创建配置目录
mkdir -p ~/.pole/config
mkdir -p ~/.pole/data

# 复制创世文件
cp config/genesis.json ~/.pole/config/
```

## 配置说明

### 创世配置 (genesis.json)

```json
{
  "chain_id": "pole-mainnet",
  "genesis_time": "2026-01-01T00:00:00Z",
  "initial_supply": "1000000000000000000000000000",
  "accounts": [
    {
      "address": "pole1qql8ag4cluz6r4dz28p3w00dnc9w8ueulg2gmc",
      "balance": "600000000000000000000000000"
    }
  ]
}
```

### 节点配置 (config.toml)

创建 `~/.pole/config/config.toml`:

```toml
[node]
moniker = "my-pole-node"
chain_id = "pole-mainnet"

[rpc]
port = ":9090"
enable_tls = false
max_msg_size = 4194304

[p2p]
listen_addr = "0.0.0.0:26656"
seeds = "seed1.pole.network:26656,seed2.pole.network:26656"
max_peers = 50

[consensus]
block_time = 5
timeout_propose = 3000
timeout_commit = 5000

[mining]
enabled = false
auto_collect = false
reward_interval = 300

[logging]
level = "info"
format = "json"
output = "stdout"
```

### 环境变量

创建 `.env` 文件:

```bash
# 节点配置
POLE_HOME=~/.pole
POLE_CHAIN_ID=pole-mainnet
POLE_MONIKER=my-node

# RPC 配置
POLE_RPC_PORT=9090
POLE_RPC_ENABLE_TLS=false

# P2P 配置
POLE_P2P_PORT=26656
POLE_P2P_SEEDS=seed1.pole.network:26656

# 挖矿配置
POLE_MINING_ENABLED=false

# 日志配置
POLE_LOG_LEVEL=info
POLE_LOG_FORMAT=json
```

## 启动节点

### 测试网节点

```bash
# Windows
.\pole-node.exe --config ~/.pole/config/config.toml

# Linux/macOS
./pole-node --config ~/.pole/config/config.toml
```

### 主网节点

```bash
# Windows
.\run-mainnet.bat

# Linux/macOS
./scripts/start-mainnet.sh
```

### 挖矿节点

```bash
# Windows
.\run-mainnet-mining.bat

# Linux/macOS
./scripts/start-mainnet.sh --mining
```

### 使用 PowerShell 脚本

```powershell
# 测试网
.\scripts\run.ps1 -Profile testnet

# 主网
.\scripts\run.ps1 -Profile mainnet

# 主网 + 挖矿
.\scripts\run.ps1 -Profile mainnet -Mining

# 打开浏览器
.\scripts\run.ps1 -Profile mainnet -OpenBrowser
```

## Docker 部署

### 构建镜像

创建 `Dockerfile`:

```dockerfile
FROM golang:1.24-alpine AS go-builder

WORKDIR /app
COPY go.mod go.sum ./
RUN go mod download

COPY . .
RUN go build -o pole-node ./cmd/node

FROM rust:1.70-alpine AS rust-builder

WORKDIR /app
COPY core/ ./core/
WORKDIR /app/core
RUN cargo build --release

FROM alpine:latest

RUN apk --no-cache add ca-certificates

WORKDIR /root/

COPY --from=go-builder /app/pole-node .
COPY --from=rust-builder /app/core/target/release/*.so /usr/local/lib/
COPY config/ ./config/

EXPOSE 9090 26656

CMD ["./pole-node"]
```

### 构建和运行

```bash
# 构建镜像
docker build -t pole-node:latest .

# 运行容器
docker run -d \
  --name pole-node \
  -p 9090:9090 \
  -p 26656:26656 \
  -v ~/.pole:/root/.pole \
  pole-node:latest
```

### Docker Compose

创建 `docker-compose.yml`:

```yaml
version: '3.8'

services:
  pole-node:
    build: .
    container_name: pole-node
    ports:
      - "9090:9090"
      - "26656:26656"
    volumes:
      - pole-data:/root/.pole
    environment:
      - POLE_CHAIN_ID=pole-mainnet
      - POLE_MONIKER=docker-node
      - POLE_LOG_LEVEL=info
    restart: unless-stopped

  prometheus:
    image: prom/prometheus:latest
    container_name: pole-prometheus
    ports:
      - "9091:9090"
    volumes:
      - ./monitoring/prometheus.yml:/etc/prometheus/prometheus.yml
      - prometheus-data:/prometheus
    command:
      - '--config.file=/etc/prometheus/prometheus.yml'
    restart: unless-stopped

  grafana:
    image: grafana/grafana:latest
    container_name: pole-grafana
    ports:
      - "3000:3000"
    volumes:
      - grafana-data:/var/lib/grafana
      - ./monitoring/grafana/dashboards:/etc/grafana/provisioning/dashboards
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin
    restart: unless-stopped

volumes:
  pole-data:
  prometheus-data:
  grafana-data:
```

启动:

```bash
docker-compose up -d
```

## Kubernetes 部署

### 创建命名空间

```yaml
# namespace.yaml
apiVersion: v1
kind: Namespace
metadata:
  name: pole
```

### 部署节点

```yaml
# deployment.yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: pole-node
  namespace: pole
spec:
  replicas: 3
  selector:
    matchLabels:
      app: pole-node
  template:
    metadata:
      labels:
        app: pole-node
    spec:
      containers:
      - name: pole-node
        image: pole-node:latest
        ports:
        - containerPort: 9090
          name: rpc
        - containerPort: 26656
          name: p2p
        env:
        - name: POLE_CHAIN_ID
          value: "pole-mainnet"
        - name: POLE_MONIKER
          valueFrom:
            fieldRef:
              fieldPath: metadata.name
        volumeMounts:
        - name: data
          mountPath: /root/.pole
        resources:
          requests:
            memory: "4Gi"
            cpu: "2"
          limits:
            memory: "8Gi"
            cpu: "4"
      volumes:
      - name: data
        persistentVolumeClaim:
          claimName: pole-data-pvc
```

### 创建服务

```yaml
# service.yaml
apiVersion: v1
kind: Service
metadata:
  name: pole-rpc
  namespace: pole
spec:
  selector:
    app: pole-node
  ports:
  - port: 9090
    targetPort: 9090
    name: rpc
  type: LoadBalancer

---
apiVersion: v1
kind: Service
metadata:
  name: pole-p2p
  namespace: pole
spec:
  selector:
    app: pole-node
  ports:
  - port: 26656
    targetPort: 26656
    name: p2p
  type: NodePort
```

### 持久化存储

```yaml
# pvc.yaml
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: pole-data-pvc
  namespace: pole
spec:
  accessModes:
    - ReadWriteOnce
  resources:
    requests:
      storage: 500Gi
  storageClassName: fast-ssd
```

### 部署

```bash
kubectl apply -f namespace.yaml
kubectl apply -f pvc.yaml
kubectl apply -f deployment.yaml
kubectl apply -f service.yaml
```

## 监控和日志

### Prometheus 配置

创建 `monitoring/prometheus.yml`:

```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'pole-node'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
```

### Grafana 仪表板

导入预配置的仪表板:

1. 访问 Grafana: `http://localhost:3000`
2. 登录 (admin/admin)
3. 导入 `monitoring/grafana/dashboards/pole-node.json`

### 日志收集

使用 ELK Stack:

```yaml
# filebeat.yml
filebeat.inputs:
- type: log
  enabled: true
  paths:
    - /var/log/pole/*.log
  json.keys_under_root: true

output.elasticsearch:
  hosts: ["elasticsearch:9200"]
```

## 备份和恢复

### 备份数据

```bash
# 停止节点
systemctl stop pole-node

# 备份数据目录
tar -czf pole-backup-$(date +%Y%m%d).tar.gz ~/.pole/data

# 备份配置
tar -czf pole-config-$(date +%Y%m%d).tar.gz ~/.pole/config

# 启动节点
systemctl start pole-node
```

### 恢复数据

```bash
# 停止节点
systemctl stop pole-node

# 恢复数据
tar -xzf pole-backup-20260101.tar.gz -C ~/

# 启动节点
systemctl start pole-node
```

### 自动备份脚本

```bash
#!/bin/bash
# backup.sh

BACKUP_DIR="/backup/pole"
DATE=$(date +%Y%m%d_%H%M%S)

mkdir -p $BACKUP_DIR

# 备份数据
tar -czf $BACKUP_DIR/data-$DATE.tar.gz ~/.pole/data

# 保留最近 7 天的备份
find $BACKUP_DIR -name "data-*.tar.gz" -mtime +7 -delete
```

添加到 crontab:

```bash
# 每天凌晨 2 点备份
0 2 * * * /path/to/backup.sh
```

## 故障排查

### 常见问题

#### 1. 节点无法启动

检查日志:
```bash
tail -f ~/.pole/logs/node.log
```

可能原因:
- 端口被占用
- 配置文件错误
- 权限不足

#### 2. 无法连接到种子节点

检查网络:
```bash
telnet seed1.pole.network 26656
```

解决方案:
- 检查防火墙设置
- 验证种子节点地址
- 检查网络连接

#### 3. 同步缓慢

优化配置:
```toml
[p2p]
max_peers = 100
recv_rate = 5242880
send_rate = 5242880
```

#### 4. 内存不足

增加交换空间:
```bash
sudo fallocate -l 8G /swapfile
sudo chmod 600 /swapfile
sudo mkswap /swapfile
sudo swapon /swapfile
```

### 性能调优

#### 系统参数

```bash
# 增加文件描述符限制
ulimit -n 65536

# 优化网络参数
sysctl -w net.core.rmem_max=134217728
sysctl -w net.core.wmem_max=134217728
```

#### 数据库优化

```toml
[database]
cache_size = 2048
max_open_files = 10000
```

### 获取帮助

- **文档**: https://docs.pole.network
- **Discord**: https://discord.gg/pole
- **GitHub Issues**: https://github.com/pole-chain/pole/issues
- **邮件**: support@pole.network
