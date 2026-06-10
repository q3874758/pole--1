# PoLE 更新与回滚指南

> **路径约定（Windows MSI）**：PoLE V1 MSI 是 perUser 范围（`InstallScope="perUser"`，`HKCU` 写注册表），实际安装到 `%LOCALAPPDATA%\PoLE\`（即 `C:\Users\<当前用户>\AppData\Local\PoLE\`），不写入系统级 Program Files 目录。`%LOCALAPPDATA%` 是 Windows 用户环境变量（PowerShell 中等价于 `$env:LOCALAPPDATA`），在 `cmd.exe` 批处理 / PowerShell / 资源管理器中可直接展开。

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
copy /Y PoLE-new.exe "%LOCALAPPDATA%\PoLE\pole-node.exe"

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

V1 起，发布清单（`stable.json`）改为 **cosign keyless 签名**（Sigstore OIDC + Rekor 透明日志），不再使用 PGP/GPG。`stable.json` 本身不再内联签名字段；真实的 Ed25519-SHA256 签名与 Fulcio 证书作为 sidecar 文件随 GitHub Release 一起分发：

| 文件 | 用途 |
| --- | --- |
| `stable.json` | 待签名的发布清单（SHA256、artifact 列表、版本号） |
| `stable.json.sig` | cosign 签名（base64 编码的 Ed25519-SHA256 signature） |
| `stable.json.cert` | Fulcio 颁发的 short-lived 证书（含 OIDC subject / issuer） |

签名由 `.github/workflows/release.yml` 的 `Sign stable.json (cosign keyless OIDC + Rekor)` 步骤在 tag 推送时生成，OIDC token 来自 GitHub Actions，签名记录写入 Rekor 透明日志（公开可审计）。

### 验证 stable.json（操作员 / 用户）

需要 `cosign >= 2.0`：

```bash
cosign verify-blob \
  --certificate dist/release-manifests/stable.json.cert \
  --signature dist/release-manifests/stable.json.sig \
  --certificate-identity-regexp 'https://github.com/q3874758/pole--1' \
  --certificate-oidc-issuer 'https://token.actions.githubusercontent.com' \
  dist/release-manifests/stable.json
```

成功后 cosign 会打印签名的 `tlog entry`（Rekor index）与签发时间，并 exit 0；如果 `stable.json` 已被篡改、`.sig` 不是对应 OIDC 身份签的、或证书已过期，verify 会失败并给出原因。

> **关于 `dev-signature` 占位符**：早期开发版本里 `stable.json` 内联了 `signature = "dev-signature"` 字段，仅供本地联调。V1 起该字段已被移除，inline 签名刻意不存在——任何对 `stable.json` 内容的修改都会破坏 cosign 签名，从而强制验证方走 `.sig` / `.cert` 路径，避免「内联字段可被改写后仍以原值看起来合法」的风险。

## 回滚机制

更新流程会保留以下回滚信息：
- `rollback.json`: 上一版本元数据
- `install-action.json`: 安装计划
- `.bak` 备份文件

如果更新后服务启动失败，系统会自动尝试回滚。
