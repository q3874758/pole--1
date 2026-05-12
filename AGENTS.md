# PoLE 仓库 — AI 助手约定

本文件取代原 oh-my-codex（OMX）生成的 `AGENTS.md`，不再依赖 `.codex/` 技能树或 `omx` CLI。

## 自主执行

在任务边界清晰、可逆时直接推进实现与验证；仅在不可逆操作、破坏性变更或需求存在实质分歧时向维护者确认。

## 工作方式

- 以证据为准：结论应能由代码、测试或命令输出支撑。
- 改动尽量小、可审查、可回滚；非请求不引入新依赖。
- 清理/重构前先理清影响面；若行为未由测试保护，优先补回归测试再改。
- 完成后运行与改动相关的 `cargo` 检查（如 `cargo test`、相关 crate 的 clippy 等）。

## 提交说明（可选 Lore 风格）

需要记录决策时，首行写**动机**（为什么改），正文可含约束与取舍；可使用 Git trailer，例如：`Tested:`、`Not-tested:`、`Confidence:`。

## 仓库范围

PoLE（Proof of Live Engagement）Rust 工作区：玩家/节点 CLI、可选 GUI、链与运维文档等。具体模块以 `Cargo.toml` 与各 crate 内说明为准。
