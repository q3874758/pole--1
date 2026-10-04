# PoLE V1 安装指南 (Windows)

本指南面向正式发布版 PoLE V1，现阶段项目专注于 Windows 客户端与节点服务。安装口径与白皮书一致：PoLE 是一个围绕小时奖励结算、玩家主奖励优先、可挑战可复核的专用应用型网络。

## 系统要求

- 操作系统：Windows 10 / Windows 11 (x64)
- 磁盘空间：最低 100MB，推荐 10GB+（用于数据与日志）
- 网络：需要访问互联网（P2P 网络与 Steam/Epic 游戏数据通信）

## 下载方式

从项目实际发布仓库的 GitHub Releases 页面下载：

- `PoLE-x.x.x-x64-portable.zip`（便携解压版）

## Windows 安装与目录布局

### 便携版

1. 打开 GitHub Releases 页面下载 `PoLE-x.x.x-x64-portable.zip`
2. 解压到任意目录（或 `%LOCALAPPDATA%\PoLE\`）
3. 运行 `pole-node.exe`（后台节点服务）与 `pole-client.exe`（玩家控制台 CLI）

### 目录布局

绿色版和本地解压运行目录默认采用以下结构：

```
<解压目录>\                    # 或 %LOCALAPPDATA%\PoLE\
├── pole-client.exe
├── pole-node.exe
├── config\                  # node.json / client.json
├── data\                    # 节点运行时数据
├── logs\                    # pole-node.log 等
└── updates\                 # 升级包暂存
```

`%LOCALAPPDATA%` 是 Windows 用户环境变量（PowerShell 中等价于 `$env:LOCALAPPDATA`）。

## 验证运行

启动 `pole-node.exe` 后，浏览器访问控制台：`http://127.0.0.1:8787/`

CLI 状态检查：
```cmd
pole-client.exe status
pole-node.exe service-status
```

## 卸载

- 停止正在运行的 `pole-node.exe`
- 若注册了 Windows 服务，运行 `pole-node.exe service-uninstall config\node.json`
- 直接删除解压目录或 `%LOCALAPPDATA%\PoLE\` 即可完全清除
