# 剩余 8 个 Msg proto3 wire encoder — 落地报告

> **结论一句话**：本次会话内把 V1必修 e2e 缺口 #1 的剩余 8 个 Msg（UpsertNode / UpsertAggregateRecord / SubmitBatch / SubmitReplicaReceipt / CommitEpoch / ResolveChallenge / UpsertGameWeight / UpdateParams）全部接通真 proto3 wire encoder。新增 `wire_types.rs` 模块承载 11 个 wire-only 类型，配套 9 个新单测 + 改动 `pole_msgs.rs` / `tx_builder.rs`，`cargo test` 127/127、clippy 改动文件 0 warning。

## TL;DR

- 新增 `src/cosmos/wire_types.rs`（11 个 wire 类型 + 1 个 enum 映射 helper）
- 新增 8 个 `encode_msg_*` 函数 + 11 个 `encode_*_inner` helper
- `BridgeMessage` 枚举从 4 个变体扩到 **12 个变体**（原 3 + Unsupported + 8 新）
- 9 个新单元测试覆盖每个 encoder 的非空性、type_url、首字节 0x0A；UpsertNode 加 3 变体枚举值映射测试、CommitEpoch 加 5 nested MerkleCommitment tag 测试、UpdateParams 加末字段 varint 定位测试
- 总工作量：4 文件改动，+904/-7（commit `f027c9d`）

## 背景

接续 `09d5700`（MsgOpenChallenge wire encoder）。本次扫剩余 8 个 Msg 时发现两个事实，改变了实现策略：

1. **Rust 侧 3 个 wire 类型缺失**：`NodeRecord` / `NodeRole` / `NodeCapabilitySet` / `GameWeightEntry` 在 src/ 里都不存在。
2. **4 个 off-chain records 跟 proto 字段不1:1**：
   - `records::BatchCommit` 8字段，proto `BatchCommit` 8字段，但**字段名差异**（`collector_id` vs `collector_address`、`obs_count` vs `observation_count`）
   - `records::ReplicaReceipt` 5字段，proto 6字段，缺 `receipt_hash_hex`
   - `records::EpochCommit` 10字段，proto 12字段，缺 `finalized` + `total_network_weight_units`
   - `records::AggregateRecord` 15字段（含 GVS tier 权重），proto 4字段
   - `params::ProtocolParams` 嵌套 4 个子结构，proto `Params` 21 平铺 primitive

策略选择（用户拍板：**wire-only-now**）：
- 新建 `src/cosmos/wire_types.rs`，定义 11 个 1:1 对齐 proto 的 wire 类型
- `BridgeMessage` 变体直接持 wire 类型（不走 off-chain records）
- **适配层（records → wire）留作 follow-up 技术债**

## 改了什么

### A. 新增 `src/cosmos/wire_types.rs`（11 个 wire 类型 + 1 helper）

| 类型 | proto 来源 | 字段数 |
|------|-----------|--------|
| `MerkleCommitmentWire` | state.proto:7-10 | 2 |
| `NodeRoleWire` (enum) | state.proto:12-17 | 3 变体 (PLAYER/SERVICE/COORDINATOR) |
| `NodeCapabilitySetWire` | state.proto:19-24 | 4 bool |
| `NodeRecordWire` | state.proto:26-35 | 8 |
| `AggregateRecordWire` | state.proto:92-97 | 4 |
| `BatchCommitWire` | state.proto:61-70 | 8 (含 1 nested MerkleCommitment) |
| `ReplicaReceiptWire` | state.proto:163-170 | 6 |
| `EpochCommitWire` | state.proto:99-112 | 12 (含 5 nested MerkleCommitment) |
| `GameWeightEntryWire` | state.proto:156-161 | 4 |
| `ParamsWire` | state.proto:37-59 | 21 平铺 primitive |
| `node_role_to_proto()` helper | — | 1-based 偏移映射 |

所有 wire 类型字段命名严格对齐 proto（snake_case），字段顺序按 proto field number 排列。`ParamsWire` 实现了 `Default::default()` 返回全 0（21 个字段都默认 0）。

### B. 新增 8 个 `encode_msg_*` 函数（`src/cosmos/pole_msgs.rs`）

```rust
encode_msg_upsert_node(operator_bech32, &NodeRecordWire) -> Any
encode_msg_upsert_aggregate_record(operator_bech32, &AggregateRecordWire) -> Any
encode_msg_submit_batch(collector_bech32, &BatchCommitWire) -> Any
encode_msg_submit_replica_receipt(storer_bech32, &ReplicaReceiptWire) -> Any
encode_msg_commit_epoch(proposer_bech32, &EpochCommitWire) -> Any
encode_msg_resolve_challenge(resolver_bech32, challenge_id_hex, slash_amount, challenger_reward, resolution_summary, final_state, slash_fraction_bps, jail_validator) -> Any  // 8 flat fields
encode_msg_upsert_game_weight(authority_bech32, &GameWeightEntryWire) -> Any
encode_msg_update_params(authority_bech32, &ParamsWire) -> Any
```

外层结构统一：`field1 = signer bech32 string`，`field2 = length-delimited nested message`。例外是 `encode_msg_resolve_challenge` —— proto 里就是 flat 8 字段（没有 nested），直接平铺写。

11 个 inner encoder helper（私有函数）处理嵌套消息：

```rust
encode_node_record_inner + encode_node_capability_set_inner
encode_aggregate_record_inner
encode_batch_commit_inner + encode_merkle_commitment_inner
encode_replica_receipt_inner
encode_epoch_commit_inner (含 5 nested MerkleCommitment)
encode_game_weight_entry_inner
encode_params_inner (21 字段)
```

### C. `BridgeMessage` 加 8 个变体 + dispatch（`src/cosmos/tx_builder.rs`）

从 4 个变体扩到 **12 个**：

```rust
pub enum BridgeMessage {
    FinalizeEpoch { finalizer: CosmosAddress, epoch_id: EpochId },
    ClaimReward { claimer, epoch_id, recipient },
    OpenChallenge { challenger, challenge: Challenge },
    UpsertNode { operator: CosmosAddress, node: NodeRecordWire },                  // NEW
    UpsertAggregateRecord { operator, aggregate_record: AggregateRecordWire },    // NEW
    SubmitBatch { collector, batch_commit: BatchCommitWire },                     // NEW
    SubmitReplicaReceipt { storer, replica_receipt: ReplicaReceiptWire },         // NEW
    CommitEpoch { proposer, epoch_commit: EpochCommitWire },                      // NEW
    ResolveChallenge { resolver, challenge_id_hex, slash_amount, ... },           // NEW (8 fields)
    UpsertGameWeight { authority, entry: GameWeightEntryWire },                   // NEW
    UpdateParams { authority, params: ParamsWire },                               // NEW
    Unsupported { type_url, note },                                               // (fallback)
}
```

`to_any()` 增加 8 个 match arm，每个分发到对应 encoder。

### D. 9 个新单元测试

每个 encoder 1 个 smoke test（type_url + 非空 + 首字节 0x0A）：
- `upsert_node_emits_non_empty_wire_bytes`
- `upsert_aggregate_record_emits_non_empty_wire_bytes`
- `submit_batch_emits_non_empty_wire_bytes`
- `submit_replica_receipt_emits_non_empty_wire_bytes`
- `commit_epoch_emits_non_empty_wire_bytes`（同时验证 5 nested MerkleCommitment outer tags 0x12/0x1A/0x22/0x2A/0x32）
- `resolve_challenge_emits_non_empty_wire_bytes`（同时验证 final_state tag+varint 0x30 0x02=RESOLVED）
- `upsert_game_weight_emits_non_empty_wire_bytes`
- `update_params_emits_non_empty_wire_bytes`（同时验证末字段 governance_burn_bps tag+varint）

额外测试：
- `upsert_node_role_enum_maps_to_proto_varints` —— 3 个 NodeRoleWire 变体各跑一次，断言 inner field4 tag 0x20 + 正确 varint（1/2/3）

## 验证

```powershell
PS> cargo test --lib cosmos
test result: ok. 47 passed; 0 failed   (was 38 → +9 new)

PS> cargo test --lib
test result: ok. 127 passed; 0 failed  (was 118 → +9 new)

PS> cargo clippy --lib --tests
# (改动文件 0 warning；pre-existing warnings in pole-client/pole-genesis/pole-sbom 与本任务无关)
```

## proto 字段映射总表

| proto Msg | 字段数 | Wire 类型 | Rust encoder | 关键不变量 |
|-----------|--------|----------|-------------|-----------|
| MsgUpsertNode | 2 outer + 8+4 nested | NodeRecordWire + NodeCapabilitySetWire | `encode_msg_upsert_node` | operator == node.operator_address（构造期由 caller 负责） |
| MsgUpsertAggregateRecord | 2 outer + 4 nested | AggregateRecordWire | `encode_msg_upsert_aggregate_record` | （caller 校验） |
| MsgSubmitBatch | 2 outer + 8+2 nested | BatchCommitWire + MerkleCommitmentWire | `encode_msg_submit_batch` | collector == batch.collector_address；payload_cid 非空；observation_count > 0；slot_start ≤ slot_end |
| MsgSubmitReplicaReceipt | 2 outer + 6 nested | ReplicaReceiptWire | `encode_msg_submit_replica_receipt` | storer == receipt.storer_address；payload_cid 非空 |
| MsgCommitEpoch | 2 outer + 12+2×5 nested | EpochCommitWire + MerkleCommitmentWire | `encode_msg_commit_epoch` | proposer == commit.proposer_address；challenge_deadline_height > challenge_open_height |
| MsgResolveChallenge | 8 flat (no nested) | （直接 flat 编码） | `encode_msg_resolve_challenge` | final_state != UNSPECIFIED；authority chain 校验 resolver |
| MsgUpsertGameWeight | 2 outer + 4 nested | GameWeightEntryWire | `encode_msg_upsert_game_weight` | authority chain 校验；game_weight_ppm > 0 |
| MsgUpdateParams | 2 outer + 21 nested | ParamsWire | `encode_msg_update_params` | authority chain 校验 |

## 已知局限（与 #1 一致）

1. **chain-side 覆盖字段仍编码**：MsgUpsertNode 的 `bonded_tokens`、MsgSubmitBatch 的 `submitted_at_height`、MsgCommitEpoch 的 `challenge_open_height` 链端会用 `ctx.BlockHeight()` 或 validator.Tokens.Uint64() 覆盖。Rust 编进去是浪费几个字节的 varint，无害。
2. **wire-only 类型双维护**：调用方需要从 off-chain records（如 `BatchCommit`）构造 `BatchCommitWire`，需要手填字段映射。**没有自动 From 适配器**——follow-up 工作。
3. **field 编号 ≥ 16 的 tag 是 2 字节 varint**：proto3 wire format 规定 tag 自身是 varint，field 16+ 时 tag 跨字节（因为 `(16<<3)|0 = 128` 的 MSB = 1）。encoder 正确处理（直接走 `encode_tag` → `encode_varint`），但写测试时容易踩坑——`update_params` 测试第一次失败就是因为搜了 `[0xA8, 0x64]`（field 21 tag 当单字节算）而不是 `[0xA8, 0x01, 0x64]`（field 21 tag 是 2 字节 varint + 100 varint）。
4. **无 e2e 验证**：poled 当前不是 daemon（`cmd/poled/main.go` 缺失，V1必修），无法跑真链 SubmitBatch/CommitEpoch 验证。仅靠字段映射 + Go handler 校验对照保证。

## 后续工作（不阻塞本任务）

1. **records → wire 适配器**：写 `From<&records::BatchCommit> for BatchCommitWire` 等 4-5 个 adapter，让调用方一处真相。
2. **poled daemon 启动**：cmd/poled/main.go 修了之后，可以写 `tests/integration_open_challenge.rs` 类 e2e 跑全 11 个 Msg 的提交 + 查询。
3. **V1必修其他 e2e 缺口**：剩 3 个（来自 deliverable-overview.md §7.1）：Merkle 跨语言对账、BeginBlock/EndBlock 钩子挂上、poled daemon 启动。
4. **修复 pole-cosmos-expert persona marker**：`saw_persona_marker: NO` 红旗还在，下次起 plan 前最好先补 PERSONA.md 让 daemon 校验通过。

## 文件清单

| 文件 | 改动 | 净增 |
|------|------|------|
| `src/cosmos/wire_types.rs` | 新建 | +207 |
| `src/cosmos/pole_msgs.rs` | +557/-7（追加 8 encoder + 11 helper + 9 tests） | +550 |
| `src/cosmos/tx_builder.rs` | +123/-?（8 新 BridgeMessage 变体 + 8 dispatch + 0 新测试） | +123 |
| `src/cosmos/mod.rs` | +1 行（`pub mod wire_types;`） | +1 |

总计 **+904/-7**（commit `f027c9d`）。

## 当前 git 状态

```
[main 09d5700] fix(cosmos): wire MsgOpenChallenge through real proto3 encoder   ← 上一轮
[main f027c9d] feat(cosmos): wire remaining 8 Msg types via proto3 encoders    ← 本轮
```

origin/main 仍滞后 2 个 commit，未推送（按规范不主动 push）。