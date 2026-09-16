# V-A03 外部评审记录（DeepSeek v4-pro）

- **日期**：2026-09-16
- **对象**：`xtask/resp-server/src/encode.rs`（RESP2/RESP3 wire encoder）
- **方式**：对照 https://redis.io/docs/latest/develop/reference/protocol-spec/ 逐 variant 审查
- **备注**：模型两次在 reasoning 阶段耗尽 token，未产出结构化正文；结论从其 reasoning 尾部提取后**由本仓逐条复核**，不直接采信。

## 采纳（真实问题）

| # | 问题 | 复核结论 | 修复 |
|---|---|---|---|
| 1 | `Frame::Push` 在 RESP2 下直接报错 | **成立。** RESP2 订阅者确实以普通 array 收到 pub/sub 消息；一律拒绝意味着**根本无法生成 RESP2 pub/sub fixture** | 改为 RESP2 下降级为 `*n` |
| 2 | `StreamedBulk` 静默跳过空 chunk | **成立，且是本仓最不能接受的一类 bug。** `;0` 是终止符，空 chunk 无表示形式；静默丢弃 = 不可检测的数据丢失 | 新增 `EncodeError::UnrepresentableFrame`，明确报错 |
| 3 | `NullBulk`/`NullArray` 在 RESP3 下仍输出 RESP2 形式，语义不清 | **行为正确但文档不足。** 这两个 variant 的用途就是「逐字面输出该 wire 形式」，以便构造「RESP3 连接上收到 RESP2 null」这类畸形 fixture | 补文档并新增 `literal_null_forms_are_emitted_verbatim_in_both_protocols` 测试固定该意图 |

## 不采纳

| 建议 | 不采纳理由 |
|---|---|
| `Attribute` 在 RESP2 下应丢弃 attrs 后降级 | 真实 Redis 从不向 RESP2 客户端发送 attribute。测试服务器如实报错「你要的东西在这个协议里不存在」比悄悄改写更有价值 |
| `StreamedArray/Map/Set` 在 RESP2 下应降级 | 同上：streamed 形式在 RESP2 中不存在 |
| `BulkError` 文本含 CRLF 时应拒绝 | 评审自己核对 Redis 源码后认为真实 Redis 也不做该检查；测试服务器应能复现真实行为，包括其不足 |

## 评审确认正确的部分

verbatim 长度算术（`len` 计入 `xxx:` 前缀共 4 字节）、streamed 终止符（`;0` / `.`）、
`Boolean`/`Double`/`BigNumber`/`Verbatim`/`Set`/`Map` 的 RESP2 降级、attribute 相对其修饰值的位置、
长度前缀无 off-by-one。

## 结果

`cargo test -p resp-server` 17 → 全绿；workspace 87 tests 全绿；fixtures 重新生成后 316 样本不变。
