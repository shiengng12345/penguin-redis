# Architecture Decision Records


| ADR | 标题 | 状态 | 关联 V 项 |
|---|---|---|---|
| [ADR-001](ADR-001.md) | 产品名 `Penguin Redis`，可执行文件 `prc`，profile 前缀 `@` | accepted | V-B07 |
| [ADR-002](ADR-002.md) | 原生 Redis 命令语义优先；本地增强操作使用 `:` 命名空间 | accepted | V-F03、V-B07 |
| [ADR-003](ADR-003.md) | 无必需 backend / daemon；Core 是链接进可执行文件的库 | accepted | V-H04、V-H05、V-D08 |
| [ADR-004](ADR-004.md) | Table-first：每 field 横线、强化表头（双线）、JSON 在 value 单元格内多行 | accepted | V-E02、V-C06 |
| [ADR-005](ADR-005.md) | 原始 bytes 是权威数据；任何 view 不改变类型、顺序或词法 | accepted | V-E01、V-B02 |
| [ADR-006](ADR-006.md) | 原始命令不隐式附带元数据请求，也不自动换算法 | accepted | V-F10、V-E05 |
| [ADR-007](ADR-007.md) | `UnknownAfterSend` 是正式 outcome；写命令不盲目重试 | accepted | V-B04、V-B05、V-G02 |
| [ADR-008](ADR-008.md) | CLI / TUI / MCP / recipe / `--pipe` 全部经过同一 execution kernel 与 policy | accepted | V-B03、V-D08、V-C02 |
| [ADR-009](ADR-009.md) | 有界解码与有界保留；落盘 spool 必须显式授权 | accepted | V-B02、V-E03、V-H02、V-H03 |
| [ADR-010](ADR-010.md) | profile 的 secret reference 绑定 TLS 身份 + 服务身份（`TrustIdentity`），不绑定网络地址 | accepted | V-D01、V-D02、V-D04 |
| [ADR-011](ADR-011.md) | 兼容性只对固定 baseline 的具体版本声称，不做全称承诺 | accepted | V-A05、V-I01、V-I02、V-A02 |
| [ADR-012](ADR-012.md) | Result Context：本地 inspect 与重新查询是两个不同动作 | accepted | V-E03、V-F06 |
| [ADR-013](ADR-013.md) | `--raw`、`--json`、`--output typed-json|ndjson|resp`、`--bytes` 是独立契约，互斥 | accepted | V-B06、V-E04 |
| [ADR-014](ADR-014.md) | 完整测试与发布证据属于每个阶段，不是最后补 | accepted | V-A01、V-A02、V-A03、V-A04、V-A05 |
| [ADR-015](ADR-015.md) | CLI 与 TUI 的自动建议共用同一本地 Command Intelligence；LLM 不是依赖 | accepted | V-F01、V-F04、V-F09 |
| [ADR-016](ADR-016.md) | 接受候选只编辑输入缓冲区；菜单焦点与提交分离；Enter 永不「补全并发送」 | accepted | V-C01、V-C02 |
| [ADR-017](ADR-017.md) | 动态候选按 `ObservationScope`（profile/service identity/DB/auth epoch/policy epoch/topology epoch）隔离；buffer revision 防迟到污染 | accepted | V-F05、V-F06 |
| [ADR-018](ADR-018.md) | Grammar / catalog ≠ server capability ≠ ACL 权限；三者分开显示 | accepted | V-D07、V-F04 |
| [ADR-019](ADR-019.md) | Hash field 发现避免隐式抓取 value；不支持 `HSCAN NOVALUES` 的目标明确降级 | accepted | V-F07 |
| [ADR-020](ADR-020.md) | Find / Guide / Learn / 单位助手 / 错误帮助默认零联网；模板与建议只插入，永不自动执行 | accepted | V-F10、V-F07、V-F08 |
| [ADR-021](ADR-021.md) | 执行结果模型分四个正交维度：delivery / reply / effects / render | accepted | V-B04 |
| [ADR-022](ADR-022.md) | 单一 terminal coordinator；shell completion 与 REPL completion 是不同适配层 | accepted | V-C02、V-A01 |
| [ADR-023](ADR-023.md) | `prc` 自身许可与 catalog 再分发结论（Phase 0 / V-I03 交付） | accepted | V-I03 |
| [ADR-024](ADR-024.md) | 审批令牌绑定 canonical argv 字节哈希 + 完整 `TrustIdentity` + epoch + 有效期；摘要只用于显示 | accepted | V-D03、V-D08 |
| [ADR-025](ADR-025.md) | `--pipe` 原始协议输入逐帧解析并经过同一 catalog / policy；agent origin 默认不可用 | accepted | V-B03 |
| [ADR-026](ADR-026.md) | Reedline 仅作编辑状态引擎；终端所有权归 `pr-terminal`；SPIKE-001 决定是否回退自有 LineBuffer | accepted | V-C01、V-C02、V-J04 |
| [ADR-027](ADR-027.md) | 普通编辑器与 shell argv 是 UTF-8 文本；二进制 / NUL / 无效 UTF-8 只经 §3.2 定义的入口 | accepted | V-F03、V-B02 |
| [ADR-028](ADR-028.md) | `pr-json` 是服务器业务 JSON 的唯一权威模型；重复成员以 occurrence 索引寻址；`serde_json` 只用于 Penguin 自有 schema | accepted | V-E01 |
| [ADR-029](ADR-029.md) | 所有内存 / 并发 / 速率预算为单进程预算；无 daemon 架构不做跨进程仲裁 | accepted | V-F09、V-H01、V-A06 |
| [ADR-030](ADR-030.md) | 本地签名 catalog / overlay 是 effects、危险性、审批等级与 key extraction 的唯一权威；远端 metadata 只补充 availability 与文档 | accepted | V-D07、V-F01 |
| [ADR-031](ADR-031.md) | `pr-protocol` 自研 codec 是唯一 kernel 协议边界；`redis-rs` 降为互操作测试对象 | accepted | V-B01、V-J04 |
| [ADR-032](ADR-032.md) | TLS 用 rustls + 显式 CA 集合；不存在关闭校验的开关 | accepted | V-G04、V-J04 |
| [ADR-033](ADR-033.md) | 显式发现的 SCAN 次数上限改为 200，与「20 req/s × 10 s」对齐 | accepted | V-F07、V-J04 |
| [ADR-034](ADR-034.md) | 用途搜索的 held-out 指标需要一位不是实现者的撰写人；该输入是 Phase 0 出口依赖 | accepted | V-F08、V-J04 |
| [ADR-035](ADR-035.md) | Windows Terminal / ConPTY 是一级目标，conhost 是 plain 模式降级目标；`SetConsoleCtrlHandler` 投递层延至 Phase 1 | accepted | V-C07、V-J04 |
| [ADR-036](ADR-036.md) | 「粘贴绝不自动执行」不依赖终端能力：bracketed paste 与时序启发式并存，`plain-only` 是最后手段 | accepted | V-C03、V-J04 |
| [ADR-037](ADR-037.md) | 宽度分歧由「每格重置光标」降级绘制承担；`CSI 6 n` 只在 TTY 且用户触发 | accepted | V-C04、V-J04 |
| [ADR-038](ADR-038.md) | 每个功能至少一条纯文字入口；重映射不得连带移除文字入口 | accepted | V-C05、V-J04 |
