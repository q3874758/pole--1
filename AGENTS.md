# AGENTS.md

PoLE（Proof of Live Engagement）— Rust 链下协议引擎 + Cosmos SDK 链 + Win 桌面壳 + 多形态打包。

## Setup commands

- Install deps: `cargo build`（根目录 Rust workspace）+ `cd chain && go mod download`（chain/ Go 子模块）
- Start dev: `cargo run --bin pole-client -- --help`（看子命令树）/ `cargo run --bin pole-node -- --help` / `cargo run --bin pole -- --help`
- Build (release): `cargo build --release`（产物在 `target/release/`）/ `cd chain && go build ./...`（poled 二进制）
- Test (Rust): `cargo test`（workspace 全部）；集成测试 `cargo test --features integration`（依赖已编译的 poled 在 PATH）
- Test (Go/chain): `cd chain && go test ./...`
- Lint: `cargo clippy --all-targets`（Rust）/ `cd chain && go vet ./...`（Go）
- Typecheck: N/A（Rust + Go 都是编译期类型检查，跑 build/test 即可）
- Format: `cargo fmt --all` / `cd chain && gofmt -l .`

## Project layout

- `src/` — Rust 协议层 + CLI 入口（6 个 `[[bin]]` + `pole_protocol_draft` 共享 lib，49 pub mod + 2 private mod）
- `src/bin/` — 6 个二进制入口：`pole-client` / `pole-node` / `pole-gui` / `pole` / `pole-genesis` / `pole-sbom`
- `src/control_api.rs` — 本机 loopback HTTP/1.1（GUI webview ↔ pole-client 唯一通道）
- `chain/` — Cosmos SDK 链（Go 1.26, SDK v0.54, CometBFT v0.39）；`x/pole/` 自定义模块
- `chain/proto/pole/chain/pole/v1/` — 4 个 proto 文件（state / tx / query / genesis）
- `chain/cmd/poled/` — poled daemon 入口（`main.go` + `cmd/root.go`，已补齐）
- `chain/app/params/` — 编码配置（`encoding.go`，ProtoCodec + TxConfig + Amino）
- `desktop/web/` — pole-gui 内嵌的本地控制台静态资源（HTML / JS / CSS 三件套，`include_str!` 嵌入 pole-client）
- `docs_PoLE_Whitepaper.md` — PoLE 正式白皮书 v2.1（1055 行）
- `docs/operations/` — 运维文档（install / service-management / troubleshooting / testnet / update）
- `packaging/windows/` — WiX 3.14 MSI 打包（`Product.wxs` + `layout.json`）
- `packaging/linux/deb/` — Debian 包 + systemd unit
- `tools/wix/` — WiX 3.14 RTM 工具链（**已 untrack，依赖 `release.yml:78-83` 在 runner 上自动下载**；本地 `tools/wix/` 仍存在但被 `.gitignore:32-35` 排除；如要改 wix 版本改 workflow step 即可）
- `scripts/` — Windows PowerShell 启动 / 停止 / 状态 / 打包 / 安装脚本
- `dist/release-manifests/` — 升级清单（`stable.json` 已改为 cosign keyless 签名结构，见下「V1 发布前必修 #2」）
- `pole-sbom` 自研二进制 — 输出 CycloneDX 1.5 + SPDX 2.3 双格式 SBOM

## Code style

- Rust: 跟随默认 rustfmt（`cargo fmt --all`）+ clippy 默认 lint（`cargo clippy --all-targets`）
- Go: `gofmt` + `go vet`（无强制 `golangci-lint`，如要补先建 `.golangci.yml`）
- 提交前跑 `cargo fmt --all` 和 `gofmt -l .`（后者输出应为空）
- 命名：`snake_case` 文件 / 函数 / 变量；`CamelCase` 类型；SCREAMING_SNAKE 常量
- 引用规范：白皮书 § → 代码 file:line 双向引用用 `TRACEABILITY.md` 维护

## Testing instructions

- Rust 单测：`cargo test`（workspace-wide）
- Rust 集成测试：`cargo test --features integration`（需要先 `cd chain && go build -o /tmp/poled .` 并把 poled 放 PATH）
- Go 测试：`cd chain && go test ./...`（含 7 个 `app_test.go` + `keeper_test.go` + `reward_math_test.go`）
- 改了什么就加什么测试；不在测试保护下的行为禁止重构
- 跨语言对账（如 Merkle 根）必须 Rust + Go 跑同一组 fixtures 输出 hash 一致
- CI 流水线：`.github/workflows/ci.yml` 三 job（`rust` / `license` / `sbom`）

## PR & commit conventions

- 默认分支：`main`（如未确定先 `git symbolic-ref --short refs/remotes/origin/HEAD`）
- Branch from `main`；禁止直接 push 到 main
- Commit message：建议 Lore 风格（首行写**动机**，正文含约束与取舍；可加 Git trailer 如 `Tested:` / `Not-tested:` / `Confidence:`）；不强制 conventional commits
- Open PR via `gh pr create`（CI 绿后）
- 改动尽量小、可审查、可回滚；非请求不引入新依赖
- 完成提交前跑相关 `cargo` 检查（`cargo test` / `cargo clippy` / 受影响 crate 的 fmt）

## V1 发布前必修（合规 / 打包风险）— 状态 2026-06-10

> 6 项硬卡点 **5/6 已关闭 + 1 项 PARTIAL**。逐项 commit 如下。

1. ~~MSI `UpgradeCode` 占位符~~ — ✅ `1fec72a`：替换为 PoLE 命名空间派生的 UUID v5（`AAF27219-EA7E-51B7-B059-D6F82CEE9DF8`），升级路径永久锁定。
2. ~~`stable.json` 签名占位~~ — ✅ `b8f1e56`：改为 cosign keyless（Sigstore OIDC + Rekor），`release.yml` 含 `sign-blob` + `id-token: write`。
3. ~~MSI 路径与文档路径不一致~~ — ✅ `c09cb04`：`docs/operations/*.md` 已对齐 `%LOCALAPPDATA%\PoLE`（`perUser` 安装路径）。
4. ~~WiX 工具链入库~~ — ✅ `7015164`：`git rm -r tools/wix/`，改由 `release.yml:78-83` 自动下载。
5. ~~License 黑名单双向同步~~ — ✅ `f1e627a`：`pole-sbom` 与 `deny.toml` 均含 GPL/AGPL/SSPL/Commons-Clause/Elastic-2.0。
6. **`core2` 路径依赖 + 无 hash 校验** — ⚠️ PARTIAL（`9f94c37`）：已加 `[patch.crates-io]` path dep + `deny.toml` clarify；**缺 `.cargo-checksum.json` 校验**。V1.1 必修：迁移 `[source.crates-io] replace-with` 或重跑 `cargo vendor --versioned-dirs --locked`。

## 项目关键契约（白皮书 → 代码 → 链 三方对账）

- 1 小时奖励区块：`reward_block_secs = 3600`（`src/params.rs:33` + `chain/x/pole/types/helpers.go:26-50`）
- 跨周期调节：平方根负反馈 + cap 20%（`src/node_rewards.rs:860-882`）
- 玩家权重 × 游戏权重 GVS：`src/node_gvs.rs:121-136`（Rust 端 PPM 三阶乘积）↔ `chain/x/pole/types/reward_math.go`（Go 端 big.Int 实现）
- Merkle 树：**两端都自实现**（链端 `chain/x/pole/types/merkle.go` 0x00/0x01，Rust 端 `src/node_pipeline.rs::merkle_root` stable_hash32）— ✅ 已做跨语言对账（Rust↔Go 同一组 fixtures 输出 hash 一致，见 `deliverable-merkle-cross-language.md`）

## Security

- 永不提交 secrets — `.env` 已在 `.gitignore`
- 私钥/助记词只在 `pole-node-data/identity.json`，永不 commit（已在 `.gitignore`）
- 🔴 链下 `transitions::apply_*` 验签**当前只校验 `signature` 非空**（`src/transitions.rs::ensure_signature`），无真正 Ed25519 — **当前最高优先级安全缺口**（第二步必修 #1）
- 链下 `transitions.rs` 中 grep `adjusted_player_block_reward` 0 引用、`reward_burn_*` 0 引用 — 算法实现完整但**没有任何 apply_* 调用**
- ✅ `stable.json` cosign 签名 + MSI `UpgradeCode` 锁定已闭环（见上「V1 发布前必修」）
