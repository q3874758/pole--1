# PoLE 更新与回滚指南

## 自动更新

PoLE 使用签名验证的更新机制。更新包从 release 通道（`stable`）获取。

### 检查更新

```bash
# 通过 CLI
pole-node status /etc/pole/node.json
# 查看 "update_available" 字段

# 通过 Web 控制台
# 访问 http://127.0.0.1:8787 -> 更新页面
```

### 执行更新

```bash
# 准备更新（下载并验证）
pole-node update-stage /etc/pole/node.json

# 应用更新
pole-node update-apply /etc/pole/node.json

# 提交安装
pole-node update-commit-install /etc/pole/node.json
```

### 回滚

```bash
# 回滚到上一版本
pole-node update-rollback /etc/pole/node.json
```

## 手动更新

### 下载新版本

1. 访问 PoLE 官网下载最新发布包
2. 验证 SHA256 校验和
3. 替换二进制文件

### Windows

```cmd
# 停止服务
net stop PoLENode

# 替换文件
copy /Y PoLE-new.exe "C:\Program Files\PoLE\pole-node.exe"

# 启动服务
net start PoLENode
```

### Linux

```bash
sudo systemctl stop pole-node
sudo cp pole-node-new /opt/pole/pole-node
sudo systemctl start pole-node
```

## 更新签名验证

PoLE 验证更新的签名。如果签名无效，更新将被拒绝。

当前 `stable.json` 中的签名：
- 开发版本: `dev-signature`（非真实签名）
- 正式版需替换为真实 PGP/GPG 签名

## 回滚机制

更新流程会保留以下回滚信息：
- `rollback.json`: 上一版本元数据
- `install-action.json`: 安装计划
- `.bak` 备份文件

如果更新后服务启动失败，系统会自动尝试回滚。
