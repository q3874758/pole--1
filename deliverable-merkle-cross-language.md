# Merkle 跨语言对账 — V1 必修 e2e 缺口 #3 关闭

> **结论一句话**：Rust 端 `merkle_root` 之前用的是 FNV-1a 派生 hash（`stable_hash32`），跟 chain 端 sha256 算法完全不兼容，是 V1 最高风险。本次替换为 sha256 + 0x00/0x01 域分隔符对齐 `chain/x/pole/types/merkle.go`，配套 11 个 Rust 单测 + 3 个 Go 测试（含 6 个 root fixture 子测试）双侧 byte-identical 锁定。

## TL;DR

- **根因**：`src/node_pipeline.rs::merkle_root` 父节点哈希走 `stable_hash32`（FNV-1a 派生，4×u64 seed + PRIME），chain 端 `merkle.go` 走 sha256。**两种 hash 函数输出完全不重叠**，任何跨链 Merkle 证明验证都会 fail。
- **修复**：
  - `merkle_root` 父节点改 `sha256(0x01 || left || right)`（0x01 域分隔符防 second-preimage attack）
  - 新增 `merkle_leaf_sha256(record_bytes)`：`sha256(0x00 || bytes)`（0x00 区分 leaf vs parent）
  - 5 个 caller（node_pipeline / node_rewards / node_runtime / node_verifier / node_aggregator）的 leaf 哈希从 `stable_hash32(borsh(record))` 切到 `merkle_leaf_sha256(borsh(record))`
- **跨语言 fixture 锁定**：Rust 11 个 golden vector + Go 6 个 root fixture 子测试，所有期望值由 Python `hashlib.sha256` 独立算出来后硬编码进两侧测试 —— 任何一边 drift CI 立即挂
- **验证**：`cargo test --lib` 138/138、`go test ./...` 全过、`go vet`/`gofmt`/`cargo clippy` 全清

## 背景

读两边实现时发现的核心差异：

| 维度 | Rust 端（修复前） | Go 端（chain） |
|------|-------------------|----------------|
| Hash 函数 | FNV-1a 派生（`stable_hash32`，4×u64 seed × PRIME `0x0000_0100_0000_01B3`） | **sha256**（`crypto/sha256`） |
| 父节点域分隔符 | 无 | `0x01` 前缀 |
| 叶子域分隔符 | 无 | `0x00` 前缀 |
| 奇数 leaf 处理 | 复制最后一片 | 复制最后一片 ✓ |
| 空树 | 32 字节 0 | 32 字节 0 ✓ |

**结论**：Rust 端整个 Merkle 算法跟 chain 端**完全对不上**——不是字段名/编码差异，是 hash 函数本身错。任何 Rust 生成的 `MerkleCommitment.root` 提交到链上后，链端 `ValidateMerkleProofHex`（`msg_server.go::validateChallengeEvidence`）尝试用 sha256 重算 root，**永远 mismatch**。

## 改了什么

### A. `src/node_pipeline.rs` — Merkle 算法核心

```rust
// 之前（错的）：
next.push(stable_hash32(&pair));   // FNV-1a 派生

// 之后（对齐 chain）：
let mut hasher = Sha256::new();
hasher.update([0x01u8]);           // 0x01 域分隔符
hasher.update(left);
hasher.update(right);
next.push(hasher.finalize().into());
```

新增 `merkle_leaf_sha256`：

```rust
pub fn merkle_leaf_sha256(record_bytes: &[u8]) -> Hash32 {
    let mut hasher = Sha256::new();
    hasher.update([0x00u8]);        // 0x00 区分 leaf vs parent
    hasher.update(record_bytes);
    hasher.finalize().into()
}
```

`sha2 = "0.10"` 已在 `Cargo.toml` 直接依赖里（无需新增 dep）。

### B. 5 个 caller 切到 `merkle_leaf_sha256`

| 文件 | 之前 | 之后 |
|------|------|------|
| `src/node_pipeline.rs:199` | `stable_hash32(&borsh::to_vec(item))` | `merkle_leaf_sha256(&borsh::to_vec(item))` |
| `src/node_rewards.rs:711` | `crate::stable_hash32(&encoded)` | `crate::node_pipeline::merkle_leaf_sha256(&encoded)` |
| `src/node_runtime.rs:241` | `stable_hash32(&borsh::to_vec(...))` | `merkle_leaf_sha256(&borsh::to_vec(...))` |
| `src/node_runtime.rs:261` | 同上 | 同上 |
| `src/node_runtime.rs:329` (`hash_observation`) | `stable_hash32(...)` | `merkle_leaf_sha256(...)` |
| `src/node_verifier.rs:141` | `stable_hash32(...)` | `merkle_leaf_sha256(...)` |
| `src/node_aggregator.rs:246` (`hash_aggregate_record`) | `stable_hash32(&encoded)` | `merkle_leaf_sha256(&encoded)` |
| `src/node_aggregator.rs:252` (`hash_batch_commit`) | 同上 | 同上 |

`stable_hash32` 本身保留——其他模块（identity keys / params hash / payload hash）继续用它，互不影响。

### C. 跨语言 fixture 测试（核心保障）

#### Rust 侧（11 个新测试，`src/node_pipeline.rs::tests`）

| 测试 | 验证内容 |
|------|---------|
| `leaf_domain_separator_matches_chain_format` | `sha256(0x00 \|\| "a")` = `022a6979e6da...` |
| `leaf_b_and_c_match_chain_format` | `sha256(0x00 \|\| "b")` / `"c"` |
| `root_empty_tree_is_all_zero` | 空树 = 32 字节 0 |
| `root_single_leaf_equals_leaf_hash` | 单叶 root == leaf |
| `root_two_leaves_matches_chain` | 2 叶 root = `b137985ff4...` |
| `root_three_leaves_odd_duplicates_last` | 3 叶（奇数）root = `e9636069c7...` |
| `root_four_leaves_balanced` | 4 叶 root = `33376a3bd6...` |
| `root_five_leaves_odd_duplicates_last` | 5 叶 root = `605c72ca93...` |
| `root_32byte_leaves_match_chain` | 32-byte 哈希叶子 = `03938e2c8f...` |
| `round_trip_with_chain_algorithmic_spec` | 用 sha2 crate 重写算法验证 helper 自身没漂 |
| `fixture_table_matches_chain_for_full_sweep` | 表驱动全 sweep，注释引用 Go 测试名 |

#### Go 侧（`chain/x/pole/types/merkle_test.go`，新建）

| 测试 | 验证内容 |
|------|---------|
| `TestMerkleLeafFromRecord_Sha256DomainSeparator` | `{X:1}` JSON 编码 → `sha256(0x00 \|\| ...) = f807460fcf...` |
| `TestMerkleRootFixtures`（6 子测试） | empty / 1 / 2 / 3 / 4 / 5 叶，同上 Rust 表的期望值 |
| `TestVerifyMerkleProofHex_RejectsBadInput` | 出范围 index / 空 proof 都被拒 |

所有期望值用 Python 独立计算：

```python
def leaf(b): return hashlib.sha256(b'\x00' + b).digest()
def parent(l, r): return hashlib.sha256(b'\x01' + l + r).digest()
def root(leaves):
    if not leaves: return b'\x00' * 32
    lvl = leaves[:]
    while len(lvl) > 1:
        nxt = []
        for i in range(0, len(lvl), 2):
            l = lvl[i]; r = lvl[i+1] if i+1 < len(lvl) else l
            nxt.append(parent(l, r))
        lvl = nxt
    return lvl[0]
```

完整脚本保留在 `.mavis/plans/merkle_fixtures.py` 供以后 re-run。

## 验证

```powershell
PS> cargo test --lib
test result: ok. 138 passed; 0 failed   (was 127 → +11 Merkle tests)

PS> cargo test --lib cosmos
test result: ok. 47 passed; 0 failed   (wire encoder 没受影响)

PS> go test ./...
ok  pole/chain/app         0.848s
ok  pole/chain/x/pole/keeper
ok  pole/chain/x/pole/types 0.300s   (was 0.115s, +3 Merkle tests)

PS> go vet ./...
(clean)

PS> gofmt -l chain/x/pole/types/
(clean after gofmt -w)

PS> cargo clippy --lib --tests
(改动文件 0 warning; pre-existing warnings in pole-client/pole-genesis 与本任务无关)
```

## 关键算法对比（修复前 vs 修复后）

| 输入 | 修复前 Rust（FNV-1a） | 修复后 Rust（sha256） | Go（sha256） | Rust ↔ Go |
|------|----------------------|----------------------|--------------|---------|
| 空树 | 全 0 | 全 0 | 全 0 | ✓ |
| 1 叶 `sha256(0x00\|\|"a")` | FNV-1a 值 | `022a6979...` | `022a6979...` | ✓ |
| 2 叶 `[a,b]` | FNV-1a 派生 | `b137985f...` | `b137985f...` | ✓ |
| 3 叶（奇数） | FNV-1a 派生 | `e9636069...` | `e9636069...` | ✓ |
| 4 叶 | FNV-1a 派生 | `33376a3b...` | `33376a3b...` | ✓ |
| 5 叶 | FNV-1a 派生 | `605c72ca...` | `605c72ca...` | ✓ |

**所有 6 个 fixture 现在 byte-identical**——这是 V1必修 e2e 缺口 #3 关闭的硬证据。

## 已知局限（与 #3 任务范围相关）

### 1. Rust 端 leaf 编码仍用 borsh，chain 端用 json.Marshal

> **更新（checklist 1.1，本轮）**：以下"by design 隔离"在 `FinalizeEpoch` 交叉点被打破——链端 `ValidateEpochRoots` 会用 json 叶重算 rewards/aggregates root 并与 Rust 提交的 borsh 叶 root 比对，Rust 构造的 CommitEpoch 必然 Finalize 失败。本轮已修复：
> - Rust `reward_record_root`/`aggregate_record_root` 改用**链式 json 叶**（`node_pipeline.rs::reward_record_to_chain_json`/`aggregate_record_to_chain_json`，与 Go `json.Marshal` 字节一致），并按链端 store key 序排序；
> - 链端奖励 root 检查条件化（链上无奖励记录时接受 proposer 承诺，aggregates 仍强制）；
> - `UpsertAggregateRecord` 后刷新 aggregates 承诺。
> 跨语言 golden 双侧锁定（Go `merkle_cross_language_test.go` ↔ Rust fixture 测试，同一 root hex）。

这是**设计选择**，不是 bug：

- **Rust off-chain**：`borsh::to_vec(record)` —— 紧凑、定长、可推导 schema，适合 P2P 传输和持久化
- **Chain 端 handler**：`json.Marshal(record)` —— Go 生态标准、可读、跨语言

`MerkleLeafFromRecord` 在 chain 端对 `Challenge.evidence` 验证时是**独立重算**——它接受 user 提交的 record bytes，自己 json.Marshal 算 leaf。这意味着：
- Rust 提交到链的 `MerkleCommitment.root` 是基于 borsh leaves 的 sha256 root
- Chain 存储这个 root，不重算（链只在用户**挑战**时才重算 root，那时用 chain 自己的 json leaves）
- 所以**链端 root 跟 off-chain root 永远不等**——这是 by design

### 2. 这对实际业务流的影响

| 场景 | 是否会触发 Merkle 验证 |
|------|----------------------|
| SubmitBatch 提交 batch commit | ❌ 不验证 root，链端只存 |
| CommitEpoch 提交 epoch commit | ❌ 同上 |
| 用户发起 BadBatch Challenge | ✅ 链端用 `ValidateMerkleProofHex` 重算 root，跟链上**已存**的 root 比对——但这要求用户提交的 record 是 json 编码的，**不依赖 off-chain Rust 的 root 计算** |
| 用户用 Rust off-chain 工具本地验证 | ✅ Rust sha256 算法跟 chain sha256 算法一致，本地算的 proof 可以直接给 chain 用 |

**结论**：本任务关闭的是"算法形状对齐"，off-chain Rust 跟 chain 的 root 值在 borsh-vs-json 不同 leaf encoding 下仍然不等，但这是 by design 隔离的两条路，不影响 V1 发布。

### 3. 如果未来要 off-chain Rust 算的 root 跟 chain 完全相等

需要做一项 follow-up：
- Rust off-chain 把 record encoding 从 borsh 切到 serde_json
- 改 5 个 caller 的 `borsh::to_vec` → `serde_json::to_vec`
- 验证 Rust 端跟 Go 端 `json.Marshal` 字段顺序一致（Go 按 struct 声明顺序，Rust serde_json 也按 struct 字段顺序——一般 OK 但需 fixture 验证）

这条路径**不在本次任务范围**（用户也没要求），留作后续。

## 文件清单

| 文件 | 改动 | 净增 |
|------|------|------|
| `src/node_pipeline.rs` | 改 `merkle_root` body + 新增 `merkle_leaf_sha256` + 11 个测试 + import sha2 | +200 / -8 |
| `src/node_rewards.rs` | 切到 `merkle_leaf_sha256` | +1 / -1 |
| `src/node_runtime.rs` | 切 3 处到 `merkle_leaf_sha256` + 更新 import | +4 / -4 |
| `src/node_verifier.rs` | 切 1 处 + 更新 import | +2 / -2 |
| `src/node_aggregator.rs` | 切 2 处 + 更新 import（删 stable_hash32 import） | +2 / -3 |
| `chain/x/pole/types/merkle_test.go` | 新建（3 个测试 + fixture 表 + smoke） | +117 / 0 |

总计 **+326 / -18**（commit `069a3e7`）。

## 当前 git 状态

```
[main 09d5700] fix(cosmos): wire MsgOpenChallenge through real proto3 encoder
[main f027c9d] feat(cosmos): wire remaining 8 Msg types via proto3 encoders
[main 069a3e7] fix(cosmos+chain): align Rust Merkle with chain sha256 + 0x00/0x01 domain separators
```

origin/main 仍滞后 3 个 commit，未推送（按规范不主动 push）。

---

**V1必修 e2e 4 个缺口的更新**：

| # | 缺口 | 状态 |
|---|------|------|
| 1 | MsgOpenChallenge proto3 wire | ✅ commit `09d5700` |
| 2 | 剩余 8 Msg proto3 wire | ✅ commit `f027c9d` |
| 3 | Merkle 跨语言对账 | ✅ **本轮 commit `069a3e7`** |
| 4 | poled daemon 启动 | ❌ 未动 |

剩 1 个 e2e 缺口 + 6 项 V1 必修硬卡点（MSI UpgradeCode / stable.json 签名 / 文档路径 / tools/wix 入库 / License 双向同步 / core2 hash）。