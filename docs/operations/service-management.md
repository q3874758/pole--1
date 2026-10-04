# PoLE 服务管理指南 (Windows)

> **路径约定（Windows 安装）**：Windows 安装/解压布局为 `%LOCALAPPDATA%\PoLE\`（perUser）。本文所有 Windows 命令块中的 `%LOCALAPPDATA%` 都是 Windows 用户环境变量，可直接在 `cmd.exe` / PowerShell / 资源管理器中展开，等价于 PowerShell 中的 `$env:LOCALAPPDATA` 或 `[Environment]::GetFolderPath('LocalApplicationData')`。子目录布局：`config\` / `data\` / `logs\` / `updates\`。

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

## 服务日志

日志位于安装目录的 `logs\` 子目录：

```cmd
type "%LOCALAPPDATA%\PoLE\logs\pole-node.log"
```

## 服务健康检查

```cmd
# CLI 状态检查
pole-node.exe service-status "%LOCALAPPDATA%\PoLE\config\node.json"
pole-client.exe status "%LOCALAPPDATA%\PoLE\config\client.json"

# Web 控制台
# http://127.0.0.1:8787/ -> 概览页面
```

## 后台运行模式（无服务）

```cmd
# 直接控制台运行
pole-node.exe run-once-p2p-sim node.json
```

## EcoQoS 与低开销保障

在 Windows 10/11 上，PoLE 守护进程通过 Win32 API 自动开启：
1. **EcoQoS（Efficiency Mode）**：将后台工作线程调度至能效核（E-cores），彻底避免争抢主游戏 CPU 性能；
2. **进程优先级**：设置为 `IDLE_PRIORITY_CLASS`；
3. **工作集修剪**：定期修剪无用页面，将物理内存占用控制在 15MB 以内。

## 常见问题

### 服务启动失败

1. 检查配置文件路径是否正确
2. 检查数据目录权限
3. 查看日志中的错误信息
4. 确认端口 8787 未被占用
