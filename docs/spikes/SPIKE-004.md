# SPIKE-004 · `pr-json` occurrence DOM

- **V 项**：V-E01
- **ADR**：ADR-028
- **结论**：**采用**（`PASS`）
- **日期**：2026-09-16

## 问题

v2.1 §8.3 要求：重复 object 成员全部保留并可分别寻址；数字词法（`1.2300`、`-0`、`1e3`、
`9007199254740993`）逐字节保留；编辑只重写目标 span。

## 为什么 serde_json 不行（已验证）

| 要求 | `serde_json::Value` | `RawValue` |
|---|---|---|
| 重复成员 | `Map` 后者覆盖前者，**丢数据** | 保留原文，但**无可寻址树** |
| 数字词法 | `Number` 归一化；`1.2300`→`1.23`，大整数→f64 丢精度 | 保留原文，无结构 |
| 局部编辑 | 序列化整棵树，空白/顺序全变 | 不支持 |

## 采用的模型

```text
JsonNode { span: Range<usize>, kind }
NodeKind::Object(Vec<Member>)
Member { key_span, key, occurrence, occurrence_total, value }
Document { source: Bytes, root }   // 树 + 原始缓冲区
```

- 标量不存值，只存 **span**；`number_lexeme()` / `string_value()` 按需从 source 切片。
- 路径 `$.a#2` 选第 2 个同名成员；**重复成员在无 `#n` 时拒绝解析**（`AmbiguousMember`），
  不静默取第一个——这正是 ADR-028 要防的编辑器覆盖错成员。
- `replace()` 只重写目标 span，其余字节逐字节保留（含空白与键顺序）。

## 设计决定：lone surrogate

`{"s":"\ud800"}` 语法合法但不表示任何 Unicode scalar。**保留文档并拒绝解码**
（`string_value()` 返回 `None`），而不是拒绝整个文档——符合 ADR-005「原始 bytes 权威、
解码是视图」。Redis 可以存这种数据，客户端不应因此丢掉它。

## 证据

`crates/pr-json/` · 17 tests：DATA-02/03/04、JSON-05、深度预算、自动识别边界
（`70`/`true`/`null`/不完整 JSON 均不算 JSON）、source 字节往返。

## 待办（不属于本 spike 的通过条件）

16 MiB 单值的解析/格式化时间与内存基准，依赖 V-A06 测量工具，随 V-H 轨道补测。
