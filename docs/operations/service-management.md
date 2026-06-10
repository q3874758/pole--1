# PoLE 服务管理指南

> **路径约定（Windows MSI 安装）**：PoLE V1 MSI 是 perUser 范围（`InstallScope="perUser"`），实际安装到用户目录 `%LOCALAPPDATA%\PoLE\`（`HKCU`）。本文所有 Windows 命令块中的 `%LOCALAPPDATA%` 都是 Windows 用户环境变量，可直接在 `cmd.exe` / PowerShell / 资源管理器中展开，等价于 PowerShell 中的 `$env:LOCALAPPDATA` 或 `[Environment]::GetFolderPath('LocalApplicationData')`。子目录布局：`config\` / `data\` / `logs\` / `updates\`。

## Windows Service

### 安装服务

```cmd
"%LOCALAPPDATA%\PoLE\pole-node.exe" service-install "%LOCALAPPDATA%\PoLE\config\node.json"
```

### 启动服务

```cmd
"%LOCALAPPDATA%\PoLE\pole-node.exe" service-start "%LOCALAPPDATA%\PoLE\config\node.json"

# 或使用 sc
sc start PoLENode
```

### 停止服务

```cmd
"%LOCALAPPDATA%\PoLE\pole-node.exe" service-stop "%LOCALAPPDATA%\PoLE\config\node.json"

# 或使用 sc
sc stop PoLENode
```

### 查看服务状态

```cmd
"%LOCALAPPDATA%\PoLE\pole-node.exe" service-status "%LOCALAPPDATA%\PoLE\config\node.json"

# 或使用 sc
sc query PoLENode
```

### 卸载服务

```cmd
"%LOCALAPPDATA%\PoLE\pole-node.exe" service-uninstall "%LOCALAPPDATA%\PoLE\config\node.json"
```

### 服务启动类型

默认 `auto`（开机自启）。修改：

```cmd
sc config PoLENode start= auto    # 自动
sc config PoLENode start= demand  # 手动
sc config PoLENode start= disabled # 禁用
```

## Linux systemd

### 安装服务

DEB 包会自动安装并启用服务。

手动安装：
```bash
sudo cp packaging/linux/deb/pole-node.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable pole-node
```

### 启动服务

```bash
sudo systemctl start pole-node
```

### 停止服务

```bash
sudo systemctl stop pole-node
```

### 查看服务状态

```bash
sudo systemctl status pole-node
journalctl -u pole-node -f  # 实时日志
```

### 重启服务

```bash
sudo systemctl restart pole-node
```

### 卸载服务

```bash
sudo systemctl stop pole-node
sudo systemctl disable pole-node
sudo rm /etc/systemd/system/pole-node.service
sudo systemctl daemon-reload
```

## 服务日志

### Windows

日志位于安装目录的 `logs\` 子目录：

```cmd
type "%LOCALAPPDATA%\PoLE\logs\pole-node.log"
```

### Linux

```bash
journalctl -u pole-node -n 100        # 最近100行
journalctl -u pole-node -f             # 实时跟踪
cat /var/log/pole/pole-node.log
```

## 服务健康检查

```bash
# CLI 状态检查
pole-node status /etc/pole/node.json
pole-client status client-config.json

# Web 控制台
# http://127.0.0.1:8787/ -> 概览页面
```

## 后台运行模式（无服务）

### Windows

```cmd
# 使用绿色版启动器
run-pole-node.cmd

# 或直接运行
pole-node.exe run-once-p2p-sim node.json
```

### Linux

```bash
./run-pole-node.sh
# 或
./pole-node run-once-p2p-sim node.json
```

## 常见问题

### 服务启动失败

1. 检查配置文件路径是否正确
2. 检查数据目录权限
3. 查看日志中的错误信息
4. 确认端口 8787 未被占用

### 服务意外停止

PoLE 配置了 `Restart=on-failure`，服务崩溃后会自动重启。
