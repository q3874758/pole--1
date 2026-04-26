# PoLE

PoLE（Proof of Live Engagement）是一个围绕 PC 游戏真实参与信号构建的专用应用型网络。

V1 的正式发布口径与白皮书保持一致：

- 以 `1 小时奖励区块` 作为最小奖励结算单元
- 以玩家主奖励为核心，服务奖励为辅助
- 采用“链下采集与复核，链上承诺、结算与 Challenge”的最小可信结构

当前仓库包含：

- `pole-client`：玩家端与运维端 CLI
- `pole-node`：节点服务 CLI
- `pole-gui`：桌面 GUI 入口（启用 `gui` feature）
- `desktop/web/`：本地控制台页面
- `docs_PoLE_Whitepaper.md`：PoLE 正式发布版白皮书

## 快速入口

- 白皮书：`docs_PoLE_Whitepaper.md`
- 安装：`docs/operations/install.md`
- 服务管理：`docs/operations/service-management.md`
- 故障排查：`docs/operations/troubleshooting.md`

## 本地验证

```bash
cargo build
cargo test
```

## 发布说明

正式对外发布前，建议至少交付以下产物：

- Windows MSI 安装包
- Windows portable 压缩包
- Linux `deb` 包
- 对应版本的发布说明与校验信息

项目默认的便携版运行目录布局为：

- 配置文件：发布目录中的 `node.json` 或 `client.json`
- 数据目录：同级 `pole-node-data/`
- 日志目录：`pole-node-data/logs/`

这套目录结构与 CLI、控制台和运维文档保持一致，适合作为 V1 正式发布的默认布局。
