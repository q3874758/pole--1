# PoLE 重构计划：节点相互证明 + 双边奖励 + 无上限发行 + 销毁保值

**目标（用户口径）：** 各个节点能够相互证明确认某个节点真的玩了游戏、没有作假；给「玩游戏的节点」和「做证明的节点」都发奖励；奖励没有上限，但会随着各种活动销毁来保证一定的价值。

**状态：** 全部落地（P1 / P1.5 / P2 / P3 / P4 / P5 已完成并验收）。
**基线：** `0.1.2`（main @ `6491838`），Rust 416 测试 / Go 全绿 / fmt+clippy 零告警。
**收口实测：** `cargo test --all-targets` = 433 收集 / 432 passed / 1 ignored / 0 failed；`cd chain && go test ./... -count=1` 全绿；`cargo test --features integration -- --test-threads=1` = 4 passed。

---

## 0. 一页结论

> 以下差距表记录的是**动工前**的现状（2026-10-02 基线），保留作为改动依据；实施后的收口状态见 §4 各阶段的「已落地」标记与 §5 验收矩阵。

用户要的四件事，现状是「两件半没做、一件做反了」：

| 用户要的 | 现状 | 判定 |
|---|---|---|
| 节点**相互**证明某节点玩了游戏 | 只有单向 `MsgVerifyBatch`（verifier → collector 的 batch 根），且 Rust 端把 target 填成自己 → 链上必然拒绝 | 🔴 端到端断裂 |
| 给**玩游戏的**节点发奖 | 链上没有任何「某节点在本 slot 玩了 N 秒」的对象；`play_seconds` 只落本地 artifact | 🔴 对象缺失 |
| 给**做证明的**节点发奖 | `verify_pool` 整包塞给本节点自己；链上 `VerifyReward` 只在挑战结算时加 | 🔴 无实现路径 |
| 奖励**没有上限** | 年度预算 ÷ 12 = 月度硬上限，配额耗尽即停铸；池子空了 claim 直接失败 | 🔴 与诉求相反 |
| 靠**销毁**保值 | 4 条销毁通道只有 1 条接线；手续费通道链上根本没实现 | 🟠 四缺三 |

**一句话：** 协议骨架是对的（小时奖励主线 + 承诺/挑战/最终性 + 跨语言契约都扎实），但缺三样东西 —— **缺「节点在玩」这个对象**、**缺对等见证的独立性约束**、**缺奖励记录上链路径**。本计划就是把这三点接上，并按口径把发行上限拿掉、把销毁通道补齐。

---

## 1. 目标模型

### 1.1 术语（D1 已锁定：玩家 = 节点，不引入独立玩家身份）

| 角色 | 标识 | 说明 |
|---|---|---|
| **游玩节点** PLAYER | `node_address`（节点自身 bech32） | 在玩游戏的那个节点。它用自己的节点身份密钥签署游玩声明 |
| 采集节点 COLLECTOR | `collector_address` | 采集活跃度信号、产生 `Observation` / `BatchCommit` 的节点。**可以是游玩节点自己**（自采集） |
| **见证节点** WITNESS | `witness_address` | 独立观测同一 app 的活跃度并签 `WitnessAttestation` 的节点 |
| 提交节点 PROPOSER | `proposer_address` | 提交 epoch 承诺、发起最终化 |

因为玩家就是节点，**「给玩游戏的节点发奖」与「给做证明的节点发奖」是同一个 `RewardRecord` 的两个分项**（`player_reward` / `verify_reward`），收款人都是 node。这一点不需要改结构。

硬约束（链上强制）：

1. `witness ≠ player` —— 节点不能给自己的游玩声明作证（对应现有 `verifier cannot attest its own batch`）。
2. `witness ≠ collector` —— 不能为自己采集的批次作证。
3. `witness` 必须**自己在该 (app_id, epoch_id, slot_id) 上有链上观测记录**，且与游玩声明的观测值在容差内 —— 这是「独立观测」的可判定定义。
4. `witness` 的观测载荷 CID ≠ 游玩声明的观测载荷 CID —— 禁止抄同一份。

### 1.2 核心对象

```
PlaySession                节点游玩声明（游玩节点签）
  session_id             = H(node_address, app_id, epoch_id, slot_id, session_nonce)
  node_address             游玩节点（= 领奖主体）
  app_id / epoch_id / slot_id
  play_seconds             本 slot 内声明的有效游玩秒数（<= slot_seconds）
  collector_address        该信号的采集者（可等于 node_address）
  observation_cid          指向原始观测载荷（挑战窗口内可取回）
  session_signature        节点身份私钥签名

PlayHeartbeat              游玩心跳（可选强化，见 P1.5）
  session_id
  node_address
  bucket_index             声明游玩区间内的时间桶序号
  signed_at_millis
  signature

WitnessAttestation         见证声明（见证节点签）
  session_id
  witness_address
  observed_play_seconds    见证节点自己观测到的秒数
  witness_observation_cid  见证节点自己的观测载荷 CID（≠ session 的）
  attested_at_height
  witness_signature

SessionSettlement          链上判定结果（只由链计算，不接受外部传入）
  session_id
  valid                    witness_count >= min_witness_count
                           且 distinct_observation_count >= min_distinct_observations
                           且 heartbeat_count >= min_heartbeat_count（若启用 P1.5）
  player_weight_units      = play_seconds × game_weight_ppm
  witness_count / distinct_observation_count
  reward                   → 落 RewardRecord 的 player_reward / verify_reward
```

**「相互证明」的精确命题（这是整份计划的核心）：**

> 节点 A 声明「我在 epoch E 的 slot S 玩了 app X 共 N 秒」。节点 B、C 各自**独立观测到** app X 在同一 slot 的活跃度，并把 A 的声明与自己观测到的活跃度交叉核对后签名背书。链上要求至少 K 个不同节点、L 份不同观测载荷，才承认 A 的这次游玩。

它不是「B 相信 A 说的话」，而是「B 用自己的观测数据佐证 A 的说法」。这也是现有 `cross_validate_samples`（[src/activity_collector.rs:305](src/activity_collector.rs#L305)）从本机扩展到网络尺度的自然延伸。

### 1.3 奖励分配

单个 1 小时奖励区块内：

```
Hourly_Reward_Pool                      该小时固定奖励池（白皮书 §3.1 主线不变）
Player_Hour_Reward   = Pool × Σ(play_seconds_i × game_weight_i) / Total_Hour_Weight
Witness_Hour_Reward  = Verify_Pool × witness_credit_j / Σ witness_credit
Collect_Hour_Reward  = Collect_Pool × collect_score_k / Σ collect_score
```

- `Verify_Pool` 来自 `service_reward_allocation_bps`（10%）× `verify_reward_bps`（15%）= **发行量的 1.5%**（D3 锁定：复用现有参数，不新增池）。
- `witness_credit_j` = 该见证节点本 epoch 内**被采纳**的 attestation 数（去重后）。
- 玩家奖励与见证奖励分列 `RewardRecord` 的 `player_reward` / `verify_reward`（字段已有，语义修正）。

> ⚠️ **参数提示：** 见证奖励只占发行量的 1.5%，而玩家奖励占 80% —— 两者差 53 倍。若见证节点数量与玩家节点数量同量级，见证激励可能不足以支撑诚实验证。建议 P2 落地后做一次参数敏感性分析，必要时上调 `service_reward_allocation_bps` 或 `verify_reward_bps`（两者都是治理参数，无需改代码）。

### 1.4 发行与销毁（D2 锁定：完全无预算上限）

**发行：去掉一切预算上限，按已确认的结算额铸币。**

```
Emission(epoch) = Σ 该 epoch 所有已 finalize 的 session / witness / collect 奖励
```

`Hourly_Reward_Pool` 仍由治理参数 `base_hourly_reward` 控制，并保留已有的 ±cap 平方根负反馈（`AdjustedHourlyReward`）作为**单位奖励**调节器 —— 它调节的是「每份权重值多少钱」，不是「总共发多少钱」。这样白皮书 §3.1「小时奖励主线必须在任何实现、测试、审计和治理更新中保持不变」不被破坏。

**销毁：五条通道全部接线。**

| 通道 | 参数 | 触发点 | 现状 |
|---|---|---|---|
| 手续费销毁 | `fee_burn_bps` | ante handler | ❌ 链上无 ante handler |
| 奖励超额销毁 | `reward_burn_bps` / `reward_burn_threshold` | claim | ✅ 已接线 |
| 治理惩罚销毁 | `governance_burn_bps` | 提案保证金没收 | ❌ 无调用点 |
| 挑战保证金销毁 | 新增 `challenge_bond_burn_bps` | 挑战失败 / 恶意挑战 | ⚠️ 字段在，不扣款 |
| **虚假游玩惩罚销毁** | 新增 `session_slash_bps` | 挑战判定 session 无效 | ❌ 不存在 |

**净供给：** `Net Supply Change = Emission − Burn`。销毁量随网络活动浮动（活动越强 → 手续费越多 → 挑战越频繁 → 销毁越多），与无上限发行构成自动平衡。

---

## 2. 差距清单（带证据）

### 🔴 P0-1 链上没有「某节点在玩」这个对象

- `PlayerRewardBlockRecord.play_seconds` 只落本地 artifact — [src/node_rewards.rs:121-161](src/node_rewards.rs#L121-L161)
- `AggregateRecord` 只带 `median_players` / `total_weight_units`，不带节点个体 — [src/records.rs:48-63](src/records.rs#L48-L63)
- 链上 `ComputePlayerHourWeight(effectivePlaySeconds, gameWeightPPM)` **零调用点** — [chain/x/pole/types/reward_math.go:11](chain/x/pole/types/reward_math.go#L11)
- 链上 `ComputePlayerReward` 也无人调用 — [chain/x/pole/keeper/keeper.go:549](chain/x/pole/keeper/keeper.go#L549)
- **后果：** 协议目前无法回答「哪个节点在哪个小时玩了多少秒」——而这正是「相互证明」要证明的命题。玩家奖励因此只能由本机算、本机存，链上无法复算。

### 🔴 P0-2 「相互证明」是单向的，且当前实现必然被链拒绝

- 链上唯一证明消息是 `MsgVerifyBatch`，语义是「核对了 collector 的 batch 根」，不是「证明某节点玩了游戏」— [chain/proto/pole/chain/pole/v1/tx.proto:105](chain/proto/pole/chain/pole/v1/tx.proto#L105)
- 链上明确禁止自证：`verifier cannot attest its own batch` — [msg_server.go:360-362](chain/x/pole/keeper/msg_server.go#L360-L362)
- Rust 端构造凭据时把 target 填成自己：`target_collector_hex = config.node_id_hex`、`target_collector_address = verifier_address.clone()` — [src/node_daemon.rs:218-227](src/node_daemon.rs#L218-L227)
- `verify-batch-submit` 用同一个 identity 既当 verifier 又当 target collector — [src/cli_node.rs:1805-1821](src/cli_node.rs#L1805-L1821)
- **后果：** `pole-node verify-batch-submit` 当前 100% 返回 `verifier cannot attest its own batch`。端到端上「证明」这条路是断的，且测试没覆盖到（Go 侧测的是手搓的 `VerificationRecord`，不走 Rust 编码器）。

### 🔴 P0-3 「做证明的节点拿奖励」没有实现路径

- Rust 侧把整个 `verify_pool` 给本地节点自己，条件只是「本地开了 verify capability」，与外部 attestation 数量无关 — [src/node_rewards.rs:456-461](src/node_rewards.rs#L456-L461)
- Go 侧 `RewardRecord.VerifyReward` 只在挑战结算时被加 — [msg_server.go:648-658](chain/x/pole/keeper/msg_server.go#L648-L658)
- `VerificationRecord` 存了，但从不参与任何奖励计算 — [keeper.go:325-351](chain/x/pole/keeper/keeper.go#L325-L351)
- **后果：** 诚实做见证的节点拿不到见证奖励；反而只有「挑战成功者」拿钱。这与用户口径正相反。

### 🔴 P0-4 奖励记录没有上链路径 → 节点实际领不到奖

- 代码自认：reward records 只经 genesis / challenge 入库，「no live reward-record submission path」— [keeper.go:650-664](chain/x/pole/keeper/keeper.go#L650-L664)
- `ValidateEpochRoots` 因此对 rewards root 做条件检查：链上无记录时直接接受 proposer 的承诺 — [keeper.go:660-664](chain/x/pole/keeper/keeper.go#L660-L664)
- `ClaimReward` → `GetRewardRecord` 找不到就报错 — [msg_server.go:436-442](chain/x/pole/keeper/msg_server.go#L436-L442)
- **后果：** 奖励根只是「承诺」，链上没有任何可领记录。除被挑战结算过的 epoch 外，claim 必然失败。**这是「拿不到钱」的根本原因。**

### 🔴 P0-5 无上限发行与现状相反

- 年度预算 ÷ 12 = 月度配额，配额耗尽即停铸 — [emission.go:140-159](chain/x/pole/keeper/emission.go#L140-L159)
- `PayoutClaimedReward` 池子不足直接失败 — [keeper.go:781-789](chain/x/pole/keeper/keeper.go#L781-L789)
- 白皮书 §4.4.5 把月度配额写成「铸造硬上限」
- **后果：** 现状是「有硬上限」，与「奖励没有上限」直接冲突。

### 🟠 P1-1 销毁五缺四

- ✅ `reward_burn_bps` 已接线 + 有测试 — [keeper.go:767-795](chain/x/pole/keeper/keeper.go#L767-L795)
- ❌ `fee_burn_bps`：`chain/app/app.go` 无 `SetAnteHandler`（全仓 grep 零命中）→ 链上根本没有手续费扣减，参数是死的
- ❌ `governance_burn_bps`：无任何调用点
- ⚠️ 挑战保证金：`Challenge.BondAmount` 字段存在，但 `OpenChallenge` 不扣款、`ResolveChallenge` 不没收；Rust 本地状态机有 `burn_locked_bond` — [src/transitions.rs:1221](src/transitions.rs#L1221) — 但那只是离线模拟
- ❌ 虚假游玩惩罚：不存在

### 🟠 P1-2 Rust 生成的 genesis 把验证门禁清零

- `default_pole_params()` 未输出 `min_verification_count` / `min_player_verifier_share_bps` — [src/genesis_builder/mod.rs:271-300](src/genesis_builder/mod.rs#L271-L300)
- 链上默认是 `3` / `5000` — [chain/x/pole/types/helpers.go:49-50](chain/x/pole/types/helpers.go#L49-L50)
- Rust `ParamsWire` 已有这两个字段且已编码 — [pole_msgs.rs:639-643](src/cosmos/pole_msgs.rs#L639-L643) — 只是 genesis builder 没填 → proto3 零值 → **门禁被静默关闭**
- **后果：** 用 `pole genesis` 建链，`FinalizeEpoch` 的验证覆盖门禁等于没有 —— 恰好与「必须相互证明」相反。**这是全计划投入产出比最高的一处修复。**

### 🟡 P2-1 CLI 链上提交路径仍是骨架

- `submit-batch` / `submit-epoch` / `export-tx` 走 `chain_bridge`（自认 legacy skeleton，把 `serde_json` 塞进 base64，不是 proto3 wire）— [src/chain_bridge.rs:1-11](src/chain_bridge.rs#L1-L11)、[src/cli_client.rs:2557](src/cli_client.rs#L2557)、[:2598](src/cli_client.rs#L2598)
- 真实路径只有 `verify-batch-submit` 走 `CosmosClient::submit`
- **后果：** 三条命令产出的 tx 无法广播。

### 🟡 P2-2 见证独立性无约束

- `build_verification_credentials` 每个 batch 产一条，`target_batch_root_hex` 不同即视为不同凭据 — [src/node_daemon.rs:188-236](src/node_daemon.rs#L188-L236)
- 链上 `VerificationCoverage` 按 `verifier_address` 去重计数 — [keeper.go:353-375](chain/x/pole/keeper/keeper.go#L353-L375) — 但没有「见证者必须提交独立观测」的约束
- **后果：** N 个节点可以抄同一份观测，制造虚假见证覆盖。**这正是「没有作假」要防的核心攻击。**

---

## 3. 决策（已锁定）

| # | 决策 | 选定 | 影响 |
|---|---|---|---|
| **D1** | 玩家身份是否与节点身份分离 | **不分离：玩家 = 节点** | 不引入 `player_address`，不改 `NodeConfig`，`RewardRecord` 收款人保持 node_id。计划因此简化：原 P0-1「主体错位」不再是缺陷，替换为 P0-1「链上缺游玩对象」 |
| **D2** | 「没有上限」的精确含义 | **完全无预算上限，按已确认结算额铸币** | 删除月度配额与池子耗尽失败；`AdjustedHourlyReward` 的 ±cap 负反馈成为唯一的单位奖励调节器 |
| **D3** | 见证奖励资金来源 | **复用 `verify_reward_bps`（15%），按采纳的 attestation 数分配** | 不新增治理参数；把 `verify_pool` 从「本地自付」改成「按见证贡献分配」。注意激励占比仅 1.5%，见 §1.3 参数提示 |

---

## 4. 分阶段实施

### P1 — 打通「游玩声明 + 对等见证」（已落地）

**目标：** 链上能收到节点签名的游玩声明和见证节点的签名背书，并据此判定一次游玩是否成立。

**链上（Go）：**

- `chain/proto/pole/chain/pole/v1/state.proto`：新增 `PlaySession`、`WitnessAttestation`、`SessionSettlement`；`Params` 新增 `min_witness_count`(24)、`min_witness_observation_tolerance_ppm`(25)、`min_distinct_observations`(26)、`session_slash_bps`(27)
- `chain/proto/pole/chain/pole/v1/tx.proto`：新增 `MsgSubmitPlaySession`、`MsgAttestSession`、`MsgSettleSession`
- `chain/x/pole/types/`：重新生成 pb；补 `msg_signers.go` 的 `GetSigners`；补 `codec.go` 注册
- `chain/x/pole/keeper/msg_server.go`：
  - `SubmitPlaySession`：校验 `node_address == msg.Node`；校验 `session_id` 与字段一致；校验 `play_seconds <= slot_seconds`；落 `PlaySessions`
  - `AttestSession`：校验 `witness_address == msg.Witness`；**硬拒 `witness == session.node_address`（禁自证）与 `witness == session.collector_address`（禁自采自证）**；**要求见证者在该 (app_id, epoch_id, slot_id) 上有自己的链上观测记录，且与 session 观测值偏差 ≤ `min_witness_observation_tolerance_ppm`**；**要求 `witness_observation_cid ≠ session.observation_cid`**；落 `WitnessAttestations`
  - `SettleSession`：统计 `witness_count` 与 `distinct_observation_count`，写 `SessionSettlement`，**只由链计算，不接受外部传参**
- `chain/x/pole/keeper/keeper.go`：新增 `FinalizeSession`，接入 `FinalizeEpoch` 前置检查（session 未结算 → 不允许 finalize）
- `chain/x/pole/types/helpers.go`：`Params.Validate()` 补新字段校验；`DefaultParams()` 补默认值

**Rust：**

- `src/records.rs`：新增 `PlaySession` / `WitnessAttestation` / `SessionSettlement`（borsh + serde）
- `src/cosmos/pole_msgs.rs`：新增三个 encoder；按 `ParamsWire` 模式补 proto field 24-27 编码
- `src/cosmos/wire_types.rs`：`ParamsWire` 补 4 个字段（`#[serde(default)]` 向后兼容）
- `src/cosmos/tx_builder.rs`：`BridgeMessage` 补三个变体
- `src/node_daemon.rs`：
  - 新增 `build_play_session(config, slot)` —— 用节点身份密钥签名（D1 下无需独立玩家密钥）
  - 新增 `build_witness_attestation(config, session, own_observation)` —— 要求调用方传入**自己的**观测记录
  - **修 P0-2**：`build_verification_credentials` 的 `target_collector_address` 改为真实的目标采集者地址（从 `BatchVerificationReport` 携带，或从 batch 载荷里取 `collector_id` → bech32），不再填 `verifier_address`
- `src/node_rewards.rs`：`PlayerRewardBlockRecord` 增加 `session_id` 字段（`#[serde(default)]`），使本地奖励记录可追溯到链上 session

**验收：**

```
cargo test --all-targets                       # 全绿，含新增 session/witness 单测
cd chain && go test ./...                      # 全绿
cargo test --features integration              # 新增 session→witness→settle 全流程
```

新增集成测试：`play_session_with_two_independent_witnesses_settles`、`witness_cannot_attest_own_session`、`witness_without_own_observation_is_rejected`、`witness_reusing_player_observation_cid_is_rejected`。

---

### P1.5 — 游玩心跳（已落地）

**问题：** P1 只能证明「网络里 app X 有人在玩」，不能证明「**这个节点**确实在线玩了 N 秒」。一个节点可以声称玩了 3600 秒，实际只开机 1 秒。这是「没有作假」里最难防的一环。

**做法：** 节点在游玩期间按固定时间桶（如每 5 分钟）签一个 `PlayHeartbeat`，见证节点收集并背书。`SettleSession` 额外要求 `heartbeat_count >= min_heartbeat_count`，且心跳覆盖声明游玩区间的比例达标。

**代价：** 新增一个 message、一个链上集合、一条 Rust 心跳循环；增加链上写入量（每节点每 slot 约 12 条）。

**建议：** 先按 P1 落地，用真实数据评估「无心跳时伪造游玩时长的收益」，再决定是否上 P1.5。若用户希望 V1 就防住这一环，则 P1.5 升级为 P1 的一部分。

---

### P2 — 奖励记录上链 + 双边发放（已落地）

**目标：** 解决 P0-4，让「玩游戏的节点」和「做证明的节点」都能真正领到钱。

**链上：**

- 新增 `MsgSubmitRewardRecords`（或让 `MsgSettleSession` 直接写 `RewardRecord`）—— 关键是把 reward records 变成**链上可提交、可校验**的一等公民，而不是只从 genesis/challenge 进来
- `ValidateEpochRoots` 去掉 rewards root 的「链上无记录就接受承诺」特例 —— [keeper.go:650-664](chain/x/pole/keeper/keeper.go#L650-L664) —— 改为无条件强制校验
- `PayoutClaimedReward` 的 `VerifyReward` 分项按 `SessionSettlement.witness_count` 分配，取代只在挑战结算时加钱的逻辑 —— [msg_server.go:648-658](chain/x/pole/keeper/msg_server.go#L648-L658)
- 接通 `ComputePlayerHourWeight` / `ComputePlayerReward` 的调用点 —— [reward_math.go:11](chain/x/pole/types/reward_math.go#L11)、[keeper.go:549](chain/x/pole/keeper/keeper.go#L549)（当前双双零调用）

**Rust：**

- `src/node_rewards.rs`：
  - **修 P0-3**：`service_reward_pools_for_epoch` 的 `verify_pool` 不再无条件给本地节点；改为按「本节点被采纳的 attestation 数」参与分配 —— [src/node_rewards.rs:456-461](src/node_rewards.rs#L456-L461)
  - `RewardRecord` 增加 `witness_count` 分项（用于复算见证奖励）
- `src/cli_client.rs`：`submit-batch` / `submit-epoch` / `export-tx` 从 `chain_bridge` 切到 `cosmos::pole_msgs` + `CosmosClient::submit`，然后删掉 `src/chain_bridge.rs`（修 P2-1）
- `src/genesis_builder/mod.rs`：`default_pole_params()` 补 `min_verification_count: 3` / `min_player_verifier_share_bps: 5000` 及 P1 新字段（**修 P1-2，最便宜也最关键的一处**）

**验收：**

```
cargo test --all-targets && cd chain && go test ./...
cargo test --features integration   # 新增：claim 真实走通（无 challenge 干预）
```

新增 Go 测试：`TestFinalizeEpochRejectsMissingRewardRecords`、`TestWitnessRewardDistributedByAttestationCount`。

> ⚠️ **共识破坏性变更：** 去掉 rewards root 特例后，旧链上已提交的 epoch 承诺可能不再通过校验。需在 P2 明确是「新链从 genesis 开始」还是「加高度门控的迁移开关」。

---

### P3 — 无上限发行（已落地）

- `chain/x/pole/keeper/emission.go`：`BeginBlockAnnualEmission` 保留活动调整后的时间比例参考曲线，移除月度配额与剩余预算截断；`MintedThisMonth` 仅保留为观测字段。
- `chain/x/pole/keeper/keeper.go`：`PayoutClaimedReward` 在模块账户余额不足时按需铸造精确短缺额，再执行支付与 `reward_burn_bps` 销毁。
- 保留 `AdjustedHourlyReward` 的 ±cap 平方根负反馈；它负责单位奖励调节，供给不再由年度预算硬封顶。
- 已新增 `chain/app/app_test.go:TestClaimRewardMintsConfirmedShortfall`，验证空池确认奖励仍按需铸造、支付和销毁后的模块余额归零；原年度曲线测试继续通过。
- 已收口：`chain/app/app_test.go:TestNoMonthlyQuotaCeiling` 与 `TestEmissionFollowsConfirmedSettlements`；Rust↔Go 跨语言 fixture 复用 `chain/x/pole/types/emission_cross_language_test.go` ↔ `src/tokenomics.rs:424-442`；长期供给模拟 `chain/app/supply_simulation_test.go`（20 年逐年额度 + 30 年发行率递减 + 销毁跟进净供给）；白皮书 §4.2 / §4.4.5 已对齐，并新增 §4.4.6「无上限发行的长期供给验证」。

---

### P4 — 销毁五通道接线（已落地）

- **手续费销毁**：`chain/app/ante.go` 新增 `feeBurnDecorator` + `NewAppAnteHandler`，在 `chain/app/app.go` 的 `SetEndBlocker` 与 `LoadLatestVersion` 之间 `SetAnteHandler`；`fee_collector` 模块账户权限改为 `{authtypes.Burner}`。装饰器先执行 SDK 原生 ante 链（含 `DeductFeeDecorator`，把手续费收进 fee collector），再按 `params.FeeBurnBps` 从收集器余额销毁对应份额。
- **治理惩罚销毁**：`chain/x/pole/keeper/msg_server.go:applyChallengeRewardEffects` 对实际生效的 `SlashAmount` 按 `governance_burn_bps` 从奖励池销毁。
- **挑战保证金**：`OpenChallenge` 用 `EscrowChallengeBond` 从挑战者账户划入 pole 模块托管；`ResolveChallenge` 用 `settleChallengeBond` 结算 —— `ChallengeStateRejected` 视为挑战失败，按 `challenge_bond_burn_bps` 销毁后余数留池；其余终态全额退还。
- **虚假游玩惩罚**：`applyChallengeRewardEffects` 在 `ChallengeKindBadReward` 时按 `session_slash_bps` 对 `PlayerReward`（以 `NetReward` 为上限）追加销毁。
- 新增 `chain/x/pole/keeper/burn.go`（`BurnFromRewardPool`/`BurnFeeCoins`/`EscrowChallengeBond`/`SettleChallengeBond`，全部按模块余额有界，余额不足只销毁实际可销毁量）。`src/params.rs` / `src/genesis_builder/mod.rs` / `src/cosmos/pole_msgs.rs` 的新参数位此前已在 P1/P2 补齐。

**验收（已达成）：** `chain/app/burn_test.go` 覆盖五条通道各自的销毁额断言（`TestFeeBurnShareMatchesConfiguredBps`、`TestFeeBurnDecoratorDestroysCollectedShare`、`TestFeeBurnDecoratorSkipsSimulation`、`TestChallengeBondEscrowAndReturn`、`TestChallengeBondForfeitBurnsConfiguredShare`、`TestSessionSlashBurnsRewardOnBadRewardChallenge`）与端到端 `TestNetSupplyChangeEqualsEmissionMinusBurn`；既有契约 `TestResolveChallengeAdjustsRewardRecords` 保持通过；`go test ./...` 全绿。

---

### P5 — 文档与收口（已落地）

- 白皮书：§3.2（玩家权重）、§3.7（节点角色）、§3.8（完整流程）、§4.2 / §4.4.5 / §4.8 全面对齐新模型；§3.8 的「采集→批次→承诺」流程补入「游玩声明→见证→结算」。新增：§2.3.1 术语映射、§3.2.2 有效游玩时长的互证口径、§3.7.1 节点双重身份、§4.4.6 无上限发行的长期供给验证、§4.6.3.1 见证奖励二次分配、§4.8.1.1 销毁路径与责任绑定；§4.9 经济风险由 5 条扩为 10 条。
- `TRACEABILITY.md`：第二章文件映射表 + §3.2/§3.3 权重分账表 + 第六章安全性表 + 附录 B 公式表 + Rust/链文件表全部补齐；验证清单由 7 条扩为 12 条；`chain_bridge` 行换成真实路径（`src/cosmos/pole_msgs.rs` + `CosmosClient::submit`）。
- `chain/README.md`：删掉「目前还没做」的过期立项稿（那四项其实都做完了），改写为设计边界 / 模块划分 / 目录说明 / 构建与测试 / 关键约定。
- 清 `IMPLEMENTATION_PLAN.md` / `RELEASE_NOTES.md` 的 `pole-gui` / MSI 声称（改为真实的 5 个二进制与 release.yml 打包流程）；`CHANGELOG.md` 补互证与无上限发行条目、测试数改为实测值（433/432/1）；`docs/PoLE_Project_Plan.md` 的 `p2p_libp2p`/GUI 段落标为规划中并说明现状；`docs/operations/testnet.md` 与 `troubleshooting.md` 的 `real-libp2p`/`libp2p-diagnose` 引用改为真实命令。
- 删杂物：`.tmp_trx_ma25_*.mjs`、`chain/pole/` 空树、`chain/.tmp-poled-home/`、`tools/` 空目录、`dsh-web*.log` 日志。

---

## 5. 验收矩阵

| 阶段 | Rust | Go | 集成 | 跨语言 | 状态 |
|---|---|---|---|---|---|
| P1 | `cargo test --all-targets` | `go test ./...` | session→witness→settle 全流程 + 4 条拒绝路径 | 新 Params field 24-27 wire golden | ✅ |
| P1.5 | 心跳循环单测 | 心跳门禁测试 | 心跳缺失 → 结算失败 | — | ✅ |
| P2 | 同上 + claim 路径单测 | `TestFinalizeEpochRejectsMissingRewardRecords`（`chain/app/app_test.go:625`） | claim 真实走通 | rewards root 无条件校验 | ✅ |
| P3 | `tokenomics` 去上限单测 | `TestNoMonthlyQuotaCeiling` | 大额结算不失败 | 发行 fixtures 双侧更新 | ✅ |
| P4 | `params` 新字段校验 | 五通道销毁额断言 | 净供给对账 | — | ✅ |
| P5 | — | — | — | — | ✅ |

每阶段收口条件：`cargo fmt -- --check`、`cargo clippy --all-targets --features integration -- -D warnings`、`cargo test --all-targets`、`cd chain && go vet ./... && go test ./...` 全部通过。

**全部收口实测（2026-10-02）：**

| 检查项 | 结果 |
|---|---|
| `cargo fmt -- --check` | exit 0 |
| `cargo clippy --all-targets --features integration -- -D warnings` | exit 0 |
| `cargo test --all-targets` | 433 收集 / 432 passed / 1 ignored / 0 failed（lib 200 + 集成二进制 232 + 1 ignored） |
| `cd chain && go vet ./...` | exit 0 |
| `cd chain && go test ./... -count=1` | `ok pole/chain/app`、`ok pole/chain/x/pole/keeper`、`ok pole/chain/x/pole/types` |
| `go build -o poled.exe ./cmd/poled` | exit 0 |
| `cargo test --features integration --test integration -- --test-threads=1` | 4 passed / 0 failed（54.96s） |

P2 的验收矩阵条目 `TestFinalizeEpochRejectsMissingRewardRecords` 落在 `chain/app/app_test.go:625`；计划书 §262-291 提到的 `TestWitnessRewardDistributedByAttestationCount` 落在 `chain/app/session_test.go:453`（切分数学本体另在 `chain/x/pole/types/witness_reward_test.go` 三条测试中）。

---

## 6. 风险

1. **无上限发行的通胀风险（D2 的直接代价）。** 发行随活跃度线性增长，销毁若跟不上就会失控。缓解：(i) `AdjustedHourlyReward` 的 ±cap 负反馈作为单位奖励调节器；(ii) 销毁率显式随活跃度浮动，形成自动平衡；(iii) P3 落地前必须先跑长期供给模拟。**这是本计划最大的单点风险。**
2. **见证合谋。** `min_distinct_observations` 与「见证者须有自己观测」能防「抄同一份观测」，防不住 N 个节点串通伪造独立观测。V1 的缓解是挑战窗口 + `session_slash_bps`，不追求彻底解决（与白皮书 §3.2.1「V1 不追求一次性穷尽所有真实参与判定技术」一致）。
3. **游玩时长伪造。** 即使 P1 落地，节点仍可虚报 `play_seconds`。P1.5 心跳是针对性的缓解，但会增加链上写入量。
4. **双层奖励数学双实现。** 本次改动会让 Rust / Go 的奖励公式进一步分叉（新增 witness 分配）。必须沿用现有做法：**每个新公式都配一组双侧内嵌 fixtures**，否则会静默分叉。
5. **改动量。** P1+P2 触及 proto、keeper、Rust wire 编码、CLI 五层，是本仓库目前最大的一次结构性改动。建议每阶段单独 PR、单独 tag，不在同一个 tag 里跨阶段。
6. **见证激励可能过弱。** 见 §1.3 参数提示（1.5% vs 80%）。

---

## 7. 建议的执行顺序

> **先做 P1（打通对等见证）+ P2（奖励记录上链 + 修 genesis 门禁清零）** —— 这两步是「拿不到钱」和「证明不成立」的直接病因，且 P2 里的 `default_pole_params()` 补两个字段是整份计划里投入产出比最高的一处改动。P3/P4 的经济模型调整建立在 P1/P2 产生的真实数据之上，放后面做。P1.5 建议在 P1 落地后用真实数据评估收益再定。
