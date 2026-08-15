# PoLE V1 发布前 Checklist

> 状态：2026-08-15 更新。综合全面分析（Rust/链层/打包/文档四路审计）+ 世界趋势 + 经济模型评审。
> 原则：每项必须可勾选、可验证（给出证据命令/路径），不允许"概念上完成"。

---

## 0. 范围与依据

- 本清单覆盖：**代码缺陷 → 安全 → 经济模型 → 测试/CI → 打包发布 → 文档清理**。
- 依据：六子代理全面分析（Rust/链层/打包/文档四路）、跨语言契约核对、发行曲线量化模拟、世界趋势调研。
- 目标：让 `cargo test` 全绿之外的"真实可发布性"达到可验收状态。
- 进度：✅ = 已完成（含 commit 号）；⬜ = 待办；⚠️ = 部分/进行中。

---

## 1. 🔴 代码缺陷（阻断上线）

- [ ] **1.1 FinalizeEpoch root 交叉冲突**：链端 `ValidateEpochRoots` 用 `json.Marshal` 叶重算 root，Rust 链下 root 基于 borsh 叶 → Rust 构造的 CommitEpoch 上链后 Finalize 必失败。
  - 证据：`chain/x/pole/keeper/keeper.go:586,617-657,839` + `deliverable-merkle-cross-language.md:157-178`
  - 修法：Rust commit 前改用链式 json 叶计算 root，或链端对 CommitEpoch 校验 root 来源。
- [ ] **1.2 MsgUpdateParams 缺 proto field 22/23**：Rust 端 `encode_params_inner` 只编到 field21，`min_verification_count`(22)/`min_player_verifier_share_bps`(23) 恒缺省 0 → Rust 治理调参把链上验证门禁清零。
  - 证据：`src/cosmos/pole_msgs.rs:632-656` vs `chain/proto/pole/chain/pole/v1/state.proto:65-69`
  - 修法：Rust 编码器补 field 22/23；加 golden wire 测试。

## 2. 🔴 安全（发布前必须）

- [ ] **2.1 identity.json 明文私钥**：`node_config.rs:643-648` 直接读明文 KeyPair（含 secret），无加密无口令。
  - 修法：复用 wallet AES-GCM keystore（scrypt 派生）加密 identity.json。
- [ ] **2.2 KeyPair 无 Zeroize**：`keys.rs:7` `KeyPair.secret` 无零清除；`zeroize` 依赖声明但零使用。
  - 修法：实现 `Drop for KeyPair`（zeroize secret），导出路径避免明文副本。
- [ ] **2.3 采集者签名验证旁路**：`node_verifier.rs:174-225` `all_valid` 排除 signature 项 → 无效签名不阻断批次通过。
  - 修法：把签名验证纳入 `all_valid` 硬门槛。
- [ ] **2.4 32 字节 dev-placeholder 旁路**：`node_pipeline.rs:187-189` 任意 32 字节被当 DevPlaceholder 豁免（Ed25519 恒 64 字节）。
  - 修法：收紧 dev placeholder 判定，仅 debug 构建允许。

## 3. 🔴 经济模型（方案 A：活跃度挂钩发行）

> 用户已确认参数：锚点 = 现有 `TargetNetworkWeightUnits`（单一治理参数）；年度 cap = 10%。

- [ ] **3.1 Rust 计算层**：`tokenomics.rs` 新增 `annual_emission(year, activity)` = 基准发行 × 活跃度调节因子（sqrt + cap 10%）；单测覆盖 0/边界/cap。
- [ ] **3.2 链上执行层**：`chain/x/pole` BeginBlock 年度发行铸币（按新公式入奖励池）；接通 `ComputeAdjustedHourlyReward` 调用点（当前零调用）。
- [ ] **3.3 销毁闭环**：补治理可用的 burn 通道（兑现白皮书 `Net Supply = Emission - Burn`；当前只有单向增发）。
- [ ] **3.4 Rust↔Go 同 fixtures 对账**：同一组权重 fixtures，Rust 与 Go 输出年度发行一致（进 CI）。
- [ ] **3.5 文档对齐**：白皮书 §4.4 更新公式（含 cap 10%、锚点定义），消除"文档写目标、代码不执行"。

## 4. 🟠 测试与 CI

- [ ] **4.1 transitions 状态机单测**：10 类 `apply_*`（质押/解锁/挑战/治理 quorum）补边界测试——当前零单测。
- [ ] **4.2 CI 补 go test job**：`ci.yml` 加 setup-go + `cd chain && go test ./...`（6 个 Go 测试当前不进 CI）。
- [ ] **4.3 集成测试闭环**：`tests/integration.rs` 目前仅 1 个单节点 happy-path，且 genesis 预置奖励、`open_challenge` 返回 Unimplemented——补齐 challenge/finalize/aggregate 全流程，去掉取巧。
- [ ] **4.4 CI 加 integration feature**：`cargo test --features integration`（需 poled 在 PATH）进 CI。

## 5. 🟠 打包发布（首次真实发布）

- [ ] **5.1 打 v0.1.0 tag 触发 release.yml**：从未 tag，MSI/deb/zip/SHA256/cosign 从未产出。
- [ ] **5.2 更新通道接真实发布**：`control_api.rs:529,574` 从编译期 `CARGO_MANIFEST_DIR` 读 stable.json → 改为安装布局路径 + GitHub Releases 拉取 + cosign 校验。
- [ ] **5.3 路径矛盾修复**：`layout.json`/`install-service.cmd`/`pole-node-service.json` 硬编码 `C:\Program Files\PoLE` 与 perUser `%LOCALAPPDATA%\PoLE` 冲突。
- [ ] **5.4 RELEASE_NOTES heredoc 变量展开**：`release.yml` 用 `<<'EOF'` 使 `${VERSION}` 变字面量。
- [ ] **5.5 MSI Authenticode 签名决策**：无代码签名（SmartScreen 警告）——补签名或明确文档声明"未签名"。
- [ ] **5.6 deb conffiles/版本号**：`conffiles` 未随包安装；deb 命名带 `v` 前缀与 control 版本不一致。
- [ ] **5.7 stable.json 清理**：残留 `"signature": "dev-signature"` 字段（文档称已移除）——发布前移除。

## 6. 🟡 文档清理（发布前顺手做）

- [x] **6.1 删除 legacy/**（29 文件）— ✅ `77823ee`
- [x] **6.2 删除 docs/aardio重写方案评估.md** — ✅ `77823ee`
- [x] **6.3 删除 docs/六大P0阻断问题解决方案.md + docs/待解决问题清单.md** — ✅ `77823ee`
- [x] **6.4 删除 src/proto.rs**（991 行 + 连带 96 行往返测试）— ✅ `77823ee`
- [x] **6.5 troubleshooting.md 错误仓库链接** — ✅ 本轮修复（`pole-local/pole` → `q3874758/pole--1`）

## 7. 🟢 可选（不阻断，发布后迭代）

- [ ] 7.1 libp2p 真实 swarm 传输（当前 socket 明文 + libp2p 骨架）
- [ ] 7.2 压力测试/benchmark（criterion）
- [ ] 7.3 归档重放策略（IPFS/Arweave）
- [ ] 7.4 灾难恢复手册
- [ ] 7.5 GUI Tauri 壳 / CONFIG.md / macOS CI 矩阵
- [ ] 7.6 keystore scrypt N 提升（2^14 → 2^18）

---

## 验收定义

- [ ] `cargo test` 全绿（lib 158+ 集成全部）
- [ ] `cd chain && go test ./...` 全绿
- [ ] `cargo test --features integration` 全绿（poled 在 PATH）
- [ ] CI 三 job + go test job 全绿
- [ ] 一次真实 `v0.1.0` 发布：MSI/deb/zip + SHA256 + cosign 签名 + stable.json 可被 `pole-client` 更新通道验证
- [ ] Rust↔Go 奖励/发行/Merkle 对账 fixtures 全部通过
