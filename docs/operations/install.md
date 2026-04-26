# PoLE V1 安装指南

本指南面向正式发布版 PoLE V1，安装口径与白皮书一致：PoLE 是一个围绕小时奖励结算、玩家主奖励优先、可挑战可复核的专用应用型网络。

## 系统要求

- 操作系统：Windows 10/11 (x64) 或 Linux (amd64)
- 磁盘空间：最低 100MB，推荐 10GB+（用于数据与日志）
- 网络：需要访问互联网（Steam API / Epic API 等数据源）

## 下载方式

正式发布时，所有安装包和绿色版都应从该项目实际发布仓库的 GitHub Releases 页面下载。

建议发布至少包含以下产物：

- `PoLE-x.x.x-x64.msi`
- `PoLE-x.x.x-x64-portable.zip`
- `pole-node_x.x.x_amd64.deb`

## Windows 安装

### MSI 安装包（推荐）

1. 打开项目发布仓库的 GitHub Releases 页面
2. 下载 `PoLE-x.x.x-x64.msi`
3. 双击运行安装程序
4. 安装完成后，从开始菜单或桌面快捷方式启动

安装内容：
- `pole-gui.exe` - 图形界面
- `pole-client.exe` - 客户端 CLI
- `pole-node.exe` - 节点服务
- 开始菜单和桌面快捷方式

可选功能：
- `Start with Windows`：开机自动启动 GUI

### 便携版

1. 打开项目发布仓库的 GitHub Releases 页面
2. 下载 `PoLE-x.x.x-x64-portable.zip`
3. 解压到任意目录
4. 运行 `pole-gui.exe`

## Linux 安装

### DEB 包（推荐）

1. 打开项目发布仓库的 GitHub Releases 页面
2. 下载 `pole-node_x.x.x_amd64.deb`
3. 安装：

```bash
sudo dpkg -i pole-node_x.x.x_amd64.deb
sudo systemctl enable pole-node
sudo systemctl start pole-node
```

安装后文件位置：
- 服务配置：`/etc/pole/node.json`
- 数据目录：`/var/lib/pole`
- 日志目录：`/var/log/pole`
- 二进制文件：`/opt/pole/`

## 便携版默认目录布局

Windows 绿色版和本地解压运行目录默认采用以下结构：

- `pole-client.exe`
- `pole-node.exe`
- `pole-gui.exe`
- `client.json` 或 `node.json`
- `pole-node-data/`
- `pole-node-data/logs/`
- `pole-node-data/updates/`

这也是当前 CLI `paths` 命令与控制面默认输出的路径结构。

## 验证安装

### Windows

GUI 启动后访问：`http://127.0.0.1:8787/`

CLI 状态检查：
```cmd
pole-client status
pole-node service-status
```

### Linux

```bash
systemctl status pole-node
pole-client status /etc/pole/node.json
```

## 卸载

### Windows

- MSI 安装版：通过"应用和功能"卸载
- 便携版：直接删除目录

### Linux

```bash
sudo systemctl stop pole-node
sudo systemctl disable pole-node
sudo dpkg -r pole-node
```
