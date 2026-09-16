# SPIKE-001 · Reedline as the editing engine

- **V 项**：V-C01
- **ADR**：ADR-026
- **对象**：`reedline 0.43.0`
- **日期**：2026-09-16
- **结论**：**FALLBACK-ADOPTED(ADR-026)** —— 采用预先命名的回退方案：`pr-repl` 自有 LineBuffer

## 验收标准（来自 v2.1 §14.5 / ADR-026）

| # | 标准 | 结果 |
|---|---|---|
| (a) | `LineBuffer` 能在不丢字节、不破坏 UTF-8 边界的前提下接受外部 `TextEdit` | **部分通过** |
| (b) | 「接受候选」能封装为**单个可撤销事务** | **不通过** |
| (c) | 与 `pr-terminal` 组合通过 ASSIST-020~028 / 062~064 / 087 | 取决于 (a)(b) |

## 实测

探针：`reedline 0.43.0`，`LineBuffer` 公开 API。

### (a) 外部 TextEdit —— 部分通过

```text
A1 replace_range(18..21, "status")  →  "HGET player:10001 status"     ✅ 字节精确
A3 多字节替换 replace_range(6..12, "🐧x") →  "SET k 🐧x"               ✅ 字节精确
A2 光标：替换前 21，替换后仍是 21（应为 24）                            ❌ 不修正光标
```

`replace_range` 本身是好的。但它**不调整插入点**：一个把 3 字节换成 6 字节的编辑之后，光标停在旧位置。这一条可以由 adapter 自己补，属于可接受的额外工作。

### (a-续) 边界不变量不由库保证 —— 决定性问题

```text
D1 set_insertion_point(1)  // '中' 的中间，静默接受
D2 随后 move_left()
D3 panic: end byte index 1 is not a char boundary; it is inside '中' (bytes 0..3)
        at reedline-0.43.0/src/core_editor/line_buffer.rs:174
```

`set_insertion_point` 接受任意 byte offset 且**不校验**，下一次移动就在库内部 panic。

这对本项目是硬伤，原因具体而非洁癖：§12.8 要求异步候选带着可能**已过期的 span** 回到 UI，正确行为是「丢弃该候选」；而这里一个过期 offset 的后果是**库内 panic**，不是我们能处理的错误。workspace 又是 `panic = "deny"`。把正确性寄托在「我们永远算对 offset」上，正是 ASSIST-026~028 要防的那类失败。

### (b) 单个可撤销事务 —— 不通过

```text
B1 LineBuffer 无 undo
B2 Editor（持有 undo 栈）不是 pub
```

undo 在 `reedline::core_editor::Editor` 上，该类型未公开导出，无法构造也无法驱动。因此 ADR-016 的「接受候选 = 一次可撤销 edit transaction」**无法用 Reedline 的 undo 实现**——无论 (a) 结果如何，撤销栈都得自己写。

### (c) 组合验收

(b) 已经不通过，(c) 无需再测：即便保留 Reedline 的 `LineBuffer`，我们仍要自建撤销栈、自建光标修正、自建边界校验，剩下的只是一个 `String` 包装。

## 决定

触发 ADR-026 预先命名的回退：**`pr-repl` 自有 grapheme-aware LineBuffer**，复用 Reedline 的 history 文件格式以保持迁移路径。

回退方案必须跑**同一套测试**（ADR-026 明文要求），不降低标准。额外获得的保证：

- 光标永远落在 grapheme 边界，非法 offset 返回 `Err`，**不 panic**
- 「接受候选」是一次 `EditTransaction`，一次 undo 完整回退
- 过期 span 被拒绝并返回错误，由调用方丢弃候选（§12.8）

## 仍然复用 Reedline 的部分

`History` trait 与其文件格式（V-H06 的 SQLite 实现对齐它），以及 `Completer` 的 span 数据结构作为参考。终端所有权本来就归 `pr-terminal`（ADR-026 原文），不受本结论影响。

## 证据

探针源码与输出见本次会话记录；结论落地为 `crates/pr-repl/src/buffer.rs` 及其测试。
