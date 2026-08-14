# MsgOpenChallenge proto3 wire encoder — V1 必修 #1 落地报告

> **结论一句话**：原 plan_a0080114 的 worker 在被 abort 之前其实把代码写完了，只是没跑编译就跑不动。我接手后修了 1 个编译错误 + 1 个测试 tag 计数 + 1 个 lint 警告，`cargo test` 38/38（cosmos 模块）+ 118/118（workspace-wide）全过，`cargo clippy` 对修改文件 0 warning。

## TL;DR

- `src/cosmos/pole_msgs.rs::encode_msg_open_challenge` 已实现真 proto3 wire，**输出非空字节**（最小 BadBatch 案例 outer=193 bytes / inner=178 bytes）
- `BridgeMessage::OpenChallenge` 在 `src/cosmos/tx_builder.rs:67-70` + `95-101` 已 dispatch 到新编码器
- 5 个 `ChallengeKind` 变体（BadBatch/Omission/BadAggregate/BadReward/BadStorage）各 1 个单测全过
- 内嵌 `Challenge`（14 字段）+ `ChallengeEvidenceRef`（6 字段含 repeated merkle_proof）完整编码

## 背景

之前 plan 任务说"`encode_msg_open_challenge` 是 dead-code stub 返回 `vec![]` 占位"，但我接手读代码时发现 stub 已经不存在了——文件里已经有完整实现。证据链：

1. `git status` 报 `src/cosmos/pole_msgs.rs` 和 `src/cosmos/tx_builder.rs` 都 modified（未 commit）
2. `git diff --stat`：pole_msgs.rs +736/-207，tx_builder.rs +121/-107
3. `git log` 最近 13 个 commit 没有任何"OpenChallenge wire"主题的提交
4. 唯一解释：**前一轮 plan_a0080114 的 worker session 在被 abort 之前写完了代码，但没跑 cargo test 验证**——然后引擎在 deliverable.md 没出现的情况下强制 abort

所以我接手做的不是"从零写编码器"，而是"把现成代码跑通 + 收尾"。

## 改了什么

### A. worker 写完、我修编译错误（1 处）

**`src/cosmos/pole_msgs.rs:463`**：测试里 `decode_varint` 返回 `(u64, usize)` 元组，原代码
```rust
13 + hdr_len + inner_len == any.value.len()
```
把 `inner_len (u64)` 直接加 `usize`，编译失败。改成 `inner_len as usize`。

### B. 我修 lint 警告（1 处）

**`src/cosmos/pole_msgs.rs:326`**：测试模块顶部 `use crate::primitives::{ChallengeState, EpochId, Height};` 中 `EpochId` 和 `Height` 未使用，触发 `unused_imports` warning。改成 `use crate::primitives::ChallengeState;`。

### C. 我修测试逻辑错误（1 处）

**`src/cosmos/pole_msgs.rs::open_challenge_evidence_emits_repeated_merkle_proof`**：
原 assertion 数裸字节 `0x2A`：
```rust
let tag_count = any.value.iter().filter(|b| **b == 0x2A).count();
assert_eq!(tag_count, 3, ...);
```
但 `0x2A` 也是 inner field5 `challenger`（字符串）tag——`(5 << 3) | 2 = 0x2A`——所以实际有 5 个 `0x2A` 出现（1 inner challenger + 3 merkle proof + 1 测试值里其他巧合），3 个 assertion 永远挂。改成唯一组合 `[0x2A, 0x40]`（tag + 64-char 长度前缀，仅 32-byte hash 的 hex 表示满足）：
```rust
let tag_count = any.value.windows(2).filter(|w| w == &[0x2A, 0x40]).count();
assert_eq!(tag_count, 3, ...);
```
注释里写明歧义来源。

### D. worker 写的功能代码（我没动）

- `encode_msg_open_challenge(challenger_bech32, &Challenge) -> Any`：外层 field1 challenger + field2 nested Challenge，type_url=`/pole.chain.pole.v1.MsgOpenChallenge`
- `encode_challenge_inner`：14 字段完整编码（challenge_id_hex/kind/epoch_id/target_address/challenger/bond_amount/opened_at_height/deadline_height/state/evidence/slash_amount/challenger_reward/resolution_summary/target_cons_address）
- `encode_evidence_inner`：6 字段（batch_root/aggregate_root/reward_root/payload_cid/repeated merkle_proof/aggregate_app_id），repeated merkle_proof 一个一个 emit tag+len
- `challenge_kind_to_proto`：Rust 0-based → proto 1-based 偏移（BadBatch=1..BadStorage=5）
- `challenge_state_to_proto`：Rust 5 状态 → proto 3 状态（Open/Responded→OPEN=1；Succeeded→RESOLVED=2；Rejected/Expired→REJECTED=3，Expired 走 RESOLVED-with-summary 的链端约定）
- `BridgeMessage::OpenChallenge`：变体从 `{ challenger, epoch_id }` 改成 `{ challenger, challenge: Challenge }`
- `BridgeMessage::to_any`：OpenChallenge 分支调 `encode_msg_open_challenge(&challenger.bech32, challenge)`

### E. 测试覆盖（13 个测试，全过）

`src/cosmos/pole_msgs.rs` 测试模块新增/修改：
- `varint_encoding_matches_proto_spec`
- `finalize_epoch_encodes_to_expected_bytes`
- `claim_reward_handles_empty_recipient`
- `message_encoder_trait_is_implementable`
- `open_challenge_emits_non_empty_wire_bytes`（regression 测试，防回退到 stub）
- `open_challenge_outer_wire_layout_matches_proto`
- `open_challenge_inner_carries_kind_varint`
- `open_challenge_kind_bad_batch_value_is_1`
- `open_challenge_kind_omission_value_is_2`
- `open_challenge_kind_bad_aggregate_value_is_3`
- `open_challenge_kind_bad_reward_value_is_4`
- `open_challenge_kind_bad_storage_value_is_5`
- `open_challenge_golden_vector_bad_batch`（精确 byte-level 校验，含 14 字段 wire 拆解注释）
- `open_challenge_evidence_emits_repeated_merkle_proof`

`src/cosmos/tx_builder.rs` 测试模块新增：
- `open_challenge_emits_well_formed_proto_any`（端到端：构造 BridgeMessage::OpenChallenge → to_any → 校验 type_url + 非空 + 首字节 0x0A）

## 验证

### 单元测试

```powershell
PS> cargo test --lib cosmos
test result: ok. 38 passed; 0 failed
```

```powershell
PS> cargo test --lib
test result: ok. 118 passed; 0 failed
```

### Lint

```powershell
PS> cargo clippy --all-targets
# (warehouse-level warning 16 处，全在 pole-client/pole-genesis/pole-sbom 与 OpenChallenge 无关)
```

`pole_msgs.rs` 和 `tx_builder.rs` 在 clippy 下 0 新 warning。

### 实测字节长度（BadBatch 最小案例）

`open_challenge_golden_vector_bad_batch` 测试输出：
- **outer（Any.value）= 193 bytes**
- **inner（nested Challenge）= 178 bytes**

按字段拆解（与文件内 wire 注释一致）：
| 字段 | wire tag | 字节 |
|------|---------|------|
| outer field1 challenger `"cosmos1abc"` | `0x0A` | 12 |
| outer field2 challenge (length-prefix) | `0x12` + varint | 2 |
| inner field1 challenge_id_hex `"aa"*32` | `0x0A,0x40` + 64 | 66 |
| inner field2 kind=1 | `0x10,0x01` | 2 |
| inner field3 epoch_id=42 | `0x18,0x2A` | 2 |
| inner field4 target_address `"11"*32` | `0x22,0x40` + 64 | 66 |
| inner field5 challenger `"cosmos1abc"` | `0x2A,0x0A` + 10 | 12 |
| inner field6 bond_amount=1000 | `0x30,0xE8,0x07` | 3 |
| inner field7 opened_at_height=100 | `0x38,0x64` | 2 |
| inner field8 deadline_height=200 | `0x40,0xC8,0x01` | 3 |
| inner field9 state=1 | `0x48,0x01` | 2 |
| inner field10 evidence (empty) | `0x52,0x0A` + 10 | 12 |
| inner field11 slash_amount=0 | `0x58,0x00` | 2 |
| inner field12 challenger_reward=0 | `0x60,0x00` | 2 |
| inner field13 resolution_summary=`""` | `0x6A,0x00` | 2 |
| inner field14 target_cons_address=`""` | `0x72,0x00` | 2 |

加和：12 (outer challenger) + 2 (outer length-prefix) + 178 (inner) = 192 ≈ 193 ✓（+1 来自 outer field2 varint 长度字节计数差异；wire 拼装实测一致）

## proto 字段映射（Rust ↔ chain）

| proto 字段 (tx.proto:71-76, state.proto:139-154) | Rust 来源 | 编码方式 |
|-----------------------------------------------|----------|---------|
| MsgOpenChallenge.challenger (1) | `challenger_bech32` 参数 | string, bech32 |
| MsgOpenChallenge.challenge (2) | `&Challenge` 参数 | nested length-delimited |
| Challenge.challenge_id_hex (1) | `challenge.challenge_id: [u8;32]` | string, hex |
| Challenge.kind (2) | `challenge.kind: ChallengeKind` | int32 varint, 1-based 偏移 |
| Challenge.epoch_id (3) | `challenge.epoch_id: u64` | uint64 varint |
| Challenge.target_address (4) | `challenge.target_node: Option<[u8;32]>` | string, hex, "" when None |
| Challenge.challenger (5) | `challenger_bech32` 参数（与 outer 同步）| string, bech32 |
| Challenge.bond_amount (6) | `challenge.bond: u128` → `as u64` | uint64 varint（low 64 bits） |
| Challenge.opened_at_height (7) | `challenge.opened_at_height: u64` → `as i64` | int64 varint |
| Challenge.deadline_height (8) | `challenge.deadline_height: u64` → `as i64` | int64 varint |
| Challenge.state (9) | `challenge.state: ChallengeState` | int32 varint, 3 状态折叠 |
| Challenge.evidence (10) | `challenge.evidence: ChallengeEvidenceRef` | nested length-delimited |
| Challenge.slash_amount (11) | hardcoded 0 at open time | uint64 varint |
| Challenge.challenger_reward (12) | hardcoded 0 at open time | uint64 varint |
| Challenge.resolution_summary (13) | hardcoded "" at open time | string, empty |
| Challenge.target_cons_address (14) | hardcoded "" at open time | string, empty |
| ChallengeEvidenceRef.batch_root_hex (1) | `evidence.batch_root: Option<[u8;32]>` | string, hex, "" when None |
| ChallengeEvidenceRef.aggregate_root_hex (2) | `evidence.aggregate_root: Option<[u8;32]>` | string, hex, "" when None |
| ChallengeEvidenceRef.reward_root_hex (3) | `evidence.reward_root: Option<[u8;32]>` | string, hex, "" when None |
| ChallengeEvidenceRef.payload_cid (4) | `evidence.payload_cid: Option<String>` | string, "" when None |
| ChallengeEvidenceRef.merkle_proof_hex (5) | `evidence.merkle_proof: Vec<[u8;32]>` | repeated string, one tag each |
| ChallengeEvidenceRef.aggregate_app_id (6) | not yet in Rust struct | uint32 varint, hardcoded 0 |

## 已知局限

1. **u128 bond 截断到 u64**：Rust `Challenge.bond: u128` 在编码时 `as u64` 取低 64 位。如果未来 bond 真的超过 2^64，需要扩展 proto 字段或拆 high/low 两个 uint64。现状：PoLE 白皮书 V1 阶段 bond 在 `1_000_000 upole` 量级，远低于阈值。
2. **aggregate_app_id 没接 Rust 字段**：`ChallengeEvidenceRef.aggregate_app_id (6)` 在 Rust struct 里没有对应字段，编码器 hardcode 0。等 `records::ChallengeEvidenceRef` 加这个字段后，需要联动改 `encode_evidence_inner`。
3. **slash_amount / challenger_reward / resolution_summary 在 OpenChallenge 阶段全 0**：`OpenChallenge` 不携带结算信息，这些字段由链端 `ResolveChallenge` handler 后续写入。当前编码器直接 hardcode 0 / ""，链端 deserialize 不会因 missing-field default 出错。
4. **没有 e2e 跑通 poled**：poled 当前不是 daemon（cmd/poled/main.go 缺失，V1 必修），无法在真链上验证 OpenChallenge tx 的接受路径。Rust 侧所有验证只能到 TxRaw proto bytes 层面，跟 `chain/x/pole/keeper/msg_server.go::OpenChallenge` 的字段名/wire type 一致性靠**代码对照 + 单元测试**保证。

## 没动的东西（按要求）

- `src/bin/pole-client.rs` 的 `open-challenge` 子命令入口
- `src/transitions.rs::apply_open_challenge` 链下状态机
- `chain/x/pole/keeper/msg_server.go::OpenChallenge` 链端 handler

## 文件清单

| 文件 | 改动 |
|------|------|
| `src/cosmos/pole_msgs.rs` | +736/-207（worker 写主体 + 我修 3 处编译/lint/测试） |
| `src/cosmos/tx_builder.rs` | +121/-107（worker 改 BridgeMessage::OpenChallenge 变体 + dispatch） |

## 下一步建议（不阻塞 V1 #1 验收）

1. **commit 当前 working tree 改动**——两个文件还 modified 没入库，commit message 建议：`fix(cosmos): replace OpenChallenge wire stub with real proto3 encoder`。等用户拍板是否要 commit。
2. **链端 round-trip 验证**：等 `cmd/poled/main.go` 修了之后，可以写一个 `tests/integration_open_challenge.rs`，启动 poled + 跑一次完整的 OpenChallenge tx 提交 + `GetChallenge` 查询，断言链端反序列化结果与 Rust 输入字段一致。
3. **剩下 8 个 Msg 的 wire stub**：现状 FinalizeEpoch / ClaimReward / OpenChallenge 三个有真编码器，剩下 UpsertNode / UpsertAggregateRecord / SubmitBatch / SubmitReplicaReceipt / CommitEpoch / ResolveChallenge / UpsertGameWeight / UpdateParams 8 个还走 `BridgeMessage::Unsupported` 兜底。V1 必修 e2e 缺口里有 4 个，这是其中 #1，剩下的需要单独排期。