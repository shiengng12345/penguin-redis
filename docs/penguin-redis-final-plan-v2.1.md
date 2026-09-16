# Penguin Redis — 最终产品与工程蓝图
## Command Intelligence Edition · v2.1

> **产品：Penguin Redis · 可执行命令：`prc`**  
> **日期：2026-09-16 · 交付对象：Shi Eng 与后续实现者**  
> **定位：Native · Standalone · CLI-first · Table-first · Assisted-by-default**  
> **状态：最终实施基线；不是已发布的软件、安装包或通过测试的实现。**  
> **v2.1：在 v2.0 基础上，依据三方独立评审（Claude / Codex / DeepSeek）修正已核实的矛盾、不可实现项与缺失规格。终点范围零削减。修订清单见「v2.1 修订记录」。**

**不以开发成本、时间、资源或难度降低最终能力。** 分阶段是实现依赖顺序，不是削减产品终点；测试、安全、性能、输入正确性与数据保真从第一阶段纳入。

本文件完整整合前版 `penguin-redis-ultimate-development-plan.md` 的产品/工程能力，重新设计并扩充智能输入，可独立使用，不需要再拼接多个补丁文档。所有 `prc` 命令、界面、配置、性能预算与测试案例均为拟实施契约；引用资料用于核对外部协议/工具行为，不证明产品已经实现。

### 最终产品决策

| 你真正需要的体验 | 定案 |
|---|---|
| 产品名完整，但每天命令短 | `Penguin Redis` / `prc` |
| 安装就能用，不开自有 backend | 单次/会话内原生进程；无必需 daemon、GUI 或 Studio |
| 不反复复制 credential | 安全录入 URL 一次，之后 `prc @dev` / `prc @r2` |
| 保留 Redis muscle memory | 原始命令语法；Penguin 本地操作单独 `:` 命名空间 |
| Hash 看得清楚 | 默认完整表格，字段间横线、强化表头、双线层级 |
| JSON 不再挤一行 | 在 value 单元格内多行展开，语法配色，原始 bytes 保留 |
| 记不住 commands | **CLI/TUI 自动建议、参数提示、用途搜索、Guide、离线帮助** |
| 不想被智能功能拖慢 | 本地确定性引擎；无逐字 LLM、无隐藏全库扫描 |
| 复杂工作也能完成 | 可选 TUI、安全编辑、比较、诊断、Streams、运维、自动化 |
| 真正可长期用 | 完整测试、故障语义、资源预算、分发升级与 Studio 复用 |

### 每天大部分时间，只需要这样

```bash
prc --add dev                       # 首次：安全录入、保存连接
prc @dev                            # 日常：进入带自动建议的 CLI
prc @dev HGETALL player:10001        # 一次查询后退出
prc @dev --tui                      # 深度浏览；同样有智能命令框
prc --learn hash                    # 离线查/学 Hash，不连接 Redis
```

在会话里输入 `HG` 会提示可用命令；选 HGET 后提示 key、再提示 field。完全忘记名字时，使用 F2 / `:find 查看 hash 字段`。**你只需要表达任务，不必先背出完整 Redis 语法；所有建议仍由你决定是否接受和执行。**

### 重新思考后的三个核心

**Command Intelligence** 帮你找到并正确写出命令。**Execution Kernel** 确保目标、权限、协议、会话与失败行为可信。**Presentation Engine** 把真实结果变成清晰的彩色 Table/JSON。

三者共用契约但职责隔离：提示器不能执行写入，renderer 不能偷偷查数据；强大功能按需启用，退出没有自己的后台负担。

### 建议阅读顺序

先看 **01–09** 确认产品与显示；重点看 **10–16** 的自动建议、命令查找、Guide、键位及验收；工程实现读 **19–24、31–37**；配置与 fixtures 在附录。完整终点与不接受的退化在 **38**。

### v2.1 修订记录

本版只做三类改动：**修正内部矛盾**、**把不可实现的表述改成可实现的契约**、**补齐实现者必需但 v2.0 缺失的规格**。没有删除任何最终能力；开发成本、时间与难度不是本版的修订依据。每条标注评审来源：C = Claude、X = Codex、D = DeepSeek。

| # | 位置 | 修订 | 来源 |
|---|---|---|---|
| R01 | §18.5 | `--pipe` 原始协议输入新增逐帧解析、分类、策略与结果关联契约；不再是策略绕过通道 | X |
| R02 | §23.2 | 审批令牌改绑 canonical argv 字节哈希；「参数摘要」只用于显示 | X |
| R03 | §14.5 §31.1 §37.3 | Reedline 降为编辑状态引擎，pr-terminal 独占终端；SPIKE-001 设为 E2 门禁并命名回退方案 | C X D |
| R04 | §3.2 §11.7 | 普通编辑器明确为 UTF-8 文本；二进制/NUL/无效 UTF-8 走定义好的转义语法与专用入口 | X |
| R05 | §14.1 | 强制 bracketed paste；不支持时进入不可执行的 paste-staging | X |
| R06 | §8.3 §8.5 §31.3 | 新增 `pr-json` crate；重复成员以 occurrence 索引寻址；禁止 `serde_json::Value` 作权威模型 | C X D |
| R07 | §4.2 §30.2 | 钥匙串与 profile 文件之间定义写入顺序、journal 与启动 reconciliation | C X D |
| R08 | §12.5 附录 A | 修正 assistance 预算算术（子项合计已等于父预算）；只读 catalog 单独计量 | C D |
| R09 | §12.5 附录 A | 自动发现与显式发现分开预算；显式发现给出可用的默认上限 | C D |
| R10 | §16.4 | 用途搜索 recall 指标改为 canonical 集 + held-out 改写集，两者都公开 | C |
| R11 | §11.5 §32.6 | 新增宽容分析器与权威 tokenizer 的 span 等价性 differential 测试；quoting 语法固定版本 | C D |
| R12 | §29.4 | 「已批准策略」限定为只读 + key 模式 + 次数上限；写入一律人工审批 | C |
| R13 | 附录 A | `[assistance.cache].scope` 补入 topology epoch，与 §12.6 一致 | X |
| R14 | §21.3 | 定义 `TrustIdentity`：endpoint / TLS 身份 / 服务身份 / 认证身份四元组及轮换流程 | X |
| R15 | §21.5 | SSH 支持 per-node tunnel 或 SOCKS5 动态转发；Cluster 重定向到未登记节点需确认 | X |
| R16 | §19.3 | RESP decoder 覆盖 RESP3 streamed string / streamed aggregate | X |
| R17 | §18.1 §18.3 | one-shot `--json` 的 push 帧处理、`--show-pushes`、JSON projection v1 表 | X |
| R18 | §3.5 | 本地 `:` 命令拥有独立 tokenizer 与 quoting 规则 | X |
| R19 | §11.2 §20.2 | 远端 metadata 只能补充 availability/docs，不能覆盖本地 effect/危险性分类 | X |
| R20 | §35.1 §32.1 §33 | Windows 从纸面支持改为具体契约：ConPTY、二进制 stdio、Credential Manager、LockFileEx | D |
| R21 | §31.1 §31.4 | 命名任务监督机制（Tokio 无内建 scoped task） | D |
| R22 | §31.1 | 点名 history 引擎 crate、WAL、schema 版本 | D |
| R23 | §31.1 | Secret store 按平台点名 feature 与 headless fail-closed 行为 | X D |
| R24 | §10.10 附录 A | 生产 profile 默认脱敏 key 参数；定义保留期与文件权限 | X D |
| R25 | §7.1 | 补 Penguin Light 与 High-contrast token 表 | C |
| R26 | §3.3 | 定义 profile 与命令行 flag 的允许覆盖/冲突矩阵 | C |
| R27 | §18.6 | 定义 REPL 会话退出码与 `--exit-on-error` | C |
| R28 | §14.1 | 菜单可见时 Down 进入菜单，历史改由 Ctrl+P/Ctrl+N 保证可达 | C |
| R29 | §20.3 §20.4 §35.5 | 点名 Valkey；给出初始固定兼容矩阵；catalog 再分发许可审查升为 E0 交付物 | C X D |
| R30 | §19.2 §19.6 | 合并为单一四字段结果模型，删除会被误读的过渡 enum | C |
| R31 | §8.2 §24.4 | 定义「流式 / 阈值询问 / 可回看」的唯一顺序：已知长度先询问，未知长度流式+有界保留，机器输出永不询问 | X |
| R32 | §12.4 | 字面前缀发现与 glob MATCH 模式分离，标明所施加的变换 | D |
| R33 | §14.6 | Unicode 显示宽度策略：ambiguous width 可配置、ZWJ 序列按 grapheme、可选光标探测 | D |
| R34 | §23.5 §12.9 | 所有 server 来源字符串统一进入 SafeText 边界 | X |
| R35 | §24.7 | 明确所有预算为进程级；无 daemon 架构下不做跨进程聚合 | X D |
| R36 | §28.4 | 官方特殊模式逐项 session/协议/退出/预算/审批契约；prod/agent 默认 deny 清单 | X |
| R37 | §36 | E2 门禁的「关键 ASSIST」改为显式编号清单 | C |
| R38 | §16.2 §33 §37.1 §37.5 §38.2 | 新增 ASSIST-081~088、验收 SEC-09~11 / PIPE-08 / WIN-01~03、ADR-023~030、首批 issue | C X D |
| R39 | §12.10 | 共享本机文件要求 advisory lock + 原子替换 + 冲突记录 | D |
| R40 | §18.1 | 澄清 `--output resp` 的用途为规范化再编码，不是 wire 抓包 | C |
| R41 | §19.5 | 可观测性只记请求/响应字节**数**；原始 payload 仅 `--trace-wire` 显式落盘 | D |
| R42 | §22.3 | 定义 Ctrl+C 与 unknown/partial 结果的退出码优先级；`130` 仅表示无未知副作用 | D |
| R43 | §21.2 | 明确 Cluster hash tag `{…}` slot 计算规则 | D |
| R44 | §23.1 | ACL 维度补入 Pub/Sub channel 模式 | D |
| R45 | §30.4 | 插件隔离改为「声明式进程内 / 任意代码外部进程」两级；删除进程内原生代码可隔离的表述 | D |
| R46 | §29.4 | `--mcp-stdio` 定义为无 TUI/无 banner 的独立 bootstrap | D |
| R47 | §18.2 §30.1 | `--bytes`/`--output resp` 直写 TTY 的警告与拒绝规则；秘密存放位置说明 | D |

---

## 阅读导航

| 章节 | 内容 |
|---|---|
| [01 · 产品定案](#s01) | 产品定案：不是临时 wrapper，而是独立主力工具 |
| [02 · 使用模型](#s02) | 使用模型：一个程序，三种体验 |
| [03 · 命令契约](#s03) | 命令契约：短、稳定、不重新发明 Redis |
| [04 · 连接管理](#s04) | 连接管理：保存一次，不重复复制 credential |
| [05 · 日常使用剧本](#s05) | 日常使用剧本 |
| [06 · Table 设计系统](#s06) | Table 设计系统：把用户已经确认的视觉固定下来 |
| [07 · Colour](#s07) | Colour：可用、克制、可配置 |
| [08 · JSON、长值与二进制](#s08) | JSON、长值与二进制：好看不能牺牲原始数据 |
| [09 · Renderer 全面覆盖与正确性规则](#s09) | Renderer 全面覆盖与正确性规则 |
| [10 · 智能输入体验](#s10) | 智能输入体验：记不住命令，也能顺着提示完成工作 |
| [11 · Command Intelligence Engine](#ci01) | Command Intelligence Engine：语法、版本与确定性建议 |
| [12 · 候选数据与资源预算](#ci02) | 候选数据与资源预算：智能，但不偷偷扫描整个 Redis |
| [13 · Find、Guide、Learn](#ci03) | Find、Guide、Learn：从记忆命令转为选择任务 |
| [14 · 终端输入契约](#ci04) | 终端输入契约：焦点、按键、颜色与无障碍 |
| [15 · 端到端使用剧本](#ci05) | 端到端使用剧本：从不知道命令到读懂结果 |
| [16 · 智能提示专项验收](#ci06) | 智能提示专项验收：不能只证明“会弹框” |
| [17 · TUI](#s11) | TUI：需要时展开，而不是强迫用户迁移习惯 |
| [18 · 输出、stdin、脚本与退出码](#s12) | 输出、stdin、脚本与退出码 |
| [19 · 核心架构](#s13) | 核心架构：执行与显示彻底分离 |
| [20 · 命令、协议与版本覆盖](#s14) | 命令、协议与版本覆盖 |
| [21 · 连接层](#s15) | 连接层：真实内网、Cluster、Sentinel 与 SSH |
| [22 · 取消、重连与事务](#s16) | 取消、重连与事务：避免“看起来失败，实际执行两次” |
| [23 · 生产安全与秘密保护](#s17) | 生产安全与秘密保护 |
| [24 · 大数据与低开销](#s18) | 大数据与低开销：必须从读取端解决 |
| [25 · 搜索、TTL、编辑与并发冲突](#s19) | 搜索、TTL、编辑与并发冲突 |
| [26 · Streams、Pub/Sub 与实时观察](#s20) | Streams、Pub/Sub 与实时观察 |
| [27 · 诊断与比较](#s21) | 诊断与比较：证据明确，结论有边界 |
| [28 · 导入导出、批量与高级运维](#s22) | 导入导出、批量与高级运维 |
| [29 · 自动化、可选 AI 与 MCP](#s23) | 自动化、可选 AI 与 MCP |
| [30 · 本机配置、数据迁移与可信扩展](#s24) | 本机配置、数据迁移与可信扩展 |
| [31 · Rust 技术选型与仓库组织](#s25) | Rust 技术选型与仓库组织 |
| [32 · 完整测试体系](#s26) | 完整测试体系：不是“写一些 unit tests” |
| [33 · 必须通过的验收案例](#s27) | 必须通过的验收案例 |
| [34 · 基准、质量门禁与发布证据](#s28) | 基准、质量门禁与发布证据 |
| [35 · 安装、升级、卸载与生命周期](#s29) | 安装、升级、卸载与生命周期 |
| [36 · 分阶段交付](#s30) | 分阶段交付：有完整终点，不设缩水终点 |
| [37 · 关键 ADR 与立即开始的实施顺序](#s31) | 关键 ADR 与立即开始的实施顺序 |
| [38 · 最终完成标准与不接受的退化](#s32) | 最终完成标准与不接受的退化 |
| [附录 A · 统一配置](#appendix-a) | Profile、安全、主题、Assistance、缓存与资源 |
| [附录 B · 测试种子](#appendix-b) | 本地 fixture、示例与真实性标记 |
| [附录 C · 一手资料](#references) | 协议、工具、语法、终端与依赖来源 |

---

<a id="s01"></a>
## 01 · 产品定案：不是临时 wrapper，而是独立主力工具

### 1.1 一句话定位

**Penguin Redis 是一个无需自有后台常驻、保存连接后可直接使用、保留 Redis 命令语法，并以高质量彩色表格呈现数据、在 CLI/TUI 输入时主动提供上下文建议的原生终端客户端。**

它现在解决 `HGETALL` 难读、JSON 被挤成一行、连接凭证反复复制的问题；最终成为完整的 Redis 开发、排查和受控运维工具。Penguin Studio 上线后，它仍然独立成立，而不是被 GUI 淘汰。

### 1.2 已确定且不可被后续“简化”掉的需求

| 编号 | 不可妥协的要求 |
|---|---|
| REQ-01 | 产品可叫 `penguin-redis`，每天执行的命令必须短：`prc`。 |
| REQ-02 | 无需启动 Penguin Studio、HTTP backend、Docker 或自有 daemon。 |
| REQ-03 | 连接 URL 保存一次，之后通过 `@dev`、`@r2` 等名称复用。 |
| REQ-04 | 密码不默认明文写进配置；使用安全凭证存储。 |
| REQ-05 | `GET`、`HSET`、`HGETALL` 等 Redis 命令语法保持原样。 |
| REQ-06 | Hash 默认完整 Field / Value table，每个字段之间有横线。 |
| REQ-07 | 表头必须明显区别于数据：加粗、独立色彩、双线分隔。 |
| REQ-08 | JSON 在 value 单元格内部多行展开，支持语法高亮。 |
| REQ-09 | CLI 默认直接可用；TUI 是增强入口，不是必经路径。 |
| REQ-10 | 普通输出、脚本输出、无损导出有明确且不同的契约。 |
| REQ-11 | 低开销必须通过设计与 benchmark 验证，不靠“Rust 一定快”的宣传。 |
| REQ-12 | 完整测试、故障处理、生产安全属于产品本体。 |
| REQ-13 | 长任务显示当前阶段、范围、已完成工作和局部发现，不只有进度条。 |
| REQ-14 | 全部最终能力有交付位置；阶段化不等于永久砍功能。 |
| REQ-15 | CLI REPL 和 TUI 命令框默认都有自动下拉建议，不只支持手动 Tab。 |
| REQ-16 | 补全覆盖命令、子命令、参数、选项与当前作用域的已知 key/field。 |
| REQ-17 | 提供按中英文用途查命令、参数单位说明、Guide、离线帮助与错误解释。 |
| REQ-18 | 选择建议只编辑，绝不自动执行；不得静默纠错重发。 |
| REQ-19 | 默认提示完全本地生成，不靠 LLM、不隐藏扫描、不反复解锁凭证。 |
| REQ-20 | CLI/TUI/未来 Studio 复用同一 Command Intelligence，不复制三套 grammar。 |
| REQ-21 | 版本支持、local policy、server ACL 与命令效果是独立状态。 |
| REQ-22 | 候选按 profile/identity/DB/epoch 隔离，迟到异步结果不能污染输入。 |
| REQ-23 | 网络慢、离线、终端窄或颜色关闭时，输入与帮助仍有可用降级。 |
| REQ-24 | Suggestion/Guide/privacy/resource/PTY 测试属于发布门禁，不后置。 |

### 1.3 怎样定义“比官方更好”

不把“所有场景都更快、所有版本都 100% 兼容”当成未经验证的承诺。把优势拆成可验收维度：连接录入次数、常见任务操作次数、读取复杂结果的清晰度、脚本兼容性、错误可解释性、误操作防护、峰值内存和吞吐。

官方 CLI 已有 raw/JSON 输出、历史与补全，以及多种诊断和特殊模式；这些是兼容基线，不应被误写成完全不存在。[R01] IRedis 已提供增强补全、语法提示、友好输出和连接配置；Redis Insight 提供图形化数据工具。这些现有强项值得吸收，但不要求继承它们的产品形态。[R21][R22]

**差异化重点：短路径连接 + 不靠背诵的智能输入 + 字节级正确性 + 一致的 Table/JSON 体验 + 可控执行内核 + 完整终端工作流。**

### 1.4 明确不做成什么

不是解析官方 `redis-cli` 输出的正则 wrapper，不是浏览器页面套本地服务，不是必须登录的云客户端，也不是每次启动都初始化几十个插件的重型终端 IDE。

强大来自能力边界完整；轻快来自按需加载。二者不矛盾。

<a id="s02"></a>
## 02 · 使用模型：一个程序，三种体验

| 模式 | 入口 | 适合的工作 | 生命周期 |
|---|---|---|---|
| One-shot CLI | `prc @dev HGETALL player:10001` | 查一次、脚本、管道 | 输出完成后退出 |
| REPL，默认 | `prc @dev` | 连续输入原 Redis 命令 | 用户退出时结束 |
| 可选 TUI | `prc @dev --tui` 或 `:browse` | 大结果、搜索、比较、编辑；同样有智能命令框 | 与当前进程同生命周期 |
| 离线学习 | `prc --learn` / `prc --demo` | 查命令、练习输入、观察 fixture 显示 | 无 Redis 连接，无后台服务 |

```text
Terminal -> prc process -> TCP / TLS / Unix socket -> Redis
                    |
                    +-- local config / secure credential reference
                    +-- REPL / renderer / optional TUI
```

Core 是链接进可执行文件的库，不是 server。退出 `prc` 后没有 Penguin 自有监听服务、计划任务或后台索引器。系统钥匙串等已有操作系统服务不属于 Penguin 新增的 daemon。远端 Redis 服务当然需要在线。

SSH 被明确启用时可以产生受控子进程；MCP 被明确启用时可以由上层客户端启动 `prc` 子进程。它们都不是日常 CLI 的先决条件，也不在正常退出后偷偷驻留。

无连接参数且 stdin/stdout 都是 TTY 时，`prc` 打开本机连接选择器；没有连接时直接进入首次添加向导。选择器不探测所有地址、不读取全部凭证、不伪造当前延迟。

<a id="s03"></a>
## 03 · 命令契约：短、稳定、不重新发明 Redis

### 3.1 常用入口

```bash
prc                                      # 本机连接选择器
prc --add dev                            # 首次录入 URL / 逐项填写
prc --edit dev                           # 修改连接与凭证引用
prc --list                               # 仅列本机配置
prc @dev                                 # 默认 REPL
prc @r2 HGETALL player:10001              # 一次查询
prc @dev -n 3                            # 本次覆盖 DB，不修改保存配置
prc @dev --tui                           # 明确进入 TUI，使用同一套补全
prc --learn hash                         # 离线学习/查命令，不连接 Redis
prc --demo                               # 纯本地 fixture，展示表格与输入样例
prc -h 127.0.0.1 -p 6379 PING             # 熟悉的直连方式
prc @dev --raw GET config | jq .
prc @dev --json HGETALL player:10001 | jq '.status'
```

这些是拟实施命令，不是当前可执行的产品发布说明。安装器不得覆盖用户现有的 `prc`，也不得自动 alias 掉 `redis-cli`。

### 3.2 顶层解析语法

```text
prc [client-options] [@profile] [client-options] [--] COMMAND [redis-arguments...]
```

进入 `COMMAND` 后，后续 token 全部属于 Redis；不再解析客户端 flags。

```bash
prc @dev SET example '--raw'   # '--raw' 是 value
prc @dev SET example '@prod'   # '@prod' 是 value
prc @dev -- GET example       # '--' 结束客户端参数区
```

`-h` 始终保留 host 含义，帮助使用 `--help`。Shell 已处理过的 argv 不再执行第二轮 shell 展开。REPL 使用与官方语法对照测试的独立 tokenizer，而不是直接采用 shell parser。[R01]

字节级 NUL 等不能普遍通过操作系统 argv 传入的内容走 stdin 或专用二进制输入；不能以“所有数据都是 argv string”作为设计前提。

**二进制参数的唯一入口（v2.1 明确）。** 普通 REPL 编辑器与 shell one-shot 的 argv 是 **UTF-8 文本**；无效 UTF-8、NUL 与任意字节只能通过下列受测路径进入请求，且每条路径都产出精确 `Vec<Bytes>`：

| 入口 | 语法 / 形式 | 作用范围 |
|---|---|---|
| 双引号转义 | `"\xHH"`、`"\n"`、`"\r"`、`"\t"`、`"\a"`、`"\b"`、`"\\"`、`"\""`（与 redis-cli 双引号语义一致，固定 fixture 校验）[R01] | REPL、one-shot、`:send` |
| `-x` | 最后一个参数从 stdin 原样读取全部字节 | one-shot |
| `-X <tag>` | 将 stdin 内容替换命令中的 `<tag>` 占位 | one-shot |
| `:send --arg-file N=<path>` | 第 N 个参数从文件读取原始字节 | REPL |
| `:send --arg-hex N=<hex>` / `--arg-base64 N=<b64>` | 第 N 个参数按编码解码 | REPL |
| Guide 二进制槽位 | 内建 hex/base64/文件选择器，预览 bytes 与长度 | Guide |

编辑器中的转义只在**提交时**由权威 tokenizer 解码；显示层永远展示转义形式，不把原始控制字节送进终端。补全候选若源自含无效 UTF-8 的观察名，其 `edit` 字段插入转义形式而不是原始字节，且标注 `binary name`。

### 3.3 目标解析与安全覆盖

优先级：显式连接描述 > 显式 profile > 用户显式配置的默认连接 > 本地兼容回退。

但 profile 与另一完整 URL 不能默默混合；同时提供时应报冲突。主机、端口或身份发生覆盖时，不自动把原 profile 的秘密信息发往新目标；必须重新绑定凭证或明确确认可信目标。

**profile 与命令行 flag 的组合矩阵（v2.1 明确）：**

| 与 `@profile` 同时出现的 flag | 行为 |
|---|---|
| `-n <db>` | 本次覆盖 DB；不写回配置；Cluster profile 上非 0 值报错 |
| `--timeout`、`--color`、`--output`、`--raw`、`--json`、`--plain`、`--tui`、`-2` / `-3` | 本次覆盖，属于客户端行为，不影响目标与凭证 |
| `--tls-verify=strict`、`--ca <file>` 收紧类选项 | 允许，只能比 profile 更严格；放松类选项（关闭校验）报错 |
| `-h`、`-p`、`-s`（socket）、`-u <url>`、`--user`、`--askpass` | **冲突，退出码 2**：这些改变目标或身份；要改目标请另建 profile 或使用不带 `@profile` 的直连形式 |
| `--sentinel`、`--cluster` 模式切换 | 冲突，退出码 2 |
| `--compat=redis-cli` | 允许；不改变 profile 的安全策略（§3.4） |

直连形式（无 `@profile`）下，`-h/-p/-u/--user` 等按 redis-cli 语义解析，凭证只来自 `--askpass`、`REDISCLI_AUTH` 兼容变量或 URL 的安全隐藏输入；不会为直连形式自动搜索任何已保存 profile 的秘密。

生产连接不允许仅凭“最近用过”成为默认目标。批处理、非交互自动化要求显式目标；无参数 stdin 模式的本地回退仅在明确的兼容行为下启用。

### 3.4 两层兼容，而非一句“100% compatible”

| 层级 | 目标 |
|---|---|
| Redis command semantics | 按实际 argv 和服务器回复执行；保留错误、顺序、类型、状态。 |
| `redis-cli` executable compatibility | 对发布清单中的具体版本，测试 flags、raw/JSON、stdin、特殊模式和退出码。 |
| Penguin modern defaults | TTY 使用表格、连接选择器、保护策略、清晰错误；这些是公开的 UX 差异。 |

提供 `--compat=redis-cli` 用于既有脚本迁移。该模式按声明的 baseline 调整默认协议、格式与退出行为，但**不能成为绕过 profile 安全策略的后门**。

只有支持矩阵中全量通过的版本与模式才称兼容。未实现的 flag 明确报错，不忽略，不偷偷退回另一模式。

### 3.5 本地增强操作使用冒号命名空间

| 本地命令 | 含义 | 是否会访问 Redis |
|---|---|---|
| `:connections` | 选择或管理连接 | 选择目标后才会 |
| `:use r3` | 明确切换连接 | 是 |
| `:browse` / `:b` | 打开终端工作台 | 按选择的页面与策略 |
| `:inspect` / `:i` | 查看最近保留的结果 | 默认否 |
| `:inspect --field config` | 展开最近 Hash 结果中的字段 | 默认否 |
| `:inspect --key player:10001` | 明确读取指定 key 的增强信息 | 是 |
| `:view table` / `:view json` | 重新呈现最近结果 | 否 |
| `:refresh` | 重读当前上下文 | 是，明确显示范围 |
| `:copy --field config` | 复制选定值 | 否；调用明确允许的剪贴板通道 |
| `:tasks` | 查看当前进程内任务 | 否 |
| `:help` / F1 | 查命令、参数和快捷键 | 默认离线 |
| `:find` / F2 | 按用途找 Redis commands 或本地功能 | 默认否 |
| `:guide` / F3 | 分步填写真实命令，插入编辑框 | 默认否 |
| `:assist` | 提示密度、说明语言、状态 | 否 |
| `:complete` / F6 | 候选状态与明确的远端发现 | fetch 类 action 才会 |
| `:history` | 查看当前安全作用域内的命令历史 | 否 |

**消除之前示例的歧义：不使用裸 `:inspect config` 同时表示 key 和 field。** 历史脚本可以另有弃用提示，但新规格只保留明确语法。

**本地命令有自己的 tokenizer（v2.1 明确）。** `:` 开头的输入不经过 Redis 请求 tokenizer，而由 `pr-repl` 的 local-command 解析器处理：

- 词法：空白分隔；单引号内字面量；双引号内支持与 §3.2 相同的转义集；`--` 之后全部为位置参数。
- 无 shell 展开：不处理 `$VAR`、`~`、glob、反引号或命令替换。
- 选项：`--name value` 或 `--name=value`；布尔选项无值；重复选项报错，不静默取后者。
- 自由文本参数：仅 `:find` 与 `:help` 接受尾部自由文本；自由文本是整段剩余输入，不再分词，用于用途检索。
- 输出：解析结果是 `LocalCommand { name, options: BTreeMap<String, Bytes>, positionals: Vec<Bytes> }`，所有需要远端范围的参数（如 `:complete keys --prefix`）以 bytes 原样传递给 discovery 计划，并在计划预览中回显其字节与施加的变换（见 §12.4）。
- 每个本地命令在 `pr-repl` 中登记 `LocalCommandSpec`（选项、类型、是否访问 Redis、是否需要审批），与 §3.5 表一一对应，并有 parser fixture 与安全测试。

### 3.6 本地命令保留字与兼容边界

在 compatibility manifest 中逐项登记官方 REPL 的 HELP、CONNECT、CLEAR、EXIT/QUIT、`:set hints` 等本地行为，不能假设所有输入都是服务器命令。[R01] 与 Penguin 的 `:` 命名空间冲突时，保留有文档的兼容映射；帮助不自动变更保存 profile 的秘密绑定。

提供明确的通用发送入口 `:send -- COMMAND [args...]`，用于避开本地命令名解析；仍经过相同 kernel/policy。Shell one-shot 的 command 区域按 argv 发送，不被 REPL 的本地别名劫持。`:send` 不是绕过审批、掩码或字节校验的后门。

**帮助检查与执行语义分离。** 自动建议可以提示未知语法；是否发送明确手写的请求由原始 tokenizer、用户选择和安全策略决定，不能靠旧 catalog 否定服务器新能力。

<a id="s04"></a>
## 04 · 连接管理：保存一次，不重复复制 credential

### 4.1 首次录入

```text
$ prc --add dev

Name                 dev
Paste connection URL [hidden input]

Host                 redis-dev.example.internal
Port                 6379
Database             3
TLS                  enabled
Username             developer
Environment          development
Credential storage   system secure store

Action: Test and save
```

Redis URI 与 TLS 的 `rediss` 形式已有官方定义。[R01] 录入支持用户名、密码、百分号编码、IPv6、DB 与 TLS；错误时逐项提示，不把含密码 URL 打印到错误日志。

推荐通过隐藏输入粘贴 URL，而不是放在 shell 参数里。不要声称可以清除用户在其他终端、剪贴板管理器或操作系统日志里已有的痕迹。

### 4.2 保存成功必须真实

依次完成：解析 -> 本地验证 -> 用户确认测试 -> 建立连接 -> 凭证存储 -> 原子写入 profile -> 显示成功。

测试失败时允许“保存为未验证连接”，但状态必须写成未验证；钥匙串保存失败时不能提示“凭证已保存”。本地文件与钥匙串跨资源提交失败，要使用可恢复的补偿流程，避免留下误绑定的孤儿秘密记录。

**跨资源提交的确定顺序（v2.1 明确）：**

```text
1. 生成新的 secret_ref UUID（不复用旧 ID）
2. 追加 journal 记录 {op: bind, profile_uuid, secret_ref, state: pending}   -> fsync
3. 写入系统 secret store（key = penguin-redis/<secret_ref>）
4. 以 profile 的写锁（§30.2）读取 -> 修改 -> 写临时文件 -> fsync -> 原子 rename
5. journal 记录 state: committed                                             -> fsync
6. 若旧 secret_ref 不再被任何 profile 引用，删除旧 secret，journal 记录 state: cleaned
```

任一步失败即停止并保留 journal。**启动时 reconciliation**：`pending` 且 profile 未引用该 `secret_ref` -> 视为孤儿，列出并请求用户确认删除，不自动删除；profile 引用了不存在的 `secret_ref` -> profile 标 `credential missing`，连接前要求重新绑定；`committed` 但旧 secret 仍存在 -> 重试步骤 6。journal 文件与 profile 同目录、私有权限、只追加、有大小上限并在成功 reconcile 后截断。

### 4.3 数据拆分

| 数据 | 持久化策略 |
|---|---|
| 名称、endpoint、DB、环境标签、分组 | 本机配置，私有权限 |
| Password / token | macOS Keychain、Windows 凭证存储或 Linux 受支持 store |
| Secret reference | 只存不透明 ID，不编码真实秘密 |
| TLS 私钥 | 默认只引用受保护文件或系统身份，不复制进 profile |
| 历史与收藏 | 分连接/身份隔离，内容过滤 |
| 读取结果 | 默认会话内；明确保存后才持久化 |
| 诊断记录 | 默认元数据与脱敏结果 |

macOS 使用系统 Keychain，而不是自制“base64 加密”。Keychain 是 Apple 提供的秘密信息保护机制；访问仍可能触发系统解锁或授权。[R23]

无安全 store 的 headless 环境：支持安全输入、会话内秘密、明确批准的 secret-provider，或经过评审的加密 vault。不能悄悄退回明文文件；不要承诺无需解锁的磁盘加密秘密。

### 4.4 真正要支持的连接能力

连接分组、标签、收藏、最近使用、本地搜索、重复地址检测、多个 DB 复用凭证、连接别名、改名不改变内部 ID、克隆时默认不复制秘密、共享配置默认移除凭证。

支持独立的 Redis 和 Sentinel 身份，明确的 TLS client identity，SSH jump host，证书轮换，限时 token provider。所有 provider 默认惰性调用；启动选择器不唤醒全部 provider。

### 4.5 密码轮换体验

```text
AUTHENTICATION FAILED · @dev

Saved endpoint is unchanged.
The stored credential may have expired or been rotated.

Update credential | Retry manually | Cancel
```

不因一次失败重试多个旧密码，不自动寻找系统其他秘密尝试认证。更新密码后测试成功再替换原引用；引用被多个 profile 使用时显示影响范围。

### 4.6 安全导入

外部 profile 不得自动启用 shell 命令、secret provider、未知 SSH ProxyCommand、连接跳转或远端 AI 地址。先生成导入差异，用户批准后才转为可信本地配置。endpoint 变更必须重新审视已有凭证绑定。

<a id="s05"></a>
## 05 · 日常使用剧本

### 5.1 上班直接连接

```text
$ prc @dev

@dev · DB 3 · TLS verified

 dev:3 > HGETALL player:10001
```

连接 banner 只显示已知信息。版本未知就省略，延迟未测就不显示；不能为了漂亮自动增加 `INFO`、`TTL`、`MEMORY USAGE`。

### 5.2 Hash 默认输出

下面是黑白结构样例，实际主题为表头和 JSON token 上色。没有把 ANSI 控制字节直接写进 Markdown。

```text
player:10001 · @dev · DB 3 · 7 fields returned

╭──────────────────┬────────────────────────────────────────────────╮
│ FIELD            │ VALUE                                          │
╞══════════════════╪════════════════════════════════════════════════╡
│ playerId         │ 10001                                          │
├──────────────────┼────────────────────────────────────────────────┤
│ username         │ shieng                                         │
├──────────────────┼────────────────────────────────────────────────┤
│ platformId       │ 70                                             │
├──────────────────┼────────────────────────────────────────────────┤
│ status           │ ACTIVE                                         │
├──────────────────┼────────────────────────────────────────────────┤
│ balance          │ 12890.50                                       │
├──────────────────┼────────────────────────────────────────────────┤
│ currency         │ MYR                                            │
├──────────────────┼────────────────────────────────────────────────┤
│ config           │ {                                              │
│                  │   "notification": {                            │
│                  │     "enabled": true,                           │
│                  │     "channels": [                              │
│                  │       "FCM",                                   │
│                  │       "HUAWEI"                                 │
│                  │     ]                                          │
│                  │   },                                           │
│                  │   "theme": "dark"                              │
│                  │ }                                              │
╰──────────────────┴────────────────────────────────────────────────╯

7 fields · 1 JSON object view · original values preserved
```

这里有 **7 个字段、1 个 JSON 对象视图**。`balance` 保留原始字符串 `12890.50`，不会自动变成带千位分隔符的 `12,890.50`。

### 5.3 深入查看已有结果，不重新读取

```text
 dev:3 > :inspect --field config

Result #17 · @dev · HGETALL player:10001
Source: retained response · no new Redis request
View: JSON · original bytes available
```

只有用户执行 `:refresh` 或 `:inspect --key ...` 才发起新的读取。若旧结果已因预算被淘汰，就说明“未保留”，不能假装还在；提供明确重读入口。

### 5.4 切换 Redis2 / Redis3

```text
 r2:0 > :use r3
Switching @r2 -> @r3
Connected · idle session ready

 r3:0 > HGETALL promotion:70
```

如果当前存在事务、未完成写入或阻塞操作，不直接切换目标。先保留原会话并提示处理选择，或由用户明确另开会话。

### 5.5 脚本与退出

```bash
prc @dev --raw GET platform:70 | jq '.payment.provider'
prc @dev --json HGETALL player:10001 | jq '.status'
```

```text
 dev:3 > exit
Connections closed. No Penguin background service remains.
```

退出不是删除保存连接。钥匙串仍保留已授权保存的秘密，但不会因为保存了一份配置就保持 Redis TCP 连接。

<a id="s06"></a>
## 06 · Table 设计系统：把用户已经确认的视觉固定下来

### 6.1 默认样式

| 元素 | 规则 |
|---|---|
| 外框 | 圆角或普通框；淡色，不抢内容 |
| 表头 | 加粗、独立背景/前景、`FIELD` / `VALUE` 等真实列名 |
| 表头底线 | `╞════╪════╡`，不同于数据分隔线 |
| 数据行 | 每个逻辑 field 之间都画 `├────┼────┤` |
| 多行 JSON | 多行属于同一个 cell，不能每行再画分隔线 |
| Field 对齐 | 左对齐、顶对齐，长字段名可换行 |
| Value 对齐 | 默认左对齐；只有真实协议数字列才可右对齐 |
| 单元格间距 | 左右各 1 个字符；不默认插入大块空白 |
| 计数 | `returned` / `shown` / `retained` 分开，不虚构总数 |
| 页脚 | 采样、截断、过期、脱敏、部分失败均明确标注 |

表头有色时仍保留双线。无色终端不能失去层级。字段本身可加粗，但强度低于表头；不把整张表的边框染成最亮色。

### 6.2 宽度算法

根据当前 terminal 的显示列数，而不是 UTF-8 字节数或 ANSI 字符串长度布局。field 初始宽度使用有限采样估计并设置上下界，value 获得剩余空间；不是永远固定成 30% / 70%。

流式输出不能等到全部行收到后才决定列宽。先确定稳定布局并冻结当前输出片段；后续长字段换行。在 TUI 中可因 resize 重排，在普通 scrollback 中不回头改写数百行历史。

建议行为：正常宽度用双列表格；较窄时减少 field 宽度；极窄时变成带字段标题的纵向块。尽量消除横向滚动，但 TUI 对特别宽的多列表格保留明确的横向查看选项，不能以“绝不横滚”为理由隐藏必要列。

### 6.3 多行 JSON 的分隔规则

```text
│ config  │ {                         │
│         │   "enabled": true,        │
│         │   "channels": [           │
│         │     "FCM",                │
│         │     "HUAWEI"              │
│         │   ]                       │
│         │ }                         │
├─────────┼───────────────────────────┤
│ status  │ ACTIVE                    │
```

JSON 内部行不构成新 field。不要把空白 field 单元格误导出为额外字段。

### 6.4 表头上下文

结果上方展示 key、连接、DB、推断的视图类型和**本次已确认返回数量**。只知道收到 20 条，就显示 `20 entries returned`，不能写成 `stream has 20 entries`。

从命令与成功回复推断出的视图类型应与服务器验证的 `TYPE` 元数据区分。空 `HGETALL` 只能说明没有返回字段，不能凭这一条结果展示“确认存在的空 Hash”。

### 6.5 特殊单元格

空字符串显示明确的 empty 标记；nil 使用独立的 `(nil)`；缺失列显示 `missing` 注解；JSON 的 `null` 保留 JSON token 样式。注解要与字面量文本区分，原始字符串恰好是 `"(nil)"` 时不能混淆。

超长 field、换行、Tab、中文、组合字符、emoji 和 RTL 控制符都必须进入 renderer 测试。默认 plain/pretty 视图转义不可见控制字符，并可显示空白可视化视图。

<a id="s07"></a>
## 07 · Colour：可用、克制、可配置

### 7.1 Penguin Dark 主题 token

颜色是拟定主题，不假设用户当前 terminal 背景。提供 dark/light/system/high-contrast/mono；无法可靠判断背景时使用保守的 terminal 色槽，允许显式选择。

| Token | 建议 Dark 色值 | 用途 |
|---|---|---|
| `surface` | `#111827` | TUI 背景；普通 CLI 不强制改整个终端 |
| `text` | `#E5E7EB` | 正常数据 |
| `muted` | `#9CA3AF` | 来源、采样和视图注解 |
| `border` | `#64748B` | 单线框与行分隔 |
| `title` | `#67E8F9` | key 和结果标题 |
| `header_bg` | `#164E63` | 表头单元格背景 |
| `header_fg` | `#ECFEFF` | 加粗表头文字 |
| `json_key` | `#7DD3FC` | JSON 属性名 |
| `json_string` | `#A7F3D0` | JSON 字符串 |
| `json_number` | `#FDE68A` | JSON 数字 |
| `json_boolean` | `#C4B5FD` | JSON true/false |
| `json_null` | `#CBD5E1` | JSON null |
| `warning` | `#FBBF24` | 明确警告 |
| `error` | `#FCA5A5` | 错误文字 |
| `production` | `#FDA4AF` | 环境标记，与 PRODUCTION 文本同时存在 |

**Penguin Light 主题 token（v2.1 补）：**

| Token | 建议 Light 色值 | 说明 |
|---|---|---|
| `surface` | `#FFFFFF` | TUI 背景 |
| `text` | `#1F2937` | 正常数据 |
| `muted` | `#6B7280` | 注解 |
| `border` | `#9CA3AF` | 框线 |
| `title` | `#0E7490` | key 与标题 |
| `header_bg` | `#CFFAFE` | 表头背景 |
| `header_fg` | `#164E63` | 表头文字 |
| `json_key` | `#0369A1` | JSON 属性名 |
| `json_string` | `#047857` | JSON 字符串 |
| `json_number` | `#B45309` | JSON 数字 |
| `json_boolean` | `#6D28D9` | JSON true/false |
| `json_null` | `#475569` | JSON null |
| `warning` | `#B45309` | 警告 |
| `error` | `#B91C1C` | 错误 |
| `production` | `#BE123C` | 生产标记；亮底上必须为深色 |

**High-contrast 主题**：只使用 16 色槽位中的 `bright white / black / bright yellow / bright red / bright cyan`，表头以反色（reverse video）+ bold 表示，不依赖背景色；所有语义差异同时以文字标签体现。每套主题都必须通过 §7.3 的相邻语义可分辨测试与 WCAG 对比度 ≥ 4.5:1（正文）/ 3:1（大号标题）的自动校验。

真实 Hash value `70` 是字节字符串，默认不能只因“看起来是数字”就使用真正数字类型的配色。只有 RESP 数字或已解析 JSON 中的 number 用 number token。[R02]

`ACTIVE` / `PENDING` / `FAILED` 默认是业务字符串。按成功/警告/失败上色必须来自用户明确配置的领域规则，且不能改变导出的值。

### 7.2 层级和回退

24-bit colour -> 256 colour -> 16 colour -> mono。颜色量化后仍测试相邻语义是否可分辨。高对比主题优先清晰，而不是坚持品牌色。

支持 `--color auto|always|never`，`--no-color` 等价于 `never`。`auto` 遵守非空 `NO_COLOR` 的约定并默认不对非 TTY 输出 ANSI；显式命令行选项的覆盖关系写入测试。[R24]

`--no-color` 只说明颜色策略；另提供 `--plain` 禁用颜色、装饰、emoji 和 Unicode 框线，适合简单终端与辅助阅读。`TERM=dumb` 默认采用 plain，不用复杂终端控制序列。

### 7.3 必须测试的真实体验

macOS 常用终端、Linux PTY、SSH、tmux、Windows Terminal、暗色与亮色背景；表头背景只覆盖所在单元格，不能泄漏到下一行。每个样式片段必须正确 reset，管道结果不能夹带 escape bytes。

Markdown 不保证渲染 ANSI。本文只存文本样例与主题 token，真正彩色样例在工程中的 `prc --demo` 使用**纯本地 fixture**展示，不连接 Redis。

<a id="s08"></a>
## 08 · JSON、长值与二进制：好看不能牺牲原始数据

### 8.1 三个对象必须分开

```text
Redis bytes           原始权威数据，不修改
Presentation model    对命令结果的语义解释
Display view          Table / JSON / Tree / Hex / Preview
```

Redis bulk string 是 binary-safe，不保证 UTF-8；RESP 同时存在字符串、数字、数组、map 等协议类型。[R02] 因而不能用 `HashMap<String, String>` 充当所有结果的统一模型。

自动 JSON 识别默认只针对**完整有效的 object 或 array**。普通文本 `70`、`true`、`null` 不自动成为另一种类型。解析失败、预算耗尽或输入不完整时，回到 escaped text/binary 视图，不把 renderer 失败说成 Redis command 失败。

### 8.2 单元格内 pretty JSON

小 JSON 默认多行、2 空格缩进、原属性顺序、语法着色；数组也按可读行展示。注解 `view: JSON object` 放在非数据区域或可辨识的 dim annotation 中，不进入复制内容。

长 JSON 不默认静默省略。普通 CLI 默认输出完整表格。用户显式启用 preview 后才显示折叠，并在表外写清：`preview only; value retained` 或 `value not fully retained`。

**超过交互阈值时的唯一顺序（v2.1 明确，避免「已输出一半再询问」）：**

| 情况 | 行为 |
|---|---|
| 回复是单个 bulk string，RESP 已声明长度 > 阈值 | **首字节输出前**询问：完整输出 / 只保留供 `:inspect` / 授权落盘 spool / 取消。取消时 drain 或关闭专属连接 |
| 回复是 aggregate（长度未知）且为 TTY | 立即开始流式表格输出；同时按 §24.3 有界保留；超出保留预算后继续输出但页脚标 `not fully retained`；**不在中途弹出询问** |
| 非 TTY / `--raw` / `--json` / typed | 永不询问；按 §24.4 持续写出或以非零状态失败 |
| TUI | 只绘制可见区域；获取范围与保留范围分别标注 |

这意味着「完整可回看」只对首字节前已知长度并被授权 spool 的结果成立；流式 aggregate 的可回看范围就是保留范围，UI 必须如实显示，不能暗示能回看已滚过但未保留的行。

TUI 则默认只绘制可见行，支持展开和折叠；绘制少不等于只获取了少量数据，获取范围另行标注。

### 8.3 无损格式化规则

JSON 规范不要求 object 的成员名称绝对唯一，并指出不同实现对重复成员可能有不同处理；数字精度也有互操作边界。[R25] 本项目必须明确处理而不是忽略。

| 输入情况 | 必须采取的行为 |
|---|---|
| `"000123"` | 保持字符串，不能数值化 |
| `9007199254740993` | 不转成丢精度的浮点数 |
| `1.2300` / `1e3` / `-0` | 默认保留数字 token 的原写法 |
| 重复 JSON 成员名 | 保留次序与全部成员；提示重复，禁止普通 map 默默覆盖 |
| `\uXXXX`、emoji、转义换行 | 解码视图可选，原文本可复制 |
| 深层 JSON | 明确的深度/时间预算，达到预算后安全回退 |
| JSON 内嵌另一段 JSON 字符串 | 默认不递归解码；显式启用并标明层次 |
| 压缩、Base64、MessagePack、Protobuf | 显式解码，不盲猜；解压尺寸与 CPU 都设上限 |

推荐对 JSON 使用可保留词法 token 的格式化器，或保留原始片段后建立索引。`serde_json::RawValue` 可保留原始 JSON 文本片段，但并不自动提供完整的无损语法树或所有编辑语义。[R26]

**权威 JSON 模型（v2.1 明确）：** 上述规则**不可能**用 `serde_json::Value` 或任何 `Map<String, _>` 满足；因此新增 `pr-json` crate（§31.3）实现 token-preserving DOM：

```text
JsonNode
  span: ByteRange                  // 在原始 bytes 中的位置
  kind: Object | Array | String | Number | Bool | Null
  Number  -> lexeme: Bytes         // 原词法，不转 f64
  String  -> raw: Bytes, decoded: Option<String>
  Object  -> members: Vec<(KeyNode, JsonNode, occurrence: u32)>  // 保序，重复保留
```

- **寻址**：路径语法 `$.a`、`$.a[2]`、`$["a"]` 之外新增 occurrence 选择 `$.a#2`（第 2 次出现，从 1 起）。对存在重复成员的对象，不带 `#n` 的路径**拒绝解析**（`ambiguous member`），Tree 视图、编辑器、diff、schema 校验与 Undo 都只接受带 occurrence 的路径。
- **展示**：Tree 视图为重复成员显示 `a (1/2)`、`a (2/2)`。
- **编辑**：修改只重写目标 span 的 bytes，其他 token 逐字节保留；格式化输出是单独的视图操作，不回写。
- **格式化**：pretty 视图使用 DOM 重排空白，不改 lexeme；`--json` 投影（§18.3）对重复成员的行为在 projection 表中定义，不由 renderer 决定。
- `serde_json` 仅允许用于 Penguin 自身的配置/typed 输出等**自有** schema，禁止用于服务器返回的业务 JSON。

### 8.4 复制与导出

`:copy --field config` 默认复制原始 field value 的文本字节解释，不带表框、颜色、类型注解；不是复制屏幕里的折行文本。

明确提供 `:copy --field config --pretty` 用于复制格式化 JSON；二进制复制要求选 `hex` / `base64` 或写文件。无法安全表示在剪贴板时不做隐式替换。

本机剪贴板优先；通过 SSH 的 OSC52 必须由用户启用，限制长度并明确数据会进入哪台机器的剪贴板。不假装可以可靠撤回其他应用已读取的 clipboard 内容。

### 8.5 视图不是 schema

表格展开 JSON object list 时，保留原字段名与行索引；不能把缺失字段当成 `null`，不能把重复字段覆盖。原始记录随时可检查。显示 sorting/filter 仅作用于已获取结果，排序方向与作用范围始终可见。

<a id="s09"></a>
## 09 · Renderer 全面覆盖与正确性规则

### 9.1 返回值到视图的分派

| 命令族 | 默认视图 | 特别要求 |
|---|---|---|
| `GET`、`GETRANGE` | 文本 / JSON / bytes preview | 不额外读取 TYPE；缺失、空值分开 |
| `MGET` | Key / Value table | 保留请求顺序、重复 key、nil |
| `HGET` | 单值；可配置 table | field 名来自请求上下文 |
| `HMGET` | Field / Value table | 保留重复请求字段与缺失项 |
| `HGETALL` | Full-row-separated table | RESP2 pairs / RESP3 map 分别解码 |
| `HSCAN` | Field / Value + cursor | 支持该版本的 flags，不能一概假设每项成对 |
| `LRANGE` | Position / Value table | 索引无法确定时仅标返回序号 |
| `SMEMBERS`、`SSCAN` | Member table | 不暗示原集合有稳定排序 |
| `ZRANGE ... WITHSCORES` | Position / Member / Score | 顺序来自命令，不能自创排名 |
| `ZSCAN` | Member / Score + cursor | 扫描顺序不是分数排名 |
| `XRANGE`、`XREVRANGE` | Entry ID + 原字段名 | 不丢字段顺序和重复字段 |
| `XREAD`、`XREADGROUP` | Stream / Entry / Fields | 消费组读取可能改变状态 |
| `XINFO`、`XPENDING` | 命令对应的结构化表 | 各种子命令使用独立 schema |
| `INFO`、`MEMORY STATS` | 分组 metric tables | 原指标名、单位、数据来源可追溯 |
| `CLIENT LIST` | 每 client 一行 | 未知字段保留，不依赖固定列数 |
| `SLOWLOG GET` | Entry / duration / argv | argv 保持字节与边界 |
| `CLUSTER NODES`、`CLUSTER SHARDS` | 节点/槽位/状态 | 不把字符串解析失败当成空集群 |
| `COMMAND INFO`、`COMMAND DOCS` | 嵌套结构/参数帮助 | 版本不认识的字段保留 |
| 普通整数、状态、错误 | 精简 scalar renderer | 保留原始值与错误类别 |
| 未知/module 命令 | Generic RESP tree | 能执行不等于已有专用 renderer |

`HGETALL` 的 RESP2 输出由 field/value 成对排列，RESP3 使用 map 形态。[R03] 识别语义时结合命令上下文和协议，而不是“遇到偶数长度数组就当 Hash”。

### 9.2 HSET 的成功解释

`HSET` 的整数返回值是新增字段数量，不是修改次数。[R04]

```text
 dev:3 > HSET player:10001 status ACTIVE
(integer) 0
New fields: 0
```

不能显示失败，也不能凭返回值推断“3 个值发生了变化”。同样，返回整数 0 的其他命令必须按各自语义解释，不统一渲染成红色失败。

### 9.3 ZSET：修正容易误导的示例

`ZRANGE` 默认按升序返回；`REV` 才反转。默认 index 是从 0 开始，`BYSCORE` / `BYLEX` 又有不同边界含义。[R05]

```text
ZRANGE leaderboard 0 2 REV WITHSCORES
```

```text
╭─────┬──────────────────┬──────────╮
│ POS │ MEMBER           │    SCORE │
╞═════╪══════════════════╪══════════╡
│   1 │ player:10001     │    92881 │
├─────┼──────────────────┼──────────┤
│   2 │ player:92831     │    88291 │
├─────┼──────────────────┼──────────┤
│   3 │ player:82192     │    81203 │
╰─────┴──────────────────┴──────────╯

3 members returned · order: REV
```

表头用 `POS` 表示本次结果中的位置，而不是无条件称为全局 `RANK`。只有能从查询准确推导绝对 index 时才增加 `INDEX`；不能为了这个装饰列额外请求集合长度。

### 9.4 Streams：表格 pivot 不能丢数据

结构一致且没有歧义时，使用 entry ID 加字段列；异构记录、重复字段或过宽内容时，改为每条 entry 的 Field / Value 子表。原始字段名称不能从 `missionId` 擅自改成 `MISSION` 后再用该标题导出。[R06][R07]

### 9.5 兼容层失败时

专用 renderer 不识别结果版本 -> 显示 generic RESP -> 保留原始结果 -> 给本地诊断 ID。不要 panic、不要空白输出、不要因此重新执行命令。

<a id="s10"></a>
## 10 · 智能输入体验：记不住命令，也能顺着提示完成工作

### 10.1 产品定案：Command Intelligence 是核心能力

**默认 CLI REPL 和 TUI 的 Redis 命令输入框，都提供自动建议下拉框、当前参数提示、离线命令解释和可发现的帮助入口。** 不要求用户先按 Tab 才知道有哪些命令，也不要求打开 TUI 才获得补全。One-shot 的 shell 行使用另一个受限 completion adapter，不冒充交互 REPL。

这不只是“输入 `H`，显示一串以 H 开头的单词”。目标是帮助用户连续回答：

> 我要做的事对应什么命令？现在该填 key、field 还是 value？还有哪些合法选项？这个参数是什么单位？会不会写数据？为什么失败？下一步该怎么查？

官方 `redis-cli` 已有命令名 Tab completion、syntax hints 和历史搜索；IRedis 也已提供基于输入/响应历史的增强补全。[R01][R21][R50] Penguin 的差异目标是把这些基线统一成**有上下文、有版本/权限状态、有中英文用途搜索、可解释且不执行建议的输入系统**。

### 10.2 一套帮助，按需要逐渐展开

| 用户遇到的困难 | 默认提供的帮助 | 不需要做的事 |
|---|---|---|
| 只记得 `HG` | 自动下拉：命令名 + 一句话用途 + 读写标签 | 打开网页查列表 |
| 不知道命令名 | F2 / `:find` 按中英文用途查找 | 先猜对首字母 |
| 不记得参数顺序 | 光标所在参数高亮、剩余参数和示例 | 背整条语法 |
| 不记得 key | 当前来源中已观察过的 key 候选 | 每次复制 key |
| 不记得 Hash field | 绑定当前 key 的已知 field 候选 | 先重新 HGETALL |
| 不懂 EX / PX | 秒 / 毫秒说明与不执行的示例 | 靠试错猜单位 |
| 不确定选项组合 | 互斥项说明、重复项提示、版本标记 | 发送错误命令探测 |
| 命令写得复杂 | `:guide` 分步构建，最后看真实命令 | 使用另一种执行语言 |
| Redis 返回错误 | 解释 + 可选检查步骤；不自动重发 | 复制错误找答案 |
| 不想被打扰 | Minimal / Manual / Off 提示密度 | 放弃高级能力 |

### 10.3 普通 CLI：输入时出现真正的下拉框

以下为**拟实施交互样例**；`▏` 表示光标，不属于输入。候选说明可以用中文，Redis 命令与实际参数不翻译。

```text
$ prc @dev

dev:3 > HG▏

╭────────────┬───────────┬────────────────────────────────────╮
│ COMMAND    │ EFFECT    │ DESCRIPTION                        │
╞════════════╪═══════════╪════════════════════════════════════╡
│ HGET       │ READ      │ 读取 Hash 中一个字段               │
├────────────┼───────────┼────────────────────────────────────┤
│ HGETALL    │ READ      │ 读取 Hash 的全部字段和值           │
╰────────────┴───────────┴────────────────────────────────────╯

Tab fill first · Down select · F1 help · F2 find · Esc dismiss
```

这是前缀示例，所以 `HG` 不把 `HMGET`、`HSCAN` 当成前缀命中；它们可以在“相关命令”中显示，但必须单独分组。较新命令只按已知能力展示，不能固定只支持此处的两个条目。

下拉框最多显示一小页，不把全部 catalog 展开。默认 6 条；更多可滚动。每个建议包含来源/能力状态，例如 `built-in docs`、`observed on this connection`、`availability unknown`。没有检查服务器 ACL 时，`READ` 只是命令效果分类，不表示“已授权”。

### 10.4 选中命令后：提示当前参数

```text
dev:3 > HGETALL ▏

HGETALL <key>
         ^^^ 当前参数：Hash key

Known in this session · @dev · DB 3 · not a live listing
  player:10001       observed HASH · 2 min ago
  platform:70        observed HASH · 5 min ago

Tab fill · Down select · F1 explain · F6 discover (network)
```

候选来自当前会话已经观察的名字；提示框不因此额外发 `KEYS`、`SCAN`、`TYPE` 或 `HGETALL`。没有已知 key 时显示“暂无本地候选”，用户仍可直接输入任意合法 key。`F6` 是明确的远端发现入口，只有策略允许并满足批准条件时才请求。

### 10.5 Field 建议要跟着 key，不是全库混在一起

```text
dev:3 > HGET player:10001 no▏

HGET <key> <field>
            ^^^^^ 当前参数：字段名

Fields from result #17 · player:10001 · @dev / DB 3
  notificationEnabled
  notificationConfig

No extra Redis request · values are not shown in suggestions
```

`player:10002` 的字段不能冒充 `player:10001` 的字段。可显示显式配置的 schema 提示，但标成 `schema suggestion`，不标成已存在字段。用户输入新的 HSET field 也必须被允许。

### 10.6 参数与选项也要有解释

```text
dev:3 > SET cache:demo hello EX ▏

SET <key> <value> EX <seconds> ...
                     ^^^^^^^ 当前参数：正整数，单位秒

Examples, not defaults: 60 = 1 minute · 300 = 5 minutes
EX 已选择；其他过期选项不可同时使用。
```

例子中的数值不会自动填入，更不会替用户选择过期策略。`SET` 有条件选项组与过期选项组；选择器必须按目标命令版本处理合法组合，而不是固定一个过时的字符串列表。[R42]

```text
dev:3 > SET cache:demo hello EX 300 ▏

Next options · excerpt, filtered by the selected server capability
  NX       仅在 key 不存在时写入
  XX       仅在 key 已存在时写入
  GET      返回旧值；不会把写命令变成只读

F1 full syntax · Tab fill · Enter sends only the actual input
```

这是候选片段，不宣称最新 SET 只有 NX/XX/GET。当前官方文档还列出带版本历史的比较条件选项；catalog 与已连接目标共同决定是否提供。[R42]

### 10.7 TUI 中同样存在，不是较弱的另一套

```text
Penguin Redis · @dev · DB 3                           Command editor
────────────────────────────────────────────────────────────────────
Result: player:10001                   Result source: retained #17

FIELD                 VALUE
notificationConfig    { ... }
status                ACTIVE

────────────────────────────────────────────────────────────────────
Command > HGET player:10001 no▏
          ╭──────────────────────────────────────────────╮
          │ FIELD                         SOURCE         │
          ╞══════════════════════════════════════════════╡
          │ notificationEnabled           result #17     │
          │ notificationConfig            result #17     │
          ╰──────────────────────────────────────────────╯
HGET <key> <field> · Tab fill · F1 help · Esc close suggestions
```

CLI 与 TUI 共用解析、候选、排序、权限状态和帮助内容，只分别绘制。结果区域依然使用已经确定的**行横线、强化表头、cell 内彩色 JSON**；输入菜单不会取代这些结果样式。

### 10.8 不能把“建议”变成“代操作”

**接纳候选只编辑输入缓冲区。** 它不执行 Redis command，不切生产环境，不修复数据，不重试上次未知结果。候选聚焦时 `Enter` 只接纳；必须进入正常输入状态后另一次提交才执行。自动显示但未聚焦的提示不抢占 Enter，Enter 仅提交真实输入，不包含 ghost text。

详细按键与焦点规则见[终端输入契约](#ci04)。不要将“按一次 Enter”同时绑定“补全 + 发送”。

### 10.9 显示密度，而不是高低配产品

| 模式 | 行为 |
|---|---|
| Assisted，默认 | 自动下拉 + 一行签名；F1 展开更多说明 |
| Minimal | 当前参数提示；Tab 打开完整候选 |
| Manual | 不自动弹出；Tab/F1/F2 随时调用 |
| Off | 不显示智能提示；原命令输入、显式帮助仍可用 |

通过 `:assist assisted`、`:assist minimal`、`:assist manual`、`:assist off` 设置；`:assist language zh-CN` 改说明语言，`:assist status` 显示网络/缓存/隐私状态。仅非交互执行不初始化这套 UI。

自动弹出建议延迟目标约 75 ms，可配置；本地结果应先出现，远端慢不能阻塞输入。用户手动关闭某个菜单后，不在同一个未变化的 token 上反复弹出。

### 10.10 历史：帮忙回忆，但不泄漏内容

`Ctrl+R` 搜索当前 connection ID、身份和 DB 的历史。默认保存读命令结构与允许的参数，不保存回复；写 payload、AUTH、token、含凭证 URL 不持久化。

**key 参数的默认脱敏策略（v2.1 明确）：** key、field、member、channel、group 名都可能是身份信息。按 `environment` 分级：

| 环境 | 默认持久化的历史内容 | 可改 |
|---|---|---|
| development / local | 命令 + 全部非 value 参数（key、field、选项） | 可收紧 |
| staging | 命令 + 选项；key/field 以 `[redacted:<8 hex of blake3>]` 占位 | 可放宽为明文 |
| production | 只保存命令名与选项 keyword；所有位置参数占位 | 不可放宽为明文 |
| 未分类 | 按 production 处理 | 分类后重新生效 |

占位符在历史回填时作为 snippet 空位（§10.10 尾段）。`history.sqlite` 文件权限 0600（Windows 为仅当前用户 ACL），默认保留 90 天或 10,000 条（先到者），可配置；`:history clear [--profile]` 删除记录后执行 `VACUUM`，并明确说明无法保证已被 OS/备份复制的副本消失。任何 history 记录都不进入诊断包、telemetry 或 AI 请求，除非用户逐次预览并确认。

历史选择先回填到编辑框；跨环境复用需要明确选择和来源标记。不能因为用户常执行某条生产写命令，就把它变成优先自动接受的建议。被脱敏的历史作为 snippet 的空位，不把 `[redacted]` 当真值发送。

### 10.11 Result Context：现有结果也是安全提示来源

每次执行生成 `ResultId`，绑定 profile ID、endpoint fingerprint、DB、身份 epoch、命令、获取时间、原始结果和保留范围。

`:view`、`:copy`、`:inspect --field config` 使用这个 context，不重新读取。字段建议可以读取它的已提取名称索引，不能为补全重新解析整棵巨大 JSON，也不能引用另一个来源而不说明。

结果被淘汰时明确 `not retained`。切连接、SELECT、AUTH、HELLO/AUTH、RESET、重连或权限配置变化都要处理作用域失效；未确认的身份状态期间关闭远端候选。HELLO 的协议/认证语义需纳入状态模型。[R53]

### 10.12 长任务仍然要有可读进度

```text
Task #23 · @r2 · discover keys
Stage       Querying primary 2/3
Observed    18,420 unique names within the selected scope
Requests    212 sent · 1 node unavailable
Scope       Observations only; not a global snapshot
```

这类任务只有显式启动后才存在。输入期间来自其他任务的通知由 terminal coordinator 排队，不能把半张 table 或 Pub/Sub 消息插进光标所在 token。进度不能用 elapsed time 伪造百分比。


<a id="ci01"></a>
## 11 · Command Intelligence Engine：语法、版本与确定性建议

### 11.1 三个内核，而不是把所有逻辑写进 TUI

```text
                    Shared application layer
                 /              |               \
     Command Intelligence    Execution Kernel    Presentation Engine
     parse / suggest / help  policy / send /     table / JSON / tree /
     guide / explain         session / outcome  escaped bytes
```

Command Intelligence 默认是纯本地计算库。它输出 `Suggestions`、`SignatureHelp`、`Diagnostics`、`Documentation` 和 `ProposedAction`，不持有可任意执行 Redis command 的 client。远端发现必须通过 application/kernel，带明确 origin、预算和目标。

CLI、TUI、离线学习、shell completion 和未来 Studio 共用它。需要嵌入时链接 Rust library，不为了模仿 IDE 必须启动 LSP 服务；可选协议适配不改变普通使用的零常驻要求。

### 11.2 知识来源与可信等级

| 来源 | 主要用途 | 限制 |
|---|---|---|
| 随版本发布的官方命令目录快照 | 命令名、语法、参数、版本历史 | 记录 upstream revision，不假设是目标全部能力 |
| 经授权获取的 COMMAND / COMMAND DOCS | 补充目标暴露的能力 | 不在每次按键请求；不等于 ACL 授权 |
| 本仓评审的 semantic overlay | 条件互斥、key/field 角色、单位、危险性 | 必须绑定适用版本、来源和测试 |
| 本机已观察结果与允许的历史 | key、field、group、index 等名字 | 作用域隔离、时效和隐私限制 |
| 用户明确配置的项目 schema | 业务字段、enum、模板 | 标注为配置，不冒充数据库事实 |
| 离线中英术语/任务索引 | 按用途找命令 | 不承诺理解任意自然语言 |
| 可选 AI | 解释或提议复杂计划 | 不属于补全必需组件，无执行权限 |

`COMMAND DOCS` 提供命令描述、版本、参数等文档元数据；参数文档包含 `oneof`、`block`、重复参数和 key specification 引用等结构。[R20][R40] 这些能够支撑语法驱动帮助，但不是完整业务语义、实时权限或对所有未完成输入都准确的解析器。

**来源优先级（v2.1 明确）：** 对同一命令的同一属性，只有下列方向的覆盖是允许的：

| 属性 | 权威来源 | 远端 `COMMAND` / `COMMAND DOCS` / module metadata 的作用 |
|---|---|---|
| `effects`（读/写/消费/阻塞/管理/未知）、危险性、审批等级、key extraction 规则 | **本仓签名 catalog + 评审 overlay** | 只能把 `unknown` 变成 `unknown (server reports flags: …)`；**不能**把本地 `writes_data` 改成 read、不能降低危险等级、不能放宽策略 |
| availability（目标是否暴露该命令/子命令/参数） | 远端 introspection | 可以把本地 `documented` 标为 `observed-supported / observed-unsupported` |
| 文档文本、示例、参数说明 | 本地 catalog | 远端文本可作为补充展示，但进入 SafeText，且标 `from server` |
| 本地 catalog 中**不存在**的命令 | 无 | 允许通用执行（按策略），effects 为 `unknown`，生产/agent 按 §23.1 的未分类命令处理 |

签名的本地 catalog 是 policy 的唯一输入；被污染、过时或恶意的远端 metadata 最多导致「显示不准确」，不能导致「策略放宽」。

### 11.3 Metadata 编译与发布管线

```text
Pinned upstream snapshot + selected server introspection fixtures
        -> schema/version adapters
        -> normalized CommandSpec
        -> reviewed semantic overlays
        -> capability fixtures + parser/property tests
        -> compact immutable catalog + localized search index
        -> signed/checksummed release asset
```

不能运行时抓一页 HTML、用正则提取语法、马上将其当生产策略。schema adapter 明确支持上游 `arguments`、历史结构和 module 元数据差异，未知属性保留，无法安全理解的节点标 `unsupported grammar node`。

人工 overlay 的条目必须包含 owner、来源、适用 server family/version、测试 ID 和 review date；与新上游冲突时阻断发布，不静默覆盖。文档翻译不能修改命令 token、单位、复杂度或危险性。

### 11.4 CommandSpec 的最低结构

```text
CommandSpec
  id / canonical_name / subcommand_path
  upstream_revision / docs_reference / schema_version
  server_family / introduced / deprecated / replacements
  grammar: sequence | choice | optional | repeat | token | argument
  argument_roles: key | field | member | value | cursor | group | ...
  key_specs / routing_constraints
  constraints / unit_rules / state_requirements
  effects: reads_data / writes_data / consumes / blocks / admin / unknown
  permissions: unknown by default, separate from effects
  response_schemas / examples / locale_summaries / search_synonyms
```

Effects 与 ACL 分类分开；一个名字里带 READ 或 GET 的命令也可能改变状态。例如 `XREADGROUP` 涉及消费组状态，不能因为字符串前缀是 XREAD 就作为无副作用预览命令。[R51]

### 11.5 编辑中与发送前使用不同解析目标

**宽容的增量分析器**接受不完整引号、空参数、光标位于中间、尚未输入完的子命令和选项，用于提示“还缺什么”。

**权威请求 tokenizer**只在明确提交时输出原始 argv bytes，按受测 Redis CLI quoting 规则工作。两者共享字节跨度与词法规则，但不能为了得到一棵漂亮 AST 而改变实际参数。[R01]

**两个解析器的等价性不变量（v2.1 明确）：** 对任意完整（无未闭合引号）的输入缓冲区 `B`，宽容分析器产出的 token span 序列 `spans(B)` 必须与权威 tokenizer 产出的 argv 边界 `argv(B)` 一一对应：`decode(B[span_i]) == argv_i` 对所有 `i` 成立。对含未闭合引号的 `B`，宽容分析器必须标出「未闭合区间」且该区间之前的 span 与 `argv(B + closing_quote)` 一致。该不变量以 property test 生成随机字节串（含转义、空参数、Unicode、NUL 转义）验证，是 ASSIST-081 的内容；补全插入的 `TextEdit` 只能替换分析器给出的 span，因此该不变量成立即保证「显示的候选 == 实际发送的参数」。

quoting 语法以固定 `redis-cli` tag 的 `sdssplitargs` 行为为基准生成 fixture corpus，纳入 `fixtures/catalog/quoting/`；基准变更需通过 compatibility manifest 审核。

未知新命令仍可通过通用请求路径执行，取决于本地安全策略。语法诊断不是最终的服务器兼容裁判；新参数仅在本地旧目录中缺失时显示 `not in local catalog`，不宣称 Redis 一定不支持。

### 11.6 光标上下文，不只是字符串开头匹配

上下文包含：当前 token span、解码后的参数字节、quote mode、光标位置、期望参数集合、前后 token、命令/子命令、会话状态、profile/DB/identity、能力与 policy epoch。

例子：

```text
HGET player:10001 sta▏tus
```

补全替换当前 token 的正确 span，不能得到 `statustatus`。不改其他参数，不移除用户没选的选项。`\x1b` 等控制字符只是文档中的转义写法，原始危险字节不能直接注入显示。

### 11.7 必须支持的语法复杂度

| 语法形态 | 例子 | 分析要求 |
|---|---|---|
| 子命令 | `CLIENT LIST`、`XINFO GROUPS` | 第二个 token 是子命令，不是 key |
| 交替重复项 | `HSET key field value ...` | 字段位提示 field；value 位不枚举敏感值 |
| 分数/成员重复项 | `ZADD key score member ...` | 不能把 score 和 member 角色交换 |
| 互斥项 | `SET` 条件与 expiry；`ZADD` 条件 | 按版本过滤；解释冲突，不暗改输入 [R42][R43] |
| 成组选择 | `XADD` trimming 语法 | 嵌套 choice/block，不把所有 keyword 同时推荐 |
| 参数决定后续数量 | `EVAL ... numkeys ...` | 正确区分 keys 与 ARGV |
| 成对列表 | `XREAD ... STREAMS key... id...` | 处理 key/ID 数量与语义，不猜数值默认值 |
| 模式与文字同形 | key 名为 `NX` / `GET` | 由语法位置决定，不把数据变关键字 |
| 排序/范围模式 | `ZRANGE`、`BYSCORE`、`BYLEX`、`REV` | 参数含义跟随已选择模式 [R05] |
| 任意二进制参数 | 引号、空值、NUL、无效 UTF-8 | 编辑器保存 UTF-8 转义形式；提交时解码为精确 bytes（§3.2）；显示与插入分离 |
| 扩展数据类型 | JSON/Search/TimeSeries/其他模块 | 加载受测 adapter；未知则 generic |

Key specs 可以不完整，也可能因选项改变访问标志。[R41] 对完整请求必要时走获准的专用提取策略；**不为每次键入调用 COMMAND GETKEYS 来试解析半条命令**，也不把不确定的 key 提取结果用于批准危险写入。

### 11.8 约束与诊断等级

`Incomplete` 是正常编辑状态，不用红色吓用户。`Advisory` 表示可能冲突/目录未知。`KnownInvalid` 只有受测规则明确成立时才使用。`Denied` 来自安全策略，不是语法分析器。

已选择 `EX` 就在剩余 expiry 候选中移除互斥项；用户手写的冲突内容保留并说明原因。参数单位跟着命令/选项，不将所有 timeout 都解释为毫秒。输入值为零是否允许由该参数规则决定，而不是统一“必须正数”。

### 11.9 能力与权限状态必须分开显示

| 维度 | 状态例子 |
|---|---|
| Catalog | documented / unknown / deprecated |
| Server support | observed-supported / observed-unsupported / unknown |
| Local policy | allowed / approval-required / denied |
| Server ACL | unknown / observed-denied / checked-at-time |
| Session | ready / queued / blocked / disconnected / reply-suppressed |

历史成功不证明今天仍然允许。`COMMAND` 列出某命令不证明当前用户能运行它。`ACL DRYRUN` 可以检查给定用户运行命令的权限，但本身是管理命令；不将它作为每次候选生成的隐式前提，也不把结果当成未来执行的永久保证。[R45]

服务器类型、版本或 ACL 不可读取时仍能提供离线帮助，并标不确定。未知能力不应被统一当“不支持”；本地 policy deny 的条目可在帮助里解释，但不能通过按 Tab 获取绕过策略的执行入口。

### 11.10 排序：可解释、稳定、隐私友好

优先级：当前语法合法候选 -> 精确匹配/前缀 -> 当前上下文相关 -> 用户明确收藏 -> 当前安全作用域内的使用频率 -> 模糊匹配。危险操作不因使用频率高就被自动提升成空输入默认建议。

命令名允许 case-insensitive 模糊匹配。key/field 默认按字节精确前缀；模糊找 key 需用户打开，并标 `fuzzy name`，不能自动把相近名字认作同一个 key。

选中项有稳定 CandidateId。异步新结果出现时，不让用户正准备按 Enter 的行突然换成另一条；新候选可以追加或留到下一次输入重排。来源文本、业务值和服务器文档不能注入“请自动执行”之类控制指令。

### 11.11 核心接口草图

以下是契约草图，实施时需补全类型与 trait，不冒充现成可编译代码。

```rust
struct CompletionRequest {
    buffer_revision: u64,
    editor: EditorSnapshot,
    cursor: ByteOffset,
    target: TargetEpoch,
    identity: IdentityEpoch,
    policy: PolicyEpoch,
    capabilities: CapabilityEpoch,
    reason: Trigger, // typing / explicit / history / guide
}

struct CompletionCandidate {
    id: CandidateId,
    kind: CandidateKind,
    display: SafeText,
    detail: SafeText,
    edit: TextEdit, // replacement span + exact bytes-aware encoding
    origin: CandidateOrigin,
    observed_at: Option<Timestamp>,
    effect: EffectSummary,
    availability: Availability,
    required_epochs: EpochSet,
}

struct AssistanceSnapshot {
    revision: u64,
    candidates: BoundedCandidates,
    signature: Option<SignatureHelp>,
    diagnostics: Vec<LocalDiagnostic>,
    pending: bool,
}
```

接受时重新核对 buffer revision、目标/身份/policy epoch 和替换 span；任何过期候选均不应用。接受候选是单次可撤销的本地 edit transaction，不带 executable callback。


<a id="ci02"></a>
## 12 · 候选数据与资源预算：智能，但不偷偷扫描整个 Redis

### 12.1 四种数据层

| 层 | 内容 | 默认联网 | 默认持久化 |
|---|---|---|---|
| L0 Catalog | 命令、参数、帮助、中文/英文用途 | 否 | 随 binary/只读资源发布 |
| L1 Session observations | 用户已经执行的结果中提取的名字 | 否，复用已有结果 | 否 |
| L2 Approved history / schema | 允许的历史、模板、项目提示 | 否 | 按隐私配置 |
| L3 Explicit discovery | 当前目标的有限 key/field 等发现 | 是，需明确允许 | 默认否 |

普通打字只读 L0-L2，不建立独立 Redis 数据索引，不安装 scheduler，不把所有 key 存进一个巨大的本机库。退出后所有会话内建议任务结束。

### 12.2 名称来源细分

| 当前输入位置 | 优先候选来源 | 获准的远端补充方式 |
|---|---|---|
| 命令/子命令/选项 | 静态 catalog + 已知 capability | 手动更新 metadata，不逐字请求 |
| Profile | 本机非秘密名称与标签 | 不联网 |
| key | 当前目标已观察名字/明确保存书签 | 有范围的 SCAN 任务 |
| Hash field | 当前 key 的 retained response 索引 | 适用版本 HSCAN NOVALUES |
| Set/ZSet member | 已观察的成员名 | 明确允许 SSCAN/ZSCAN；可能暴露业务内容 |
| Stream group/consumer | 已观察的 XINFO 结果 | 明确读取指定 stream 元数据 |
| Stream entry ID | 已观察的 entry ID、语义说明 | 用户启动的范围读取，不隐式消费 |
| JSON path | 用户明确加载的 JSON 树/项目 schema | 不因每个点号重新 JSON.GET 整个文档 |
| Search index/field | 已批准加载的索引定义 | adapter 专用元数据计划 |
| value / password / token | 默认无数据补全 | 不自动读取真实业务值填入 |

`HSCAN NOVALUES` 可只返回字段名。[R44] 不支持这一选项的目标，自动远端 field completion 默认停用；只有用户明确允许接收值后，才考虑有预算的 HSCAN 并尽早丢弃 values。不能声称“只读字段名”，实际上已经抓取整份敏感 Hash。

**HKEYS/HGETALL 不是默认 field completion 的便捷捷径。** key 类型也不能为每个候选发 TYPE；只标注已经观察过的类型、观察时间和范围。

### 12.3 明确的发现入口

```text
:complete status
:complete keys --prefix 'player:'
:complete fields --key player:10001
:complete clear
```

在编辑框按 F6 或从 F2 palette 选择 Discover，可用当前输入上下文生成同样的计划并保留原草稿。它是独立 action，不伪装成一个普通 completion candidate。

计划展示 profile/DB/identity、命令族、可能读取的数据、最大请求数/持续时间、范围和缓存期限。生产默认要求本次明确请求；允许用户对 dev 设“本会话自动有限发现”，但这不是全局默认，也不能降低本地/服务器权限。

### 12.4 匹配与 Redis SCAN 的真实边界

发现请求有**两种互不混用的匹配模式（v2.1 明确）**：

| 模式 | 入口 | 对用户输入施加的变换 | 发送的 MATCH |
|---|---|---|---|
| 字面前缀 | 自动候选、F6、`:complete keys --prefix <bytes>` | 对 `*`、`?`、`[`、`]`、`\` 逐字节加 `\` 转义，再追加 `*` | 转义后模式；计划预览中同时显示原字节与转义结果 |
| 原始 glob | `:complete keys --match <pattern>`、`:bulk … --match`、原始 `SCAN … MATCH` | **不做任何变换** | 用户字节原样 |

用户输入 `player:[` 在字面前缀模式下发送 `player:\[*`，在 glob 模式下按用户字节发送并由 Redis 判定语法；两种模式的 UI 标签不同（`prefix` / `glob`），不存在「无声变成另一个匹配条件」的路径。

SCAN 的 COUNT 是提示，不保证响应数，也不提供严格分页或数据库快照。[R08] 前缀过滤不意味着 Redis 有该前缀的二级索引；稀疏匹配可能需要很多次扫描。自动补全不能无限扫描直到找到候选。

到预算停止时显示：`No matches in the observed portion; discovery incomplete`，不能显示 `key does not exist`。服务器发回异常大的单次结果时停止/关闭专用发现连接并记录部分状态；客户端预算不能保证服务器之前没有承担计算成本。

### 12.5 初始资源参数：可配置、需实测

| 项目 | 初始设计值 | 边界 |
|---|---|---|
| 自动显示 debounce | 75 ms | 手动 Tab 不必等这段时间 |
| 默认可见候选 | 6 条 | 窄屏可降为 3 条，能显式查看更多 |
| 一次请求候选计算上限 | 128 条 | 排序后取视口，不物化全库名字 |
| 普通 assistance **堆**增量工作集 | **12 MiB 上限**（下列子项合计 10 MiB + 2 MiB 余量） | 不含大结果 store、不含只读 mmap catalog；按「开/关提示」的 RSS 差测量 |
| 只读 catalog + 本地化搜索索引 | 只读 mmap，**单独报告**（目标 ≤ 6 MiB 文件） | 由 OS page cache 管理，不计入堆增量；多进程可共享页 |
| 已观察 key 名缓存 | 5,000 个或 2 MiB，先到者为准 | 超长名字受单项限制，仍可手输 |
| field/member 名缓存 | 2 MiB 总预算 | 分 target/key LRU，不按每 key 无限叠加 |
| metadata/docs 解析缓存 | 2 MiB 热工作集 | 完整离线文档按需解压，重型模块不全加载 |
| scratch 与候选/索引 | 2 MiB | 包括索引附加内存，不只计算字符串 bytes |
| 增量分析器状态 + 待返回候选 | 1 MiB | 含 revision 队列；迟到结果丢弃后立即释放 |
| 发现响应处理（见下） | 1 MiB | 计入 10 MiB 子项合计 |
| 大输入智能解析 | 超过 64 KiB 转有限窗口分析 | 命令本身不因此截断 |
| remote discovery 并发 | 每进程最多 1 个自动 + 1 个显式任务 | 不抢占普通请求与事务通道 |
| **自动**发现（dev 会话内授权时） | 每 profile 每进程 ≤ 2 请求/秒；单次 ≤ 2 次 SCAN 类请求；≤ 1 秒 | 保守；找不到即返回部分结果并标 `discovery incomplete` |
| **显式**发现（F6 / `:complete` / palette Discover） | 默认单次 ≤ 50 次 SCAN 类请求、`COUNT 1000`、≤ 10 秒、≤ 20 请求/秒；每项可在 profile 中调高 | 显示进度（已扫 cursor 次数、已匹配数、剩余预算）；可随时取消；生产默认要求每次确认 |
| 发现响应处理预算 | 1 MiB 起点 | 可能超额到达 socket；不声称严格网络硬上限 |

自动发现的预算刻意小，因为它在用户没有明确请求时发生；显式发现才是「在大库里按前缀找 key」的正式路径，其默认预算必须足以在数百万 key 的库上返回有意义的结果，否则该功能等于不存在。**所有速率限制都是进程级**（§24.7）；无 daemon 架构不做跨进程聚合，文档与 `:complete status` 都要如实说明多个 `prc` 进程会各自计费。
| 否定/权限错误冷却 | 60 秒或身份 epoch 改变 | 防止每个按键重复 NOPERM |

这些值是初始验收目标，不是测量结果。总体内存仍遵循整进程预算；catalog、索引、历史 page cache、result store、TUI layout 和 allocator overhead 必须一起计量，不能各模块都宣称“小”但总和失控。

### 12.6 硬隔离键：不能把 dev 的建议带到 prod

```text
ObservationScope =
  profile_uuid
  + service_identity/trust_binding
  + db_index
  + auth_identity_epoch
  + policy_epoch
  + topology_epoch where relevant
```

仅仅使用 `host:port` 或 profile 名作缓存键不够。改名保留 UUID；改 endpoint、身份、权限或服务角色，触发相应缓存失效。Cluster 中 DB 只按真实支持处理，不能补全不存在的逻辑 DB。[R10]

从用户明确发出的 AUTH/HELLO/SELECT/RESET 获得状态改变后，重新绑定命令上下文；无法确认当前认证状态时暂停远端发现。已有历史不会被重新标成新身份。

### 12.7 缓存新鲜度与推断

已执行的成功写入可使相关名称缓存新增或失效，但记录为 observed/inferred，不能推导其他 key 的状态。取消、错误或未知结果不能被拿来更新成“写入成功”。

对象过期、其他客户端删除/重建、failover、重分片都可能使名字过时。显示缓存年龄，允许手动清除；TTL 是观察时效，不是 key 实际 TTL。长时间不活跃的作用域回收，而不是永远常驻每个 profile 的索引。

### 12.8 异步控制：旧候选不能覆盖新输入

每次输入递增 buffer revision；每次目标/身份/policy 变更递增对应 epoch。worker 结果回到 UI 时检查全部标签，不匹配即丢弃。取消只意味着不再采用结果，不能假装撤销已发出的 Redis 读取。

UI 不等待网络；本地候选先展示，远端结果只在原输入仍有效时增补。光标移到另一个参数后，不应用上一个参数的候选。批量粘贴期间暂停每字符自动分析，粘贴完成后最多计算一次。

### 12.9 隐私与注入

key、field、group 名本身都可能包含身份信息。profile 可禁用名称缓存、禁止导出、设置掩码；调试日志默认只记录数量/时间/原因，不记录输入正文或候选数据。

候选 label、description、服务器错误和 module 文档全部走 SafeText；禁止 ANSI/OSC/终端超链接注入。候选 display string 与用于插入的原始 bytes 分开，不能把省略号或转义预览插成真实 key。

不为补全访问云 LLM，不上传历史到 telemetry，不枚举系统钥匙串。AI 功能打开也不自动获得每次按键的内容；数据外发仍需要独立策略与明确操作。

### 12.10 多进程也不需要 daemon

多个 `prc` 进程可以各自复用同一份只读 catalog 文件与受控历史数据库，但不要求常驻的中心 suggestion server。动态名字默认留在各自进程内，避免跨会话混淆。

用户明确选择共享已批准的书签/schema 时，使用版本化本机文件和权限检查：写入前取 advisory lock（POSIX `flock` / Windows `LockFileEx`），读取版本号 -> 修改 -> 临时文件 -> fsync -> 原子 rename -> 版本号 +1；版本号不匹配时不覆盖，写入 `conflicts/` 目录并提示用户合并。`history.sqlite` 使用 WAL 模式，多进程并发写由 SQLite 自身的锁保证；只读目录映射可能被 OS page cache 共享，这是实现优化，不应对每台机器承诺特定 RSS 数字。


<a id="ci03"></a>
## 13 · Find、Guide、Learn：从记忆命令转为选择任务

### 13.1 F2 / :find：不知道命令名也能找

```text
dev:3 > :find 查看 hash 全部字段

Task matches · local documentation search · no Redis request

  HGETALL   取回全部 field/value；结果可能很大
  HSCAN     分次遍历 field/value；使用 cursor，不是一致快照
  HKEYS     只取全部字段名；不是分页获取

Select to explain or prepare a command, never to execute it.
```

英文输入 `read hash fields`、中文“读哈希”、混合输入“看 hash value”进入同一知识体系。基础实现是本地词法检索、命令分类、人工审核同义词与任务标签；中文用合适的分词/字符 n-gram，不要求额外模型运行。

含义模糊时展示多条意图和差别，不把“清掉 cache”直接翻译成 FLUSHDB。对无法匹配的描述诚实显示没有可靠结果，并保留按类型/读写/用途浏览的入口。

### 13.2 Palette 统一入口，但不混淆操作

F2 的全局入口区分四类：Find Redis commands、Local tools、Saved recipes、Connections。每条明确标 `documentation`、`prepare`、`local action` 或 `connect`。

输入命令建议的 Enter 只填充；选择连接会进入明确的连接 action 流程；选择 recipe 会显示计划。不能让同一个看起来普通的命令名有时填入、有时直接执行。

默认保留当前草稿，取消 palette 回到原光标位置。连接切换前检查事务、未发送编辑与未知结果，不让发现性入口绕过既有状态机。

### 13.3 离线 Command Card

F1 或 `:help HGETALL` 展开卡片，内容来自固定 catalog 与当前已知能力，不为打开帮助请求远端数据。

| 区块 | 内容 |
|---|---|
| 名称与用途 | 一句话说明；保留 canonical command |
| 语法 | 当前版本语法；光标所在参数突出 |
| 参数 | required/optional、位置、重复、单位与约束 |
| 效果 | 读取/写入/消费/阻塞/管理；unknown 明示 |
| 返回 | 值的含义、nil/0/empty 的区别 |
| 例子 | 本地虚拟命名空间、可编辑模板、不自动执行 |
| 风险 | 大响应、过期变化、消费状态、Cluster 限制 |
| 相关命令 | 例如 HGET / HMGET / HGETALL / HSCAN 的选择差异 |
| 依据 | 文档 revision、target capability 与未确认项 |

`HSET -> 0` 卡片解释新增字段数，不暗示写失败。[R04] `TTL` 卡片区分无过期与不存在。[R34] 这些说明由已测试规则生成，不需要 LLM 才能说清楚。

### 13.4 :guide：一步步拼出真实命令

```text
dev:3 > :guide SET

Command builder · local only

Key               cache:demo
Value             hello world
Condition         Only if absent (NX)
Expiry            Relative seconds (EX)
Seconds           300
Return old value  No

Prepared Redis command
  SET cache:demo "hello world" EX 300 NX

[Insert into editor]   [Edit choices]   [Cancel]
```

这是命令构建器，不是新的 Redis 数据操作语法。构建器默认不取数据、不推断业务值；字符串 quoting 使用同一权威编码器。生成后显示 argv 预览，用户可修改，再由标准 kernel 执行。

敏感值使用隐藏输入和受保护 buffer，不把它写进历史/展示区。复杂或当前未知的语法可回到原命令编辑器；缺少 guided UI 不等于该命令不能执行。

### 13.5 Snippet 与占位符

占位符是编辑器元数据，不是普通字符串标记。`<key>`、`<field>` 只用于帮助展示；如果用户主动输入字面量 `<key>`，它仍可能是合法 key，不应该全局禁止。

模板有未填写必需槽位时，UI 阻止提交模板并指出缺项；手写 raw 命令不受这种模板规则控制。按 Tab/Shift+Tab 导航遵循焦点表，不突然把一个示例值当默认值填入。

示例分类：read-template、write-template、destructive-template。写/破坏性模板在打开前即标效果；生产环境不预填危险操作目标。所有模板参数经过 bytes-aware argv 注入，不拼 shell 字符串。

### 13.6 单位助手：帮换算，不改 Redis 语法

原命令 `SET k v EX 5m` 不是本产品发明的新语法。输入框可提示“EX 需要秒；5 分钟为 300 秒”，用户选择后才替换为 `300`。

Guide 可以接受人类友好的时间选择并显示生成的 Redis 整数参数；原命令保留官方单位和精度。对于时间戳提示，同时显示当前所选时区和绝对时间；不把本机时钟当服务器时间证明。

### 13.7 错误帮助：解释与修复分离

| 实际错误/状态 | 可以提供的帮助 | 绝不能自动做 |
|---|---|---|
| WRONGTYPE | 解释类型不匹配；提供显式 TYPE 检查 | 自动 DEL 或将 GET 换成 HGETALL 重发 |
| Wrong argument count | 标出可能缺失参数、打开正确语法 | 编造参数补齐后执行 |
| Syntax error | 标出已知冲突选项或 quote 问题 | 改写未确认的 raw bytes |
| NOPERM | 说明被服务器拒绝，展示本地策略状态 | 尝试别的凭证或绕过 ACL |
| Unknown command | 可选命令名纠错、能力未知提示 | 选择最像的写命令重新执行 |
| CROSSSLOT | 显示相关 slot 约束，解释非原子替代方案 | 将原命令拆分再假装同语义 |
| Timeout / result unknown | 解释可能已执行，提供仅重连/检查 | 默认 retry write |
| Conditional write not applied | 解释条件不满足，展示原始回复 | 把 nil/0 全部显示为网络失败 |
| AUTH/TLS failure | 修改已选 profile 的安全配置 | 关闭证书校验/尝试系统所有秘密 |

服务器错误文本本身是不可信数据；错误辅助不能按其中嵌入的链接、路径或“修复命令”自动行动。

### 13.8 执行后建议：只给当前任务相关的下一步

Hash 结果之后可以给一行 `F1 explain · :inspect --field ... · :copy --field ...`，最多少数相关项。提示本地操作与远端操作的区别，例如 `Inspect retained result (local)` 与 `Check current TTL (request)`。

不在每次读数据后推销 AI，不默认建议删 key，不根据一次 observation 断言业务故障。普通用户关闭 post-result hints 后保持关闭，但 F2 仍能找到相同能力。

### 13.9 可选学习模式，不要求连接 Redis

```bash
prc --learn
prc --learn hash
prc --demo
```

`--learn` 使用离线命令卡片、交互练习题和经过测试的 fixture transcripts。只负责解释和校验练习输入，不冒充覆盖所有 Redis 语义的内存模拟服务器。每个屏幕持续显示 `OFFLINE PRACTICE · NO REDIS CONNECTION`。

真正执行练习必须由用户单独选择 disposable sandbox profile 并确认；工具不因为“学习模式”自动安装/启动 Redis、Docker 或后台服务。不会把练习命令误发到当前 prod profile。

### 13.10 项目知识：帮你记业务约定

可以显式保存 schema hint，例如某个 Hash 常见字段、字段说明、合法 enum、时间单位、敏感等级。它是团队约定，不是数据库强 schema；默认只提示，不阻止 raw Redis 输入。

用户明确配置 `status = [ACTIVE, PENDING, FAILED]` 后才补全这些枚举并着业务色；不是抽样一些旧值就断言全部合法值。name/path/value 的保密策略一起保存。项目配置首次信任与变更审核沿用安全章节。

Schema、recipe 和自写帮助可以导出为无秘密文件，供 Studio 复用。不要自动从全部 Redis values 建立业务知识库。

### 13.11 帮助与 AI 的分工

命令名、参数顺序、选项约束、单位、文档、读写分类和常见错误说明应能**完全离线、无模型、无订阅**工作。

AI 是用户明确开启的补充：解释复杂证据、生成候选诊断计划。它不能成为每次按键的后端，不能把未验证命令放进确定性列表，不得自动接纳或批准自己的建议。


<a id="ci04"></a>
## 14 · 终端输入契约：焦点、按键、颜色与无障碍

### 14.1 默认按键必须能预测

| 当前状态 | Tab | Enter | Up / Down | Esc |
|---|---|---|---|---|
| 普通输入、无候选 | 请求候选；无候选不改字 | 提交真实 buffer，经过 tokenizer/policy | 历史或多行移动，遵循编辑模式 | 取消当前辅助 UI，不丢草稿 |
| 自动候选可见、尚未聚焦 | 接纳推荐项，只编辑 | 不接纳建议；仅提交真实 buffer | Down 进入候选；Up 保留历史/编辑语义 | 隐藏菜单，暂不重开 |
| 候选菜单已聚焦 | 接纳选中项，只编辑 | 接纳选中项，只编辑，不发送 | 移动选中项 | 关闭菜单，恢复编辑 |
| Guide/snippet 中且菜单关闭 | 下一个可编辑槽位 | 必填槽位未完成则提示；不发送占位符 | 当前控件语义 | 回退或取消，保留原草稿 |
| 文档/Find palette | 控件导航 | 按明确标识查看或填入，不执行 Redis | 导航 | 返回原输入与光标 |
| TUI 数据区域 | 页面约定导航 | 查看/展开所选数据 | 数据导航 | 返回上一层 |
| TUI 命令编辑框 | 与 CLI 相同 | 与 CLI 相同 | 与 CLI 相同 | 先关菜单，再离开编辑框 |

自动建议不会让每个本来能执行的命令都必须按两次 Enter。只有用户实际进入候选菜单，Enter 才被菜单消费；footer 必须反映当前行为。

接纳后关闭菜单；不因为 completion 自己添加了一个空格，就立刻重新弹菜单拦住下一次 Enter。用户继续输入或主动按 Tab 时再生成下一层建议。一次输入事件只能被一个状态处理，不能冒泡到执行按钮。

**自动弹出的触发定义（v2.1 明确，消除与 ASSIST-001 的表述歧义）：** 上表第一行「普通输入、无候选」指分析器对当前 token **没有**候选的状态。在 Assisted 模式下，每次导致当前 token 变化的键入都会在 debounce 后请求候选；有候选即进入第二行「自动候选可见、尚未聚焦」，**无需 Tab**。Tab 在第一行的含义是「立即请求，不等 debounce」，不是自动弹出的前提。

**历史导航不被菜单吃掉：** 菜单可见但未聚焦时 Down 进入菜单（上表）；因此历史的「下一条」始终由 `Ctrl+N` 保证可达，「上一条」由 `Up` 与 `Ctrl+P` 保证可达。`Ctrl+P/N` 在任何菜单状态下都只作用于历史并先关闭菜单。

**粘贴（v2.1 强制）：** REPL 与 TUI 编辑框启动时启用 bracketed paste（`CSI ? 2004 h`）。收到成对粘贴标记的内容：单行 -> 进入编辑缓冲区不提交；多行 -> 进入 **paste-staging** 视图，逐行显示、允许编辑/删除，用户明确选择「逐条提交」「作为多行单命令」或「取消」，任何情况下不自动执行。若终端不支持 bracketed paste（`TERM=dumb`、部分 Windows 控制台、探测失败）：连续到达的输入若包含换行且到达间隔 < 10 ms，一律视为粘贴并进入 paste-staging；无法判定时宁可多进 staging。粘贴内容超过 §24.3 的输入阈值同样进入 staging。

### 14.2 快捷键与文字替代入口

| 功能 | 默认键 | 可发现的命令/入口 |
|---|---|---|
| 完整命令/参数帮助 | F1 | `:help` |
| 按用途找命令/工具 | F2 | `:find` |
| 分步构建当前命令 | F3 | `:guide` |
| TUI 聚焦命令框 | F4 | 可见的 Command 标签；进入后使用同一 editor |
| 显式发现候选 | F6 | `:complete` / palette Discover action |
| 历史搜索 | Ctrl+R | `:history` |
| 取消输入/当前等待 | Ctrl+C | 状态相关，不能笼统当“撤销服务器操作” |
| 退出 REPL | 空输入时 Ctrl+D | `exit` |

这些是设计默认值，可重映射。`Ctrl+Space` 可能与输入法/终端快捷键冲突，不作为唯一必需入口。Function keys 被系统拦截时仍可用文字入口；不能要求用户先修改系统快捷键才能找帮助。

默认不依赖终端无法统一区分的 Ctrl+Enter、Shift+Enter 或按键松开事件。多行输入以未闭合结构、显式编辑模式和可见 Submit 控件实现；原始 parser 规则独立于视觉换行。

### 14.3 TUI 不偷走命令内容

在数据视图中 `/` 可以搜索；在命令框或 JSON 字符串中 `/` 是输入字符。`e`、`y`、`d`、`:` 也必须按 focus 分派。不能在用户输入包含字母 d 的 value 时触发 Delete。

进入 approval dialog 后输入上下文冻结，异步候选不能更新批准的命令。用户修改目标或参数必须使批准失效。退出 dialog 不会隐式执行原命令。

### 14.4 单一 terminal owner

REPL editor、TUI painter、completion menu、异步通知、pager 与信号处理都通过同一个 terminal coordinator。任意时刻只有一个活跃终端表面持有绘制/输入权。

```text
OS terminal events
  -> Terminal Coordinator
  -> focused editor/menu/view state machine
  -> immutable UI events
  -> pure Assistance Engine or explicit Application Action
  -> frame/delta paint through the same coordinator
```

CLI 模式在主屏幕保留 scrollback；下拉仅在 prompt 附近的受控区域重绘，不把每次候选变化打印进历史。TUI 才使用适用的 alternate screen；退出后恢复原 REPL 草稿与可见来源。

异步消息先排队；安全时通过统一 print-and-redraw 输出。不能让 Tokio worker、Reedline 和 Ratatui 同时直接读 stdin 或写同一个终端。

### 14.5 Reedline 的使用边界

优先使用 Reedline 的 editor、completion span 和 IDE-style menu；当前文档描述了候选位置/替换范围及可轮询的 completion 更新机制。[R47][R48] 它还提供外部重绘/打印相关能力，但是否满足本项目的自动弹出、持续消息与快捷键契约，必须用固定版本做 integration spike。[R49]

禁用无需明确接受就自动插入唯一候选的行为。库的默认按键、quick completion 或 partial completion 不能覆盖本规格。

**v2.1 决定：Reedline 只作为「编辑状态引擎」，不作为终端所有者。** 具体边界：

- 允许复用：`LineBuffer`、`Editor` 的编辑操作（插入/删除/移动/撤销）、`History` trait 与 `Hinter` 接口、`Completer` 的 span 数据结构。
- 不允许复用：`Reedline::read_line` 的事件循环、内建 menu 的绘制与按键处理、external printer、raw mode/alternate screen 管理。这些由 `pr-terminal` 唯一持有，`pr-repl` 把 crossterm 事件翻译成 `EditCommand` 喂给 Reedline 编辑器，并根据 §14.1 状态表自行决定菜单焦点、Enter 归属与异步候选合并。
- 因此 Reedline 的 `Completer` 同步 API 不构成限制：候选由 `pr-intelligence` 的异步 broker（§12.8）产出，`pr-repl` 只在 UI 线程把最新 `AssistanceSnapshot` 交给自绘菜单。

**SPIKE-001（E2 门禁）** 需在 E2 代码冻结前得出结论并附 PTY 测试：(a) `LineBuffer` 能否在不丢字节、不破坏 UTF-8 边界的前提下接受外部 `TextEdit`；(b) 撤销栈能否容纳「接受候选」为单个可撤销事务；(c) 与 `pr-terminal` 的组合是否满足 ASSIST-020~028 与 062~064。任一不满足，**命名回退方案**为：在 `pr-repl` 内实现自有 `LineBuffer`（grapheme-aware、UTF-8 字符串 + 转义字节视图），复用 Reedline 的 `History` 文件格式以保持迁移；Command Intelligence、tokenizer、request/outcome 与所有测试不跟着改。不要通过启动第二个 REPL 进程来绕开设计。

### 14.6 宽度、位置与键入稳定性

菜单按光标附近可用空间向上/向下放置；不足时压缩描述，再降为简单候选行。不要遮住用户正在编辑的 token。布局限制高度、宽度、候选数和重绘工作量。

候选 label 可截断，但详情能查看完整名称；插入永远使用未截断数据。中文、emoji、组合字符使用显示宽度，不用字节长度对齐。长度超限的名字仍可用精确输入或显式 bytes 工具访问。

**显示宽度策略（v2.1 明确）：** 终端与字体对 East Asian Ambiguous 字符、ZWJ emoji 序列、变体选择符的实际宽度不一致，因此「显示宽度正确」定义为**在已声明的策略下一致**，而不是对所有终端都恰好对齐：

- 基线：`unicode-width` 按 grapheme cluster 计算；ZWJ 序列整体按 2 列；控制字符与不可见 format 字符按 0 列并在 pretty 视图转义。
- Ambiguous width：配置项 `display.ambiguous_width = narrow | wide`（默认 `narrow`），并根据 `LANG`/`LC_CTYPE` 为 CJK locale 默认 `wide`。
- 可选探测：`prc --probe-width` 或 `:assist width-probe` 用光标位置报告（`CSI 6 n`）实测若干代表字符的宽度并写入该终端的缓存，仅在 TTY 且用户触发时执行。
- 对齐失败的降级：若探测结果与策略矛盾，表格改用「每格后强制重置列位置」的绘制方式（每行在单元格边界输出绝对光标移动），保证框线对齐，即使内容宽度判定错误。
- 验收 ASSIST-018/067 与 UX-08 在固定策略 + 固定终端仿真下进行；真实终端差异进入人工记录（§32.6）。

输入法组合期间，不依赖每个 terminal 都能报告 composition 事件。能识别则暂停候选确认；不能识别时保持普通已提交文本行为，不绑定会与输入法冲突的唯一快捷键。不能假装 native terminal 对所有 IME 都有 IDE 级事件。

### 14.7 颜色继续沿用 Penguin Table System

结果表头：独立背景/前景、bold、双线底边；每个逻辑 field 单线分隔。JSON 在单元格内用 key/string/number/boolean/null token 上色。

建议菜单：同一表头层级，选中行背景与普通行区分；匹配片段强调但不修改原字符串。READ/WRITE/BLOCKING/DENIED 不仅靠颜色，还显示文字。

Gray ghost text 与真实输入通过样式和提示区别；无颜色模式不把 ghost text直接连到可执行输入中，改为独立 `Suggestion:` 行，避免用户误认为已经输入。

### 14.8 无障碍与 plain 模式

`:assist manual`、`--plain`、`TERM=dumb` 和非 TTY 必须有明确降级。plain 模式不悬浮重绘，提供编号候选与文字帮助；是否显示 ANSI 样式遵循 color contract。

一个靠屏幕阅读器工作的用户应能关闭动态提示，在固定输出里读语法/说明，再回到编辑框。高对比主题、无色、无 Unicode、有限高度和键盘-only 都是验收场景，不是额外收费或以后再补的能力。

### 14.9 不同 shell 的边界

在 `prc @dev` 里面可以控制完整 dropdown；在 zsh/bash/fish 的 one-shot 命令行里，显示样式由 shell 决定，不能承诺一模一样的浮层。

为 shell 提供 profile、client flags、命令名、静态选项的本地 completion。默认不连接 Redis、不解锁 Keychain、不补真实 values；调用内部 completion helper 时限制输入/时间，输出只有候选记录。

生成 shell completion 的安装命令只在用户批准后写文件，不自动改 `.zshrc`，不把用户名密码嵌进脚本。shell quoting 与 REPL quoting 是不同适配层，不能直接复用输出字符串。

### 14.10 零发送不变量

**浏览、选中、预览、解释、切换帮助、换主题、接受建议、填写 Guide，每一步都必须能在断网环境下测试为 0 条 Redis 业务命令。** 只有明确的 metadata/discovery action 或正常提交进入执行内核。

对有意无回复模式另做状态处理；`CLIENT REPLY OFF/SKIP` 可以改变后续回复行为。[R46] 输入帮助不能偷偷往这种连接插命令，也不能用一条 hidden PING 来证明上一个写操作成功。


<a id="ci05"></a>
## 15 · 端到端使用剧本：从不知道命令到读懂结果

本章全是拟实施的产品交互样例，没有连真实 Redis，也没有宣称 `prc` 已发布。

### 15.1 场景 A：我只记得要“读 Hash”

```text
$ prc @dev

dev:3 > :find 看 hash

1  HGET       读一个 field
2  HMGET      读多个指定 field
3  HGETALL    读全部 field/value
4  HSCAN      用 cursor 分次遍历
```

选择 HGETALL 后先显示卡片：适合要看整个 Hash，完整结果可能很大；可选插入到输入框。

```text
dev:3 > HGETALL ▏

Next: <key>
Known here: player:10001
```

Tab 选择 key；正常提交时只发送：

```redis
HGETALL player:10001
```

返回默认彩色表格的黑白结构示意：

```text
player:10001 · @dev · DB 3 · 4 fields returned

╭──────────────┬──────────────────────────────────────────╮
│ FIELD        │ VALUE                                    │
╞══════════════╪══════════════════════════════════════════╡
│ playerId     │ 10001                                    │
├──────────────┼──────────────────────────────────────────┤
│ platformId   │ 70                                       │
├──────────────┼──────────────────────────────────────────┤
│ status       │ ACTIVE                                   │
├──────────────┼──────────────────────────────────────────┤
│ config       │ {                                        │
│              │   "notification": {                      │
│              │     "enabled": true,                     │
│              │     "channels": [                        │
│              │       "FCM",                             │
│              │       "HUAWEI"                           │
│              │     ]                                    │
│              │   }                                      │
│              │ }                                        │
╰──────────────┴──────────────────────────────────────────╯

F1 explain · :inspect --field config · :copy --field config
```

后续只想看 config：输入 `HGET player:10001 c` 时，field 下拉从这次结果建议 config，不额外 HGETALL。或者 `:inspect --field config` 直接查看保留结果，完全不访问 Redis。

### 15.2 场景 B：我写错了 HSET 的成对参数

```text
dev:3 > HSET player:10001 status ACTIVE platformId ▏

HSET <key> <field> <value> [<field> <value> ...]
                                   ^^^^^ 当前缺少 value

field: platformId
The next argument is a value. No business values are auto-fetched.
```

输入 `70` 后，签名恢复完整。工具不从另一个玩家那里读一个 platformId 给你“智能填入”，也不把前一条命令的余额当建议。

### 15.3 场景 C：我不知道 EX、PX、NX 怎么用

```text
dev:3 > :guide SET

Choose expiry:
  No explicit expiry
  EX     relative seconds
  PX     relative milliseconds
  EXAT   absolute Unix seconds
  PXAT   absolute Unix milliseconds
  KEEPTTL retain existing key TTL
```

这里只展示相应版本支持的选项；不把“没有显式 expiry”和“保留现有 TTL”混为一谈。SET 的 TTL 语义按官方说明呈现。[R42]

用户选择 EX、输入 300、选择 NX，预览：

```redis
SET cache:demo "hello world" EX 300 NX
```

Guide 只插入。提交前仍显示当前 profile、读写影响和有效策略。

### 15.4 场景 D：找 consumer group，但不想消费消息

```text
dev:3 > XINFO ▏

  STREAM       查看 stream 信息
  GROUPS       查看 consumer groups
  CONSUMERS    查看指定 group 的 consumers
```

选择 GROUPS 后提示 stream key；不要在“查看 group”途中执行 XREADGROUP。只有用户明确选择消费操作，才展示 group/consumer/位置与状态影响。[R51]

```text
dev:3 > :find 查看 pending

XPENDING    查看消费组 pending 信息
XCLAIM      转移消息归属；会改变状态
XAUTOCLAIM  自动扫描并转移归属；会改变状态
```

候选是用途比较，不把读信息与改变消息归属混在同一个“查看”动作里。

### 15.5 场景 E：拼错命令，但不能自动改成危险命令

```text
dev:3 > HGETAL player:10001

Local hint: HGETAL is not in the selected command catalog.
Did you intend HGETALL?  [Edit input] [Send unchanged if policy permits]
```

建议只修改输入，不能发送错误后自动再发另一个命令。对于服务器新命令，保留通用执行；生产安全策略允许与否仍由 policy 决定。

### 15.6 场景 F：生产环境中找到 FLUSHDB

```text
PROD:0 > FLU▏

COMMAND     EFFECT              LOCAL POLICY
FLUSHDB     destructive write   DENIED
FLUSHALL    destructive write   DENIED

F1 explain policy · no approval can override an explicit deny here
```

能在文档中找到不代表能执行。危险条目不自动接受，也不会因为 typing 匹配就发请求。

### 15.7 场景 G：远端不可达，命令帮助仍可用

```text
@dev disconnected

Input: HGET ▏
HGET <key> <field>
读取一个 Hash 字段。Target support: unknown.

Local documentation available.
Live key/field discovery unavailable. No automatic reconnect for typing.
```

用户可以继续编辑、查帮助、写 template；只有明确提交或选择 Reconnect 才尝试连接。重连不自动执行在断线期间生成的草稿。

### 15.8 场景 H：从 TUI 回 CLI，输入不丢失

TUI 命令框中正在输入 `HGET player:10001 conf`，打开 F1 看帮助再返回，原 buffer 和光标不变。关闭 suggestions 后再 Esc 回数据页；选择返回 REPL 时携带未提交草稿，不执行它。

在另一连接的旧结果上查看字段时，来源一直显示 `@r2 / DB 0 / result #17`；当前输入目标为 `@r3` 时不自动把旧 field 名标作 r3 已存在。用户可明确“以该字段构建当前目标命令”，但这是复制模板，不是数据存在证明。

### 15.9 场景 I：日常 one-shot 不被帮助 UI 拖累

```bash
prc @dev --raw GET platform:70 | jq '.payment.provider'
prc @dev --json HGETALL player:10001 | jq '.status'
```

上述流程不初始化下拉菜单、学习索引、TUI 或模型；stdout 只包含所选格式。缺凭证且不能安全非交互获取时失败，不停在一个看不见的密码框。

### 15.10 场景 J：JSON value 很复杂，不在 command line 手动转义

用户在 Guide 的 value 槽选择本地 JSON editor，编辑完成后看 bytes/格式预览；编辑器生成合法的 Redis 参数编码，而不是拼接 shell。

历史默认只保存命令骨架，不保留 JSON 里的 token 或个人资料。JSON schema 校验只在用户明确选择对应 schema 时应用；普通 Redis string 可以不是 JSON，不能强制改成 JSON 才允许写入。


<a id="ci06"></a>
## 16 · 智能提示专项验收：不能只证明“会弹框”

### 16.1 完成标准

建议系统必须同时通过**正确候选、正确参数、零隐式执行、目标隔离、响应稳定、资源有界、终端可用**七类验收。截图看起来漂亮只是其中一小部分。

下面是可直接拆分到 test suite 的要求，不是已经运行或通过的测试记录。自动化难以覆盖的系统输入法/辅助阅读器场景，需要附具体平台与人工记录，不能伪称全自动通过。

### 16.2 独立验收案例

| ID | 场景 | 必须验证 |
|---|---|---|
| ASSIST-001 | CLI 输入 HG | 自动展示带描述的前缀候选；未按 Tab 也可发现。 |
| ASSIST-002 | TUI 输入同一 buffer | 候选语义/排序/说明与 CLI 共用测试；仅布局不同。 |
| ASSIST-003 | 前缀与相关命令 | HMGET/HSCAN 不伪装成 HG 前缀命中。 |
| ASSIST-004 | 完整命令后空格 | 提示当前 key/field/value 角色，不重新列全部命令。 |
| ASSIST-005 | HGET key field | 只用当前 key 与作用域的 field 候选。 |
| ASSIST-006 | HSET 多组参数 | 每一组 field/value 交替准确，不在 value 位自动抓业务值。 |
| ASSIST-007 | SET NX 与 XX | 按受测 grammar 给冲突说明，不静默移除任一项。 |
| ASSIST-008 | SET EX/PX 单位 | 解释秒/毫秒；示例不是自动插入的默认值。 |
| ASSIST-009 | SET 带新版本选项 | 依据 capability/目录标记，不硬编码旧选项全集。 |
| ASSIST-010 | ZADD 条件与 score/member | 选项约束和重复项角色正确。 |
| ASSIST-011 | XADD 嵌套 block | choice/repeat/optional 组合不互相污染。 |
| ASSIST-012 | XREAD STREAMS 成对列表 | key 和 ID 数量/角色可解释，歧义不乱猜。 |
| ASSIST-013 | EVAL numkeys | key 与 ARGV 范围正确；不运行脚本试探。 |
| ASSIST-014 | 子命令 | CLIENT/XINFO/CONFIG 的下一个 token 正确分类。 |
| ASSIST-015 | keyword 同形 key | key 名 NX/GET/COUNT 不被当成其他语法位的选项。 |
| ASSIST-016 | 局部 token 编辑 | `sta▏tus` 接受 status 后不重复后缀（光标在 a 与 t 之间）。 |
| ASSIST-017 | 未闭合引号 | 编辑可继续；不 panic、不无声删除已有字节。 |
| ASSIST-018 | 中文/emoji/组合字符 | span 和可见宽度各用正确单位。 |
| ASSIST-019 | 二进制候选名 | 安全 label 与精确 bytes 插入分离；不插省略号。 |
| ASSIST-020 | 接受候选 | 0 条 Redis 业务命令；仅产生一个可撤销 edit。 |
| ASSIST-021 | 自动菜单未聚焦 Enter | 只提交真实 buffer，不接纳 ghost text。 |
| ASSIST-022 | 菜单聚焦 Enter | 只接纳、不发送，事件不冒泡到执行层。 |
| ASSIST-023 | 接受后 Enter | 菜单不无止境重开；能够正常提交完整命令。 |
| ASSIST-024 | Esc 关闭菜单 | 当前 token 不变化时不立即反复弹回。 |
| ASSIST-025 | 唯一候选 | 没有用户接受操作时不自动插入。 |
| ASSIST-026 | 异步旧 buffer 结果 | 迟到 revision 被丢弃，不覆盖新 token。 |
| ASSIST-027 | 跨光标位置 | key 候选不插入随后编辑的 field/value 位。 |
| ASSIST-028 | 动态候选重排 | 已聚焦 CandidateId 不因异步追加变成另一条。 |
| ASSIST-029 | 切 dev/prod | 不跨 target 泄漏名称、历史、ghost text。 |
| ASSIST-030 | SELECT 改 DB | 确认后使动态候选作用域失效。 |
| ASSIST-031 | AUTH/HELLO/RESET | 身份状态变化使缓存与待返回候选失效。 |
| ASSIST-032 | profile 改 endpoint | 旧凭证/旧候选不绑定新地址。 |
| ASSIST-033 | 权限撤销 | permission epoch 更新后拒绝旧候选对应的过期 action。 |
| ASSIST-034 | COMMAND DOCS 被拒绝 | 保留离线帮助，标未知；不重复权限探测。 |
| ASSIST-035 | COMMAND 可见但 ACL 未测 | 不能将 catalog 可见标成已授权。 |
| ASSIST-036 | ACL DRYRUN | 不在按键路径执行；显式 action 受管理权限约束。 |
| ASSIST-037 | 默认打字网络计数 | 0 个由打字触发的额外 Redis discovery 请求。 |
| ASSIST-038 | 显式 key discovery | 预览范围/预算；按已批准策略执行并显示不完整状态。 |
| ASSIST-039 | HSCAN NOVALUES | 只在对应 capability 允许时生成，字段解析正确。 |
| ASSIST-040 | 旧目标无 NOVALUES | 不偷偷用带值 HSCAN/HKEYS/HGETALL 替代。 |
| ASSIST-041 | SCAN 空页/非零 cursor | 不宣称不存在或扫描完成。 |
| ASSIST-042 | 稀疏 key 前缀 | 到预算停止，不无限扫描直到找出一个候选。 |
| ASSIST-043 | glob 特殊字符 | 前缀与原始 MATCH 模式不被混为一谈。 |
| ASSIST-044 | 发现超大响应 | 关闭/隔离辅助连接，不导致主 REPL 协议错位。 |
| ASSIST-045 | 发现 NOPERM/超时 | 冷却生效；输入仍立即响应，无请求风暴。 |
| ASSIST-046 | 取消 discovery | 停止使用结果；不声称撤销服务器已执行请求。 |
| ASSIST-047 | 敏感输入 | AUTH/token/value 不进入建议日志、云调用或持久历史。 |
| ASSIST-048 | 候选含终端控制码 | 转义显示；不执行 OSC/CSI/超链接副作用。 |
| ASSIST-049 | 恶意 docs/module metadata | 限制大小/深度；文本不能成为 executable callback。 |
| ASSIST-050 | F2 中文任务检索 | 命中审核任务集，显示相关命令差别；0 数据请求。 |
| ASSIST-051 | 任务含义模糊 | 清 cache 不直接生成/执行 FLUSHDB。 |
| ASSIST-052 | Guide 生成命令 | 精确 argv/quoting；插入而非执行。 |
| ASSIST-053 | Guide 未填槽位 | 阻止模板提交；不把占位符发给 Redis。 |
| ASSIST-054 | 字面 <key> | 用户手写的合法字面值不被模板引擎全局禁用。 |
| ASSIST-055 | SET value 包含空格/引号 | Guide/REPL 编码后 bytes 可往返。 |
| ASSIST-056 | 5m 单位帮助 | 仅显式接受后替换为秒数，不偷偷扩展 Redis 原语法。 |
| ASSIST-057 | WRONGTYPE 帮助 | 不自动重写、删除或发送替代命令。 |
| ASSIST-058 | unknown-after-send 帮助 | 不将 retry write 排成自动默认操作。 |
| ASSIST-059 | XREADGROUP 说明 | 标明消费状态影响，不当只读元数据查询。 |
| ASSIST-060 | MULTI 中建议 | 不在事务会话插入 hidden metadata/discovery。 |
| ASSIST-061 | CLIENT REPLY OFF/SKIP | 跟踪无回复状态；不 hidden PING 也不错配下一条响应。 |
| ASSIST-062 | 持续 Pub/Sub 与菜单 | 消息通过统一 painter，光标/输入/选择不损坏。 |
| ASSIST-063 | TUI 中输入 e/d/y/slash | 只编辑文本，不触发数据页快捷操作。 |
| ASSIST-064 | F1/F2 返回草稿 | buffer、cursor、目标与来源保持，未意外发送。 |
| ASSIST-065 | 生产 deny | 文档可解释；候选/Guide/AI 不获得 bypass。 |
| ASSIST-066 | NO_COLOR/plain/TERM=dumb | 可读候选与独立 ghost 提示，无失真 ANSI。 |
| ASSIST-067 | resize/窄窗口 | 菜单重排不盖住光标，输入 bytes 不变化。 |
| ASSIST-068 | IME 与系统占用快捷键 | 有可用文字入口，不依赖独占 Ctrl+Space。 |
| ASSIST-069 | shell completion | 默认不联网/解锁 Keychain，不执行导入配置中的脚本。 |
| ASSIST-070 | 学习模式 | 离线 fixture 明示；不隐式连接 prod 或启动 Redis。 |
| ASSIST-071 | 8h 高速输入/开关菜单 | 缓存/worker/线程有界，无随键击数增长。 |
| ASSIST-072 | 巨大粘贴 | 有限窗口分析，避免每字全量解析；业务输入不静默截断。 |
| ASSIST-073 | catalog 更新冲突 | overlay 与 schema 变更有 review/test gate，不自动信任。 |
| ASSIST-074 | 不支持的合法命令 | 通用请求仍由策略决定；旧提示器不能冒充最终语法裁判。 |
| ASSIST-075 | 隐私清理 | 清除缓存/历史后无可见旧候选；不宣称消除所有 OS 副本。 |
| ASSIST-076 | 无操作闲置 | 不持续重绘或轮询 Redis；无不必要 busy loop。 |
| ASSIST-077 | 退出 | completion workers、tunnel 与辅助 sessions 清理，无新 daemon。 |
| ASSIST-078 | 不完整服务器元数据 | 支持 unknown 状态，安全 fallback，不按缺失字段编造权限。 |
| ASSIST-079 | 跨实例名字复制 | 明确为模板复制，不冒充当前目标已观察数据。 |
| ASSIST-080 | 建议返回准备执行时 | 最终 kernel 重新验证 target/policy/approval，不信任 UI 的旧判定。 |
| ASSIST-081 | 两解析器 span 等价 | 随机字节缓冲区下宽容分析器 span 与权威 tokenizer argv 一一对应（§11.5）。 |
| ASSIST-082 | bracketed paste 多行 | 进入 paste-staging；无 bracketed paste 的终端按时序启发式同样进入；0 条自动执行。 |
| ASSIST-083 | 转义二进制候选 | 含无效 UTF-8 的观察名以转义形式插入，提交后 argv bytes 与原观察一致。 |
| ASSIST-084 | 字面前缀 vs glob | `player:[` 在 prefix 模式发送转义模式，在 glob 模式原样发送；UI 标签正确。 |
| ASSIST-085 | 显式发现预算 | 50 次 SCAN / 10 s 默认预算在百万 key fixture 上返回非空前缀结果并报告完成度。 |
| ASSIST-086 | 远端 metadata 覆盖 | 服务器声称某写命令为 readonly 时，本地 effects 与策略不变，UI 标 `server reports`。 |
| ASSIST-087 | Ctrl+P/N 历史 | 菜单可见时 Ctrl+N 关闭菜单并进入历史下一条；Down 进入菜单。 |
| ASSIST-088 | topology epoch | Cluster 拓扑变化后旧节点观察名不再作为候选，`:complete status` 显示失效原因。 |

### 16.3 Golden fixture 结构

```yaml
id: assist-hget-field-scope
catalog_fixture: pinned-supported-command-catalog
input:
  buffer: "HGET player:10001 no"
  cursor_utf8_bytes: 20
  target: dev-db3-identityA-epoch7
  buffer_revision: 42
observations:
  - source: result-17
    target: dev-db3-identityA-epoch7
    key: "player:10001"
    fields: [notificationEnabled, notificationConfig]
  - source: result-99
    target: prod-db0-identityB-epoch2
    key: "player:10001"
    fields: [notForDev]
expected:
  candidates: [notificationEnabled, notificationConfig]
  exclude: [notForDev]
  active_parameter: field
  redis_requests: 0
  accepted_candidate_sends_command: false
status: NOT_RUN_IN_THIS_PLAN
```

实施时由测试 helper 自动计算 UTF-8 cursor offset，避免人工示例和输入长度漂移；此例的 buffer 为 20 个 ASCII bytes。

### 16.4 性能与交互门禁

| 指标 | 初始目标 | 测量要求 |
|---|---|---|
| 普通输入到本地候选 | p95 <= 25 ms 计算时间 | 单独报告 75 ms UI debounce，不混成网络耗时 |
| 用户字符立即可见 | p95 <= 16 ms 的本地处理目标 | 与 terminal 实际绘制分别测量，慢网络不能阻塞 |
| 完整本地建议可见 | 常见场景 p95 <= 150 ms | 包含 debounce、布局；不是已实现承诺 |
| 用途搜索 · canonical 集 | top-3 recall >= 95% | 人工标注的规范表述集（每命令 ≥ 3 条中/英），**同时用于构建同义词表**，因此只证明词表完整 |
| 用途搜索 · held-out 改写集 | top-3 recall >= 80%，top-5 >= 90% | 由不参与词表构建的人独立撰写的改写/口语表述；**任何一条进入词表即从该集移除**；两套集合随仓库公开 |
| 确定性 grammar fixture | 100% 必须通过 | 不允许以平均准确率掩盖危险误判 |
| 接受建议误发送 | 0 次 | 状态机/property/PTY 测试 |
| 跨目标建议泄漏 | 0 次 | 多 profile/identity/DB 并发 fixture |
| 默认按键额外 Redis 请求 | 0 次 | transport spy + 真服务审计对照 |
| 普通 assistance 堆增量 RAM | ≤ 12 MiB 上限（§12.5） | 比较同进程开关提示的 RSS 增量；包含索引/缓存/分析器状态；只读 mmap catalog 单独报告 |

这些数字是工程验收预算。发现硬件或 Unicode 特殊情况不达标，要记录结果、定位并给出解释，不伪造 benchmark，也不通过删测试样本降低标准。

### 16.5 测试资产与责任

仓库新增 `fixtures/assistance/`、`fixtures/catalog/`、`tests/editor-state/`、`tests/assistance-network/`、`benches/assistance/`；与完整测试章节共享 runner、版本矩阵与发布报告。

每一个 catalog change 都跑 command/option fixture、错误例子、locale phrase search 与权限状态 tests。新增命令 adapter 必须同时说明支持的 suggestion、signature、guide、renderer 与执行模式，不能只有帮助文本就标成完整支持。


<a id="s11"></a>
## 17 · TUI：需要时展开，而不是强迫用户迁移习惯

### 17.1 主要页面

| 页面 | 完整功能 |
|---|---|
| Connections | 分组、过滤、收藏、修改、环境状态；默认不联网探活 |
| Browser | cursor-aware key discovery、类型过滤、已发现 key 搜索 |
| Inspector | Table / JSON / Tree / Hex、局部搜索、分页、原始内容 |
| Command editor | 自动下拉、参数提示、F1/F2/Guide；与 CLI 共用 Command Intelligence |
| Find / Learn | 以用途找命令、解释返回值、版本/权限状态、离线示例 |
| Editor | 修改、diff、TTL 策略、并发校验、计划提交 |
| Compare | 两个明确来源的 key/字段/JSON 差异 |
| Streams | entry、group、consumer、pending；写操作独立入口 |
| Diagnostics | 快照、采样、已知指标、趋势和证据 |
| Tasks | 当前进程内任务、取消、预算、失败范围与结果 |

### 17.2 一致的操作层级

在**数据页焦点**中，`Enter` 查看/展开，`/` 搜索当前数据，`Esc` 返回上一层，`e` 明确编辑，`y` 复制已选值，`?` 查看快捷键。命令输入框与候选菜单使用[终端输入契约](#ci04)，不能将数据页快捷键同时应用于用户输入。删除不用单次按键直接执行；必须进入可审阅操作。

`Esc` 从 TUI 返回 REPL 不断开正常会话，也不隐式提交编辑。未提交缓冲区先提示保留或丢弃。输入法组合状态下的快捷键不能抢走文字输入。

### 17.3 搜索的两种含义必须分开

**Filter loaded** 只筛选已加载结果，不访问服务器；**Discover more** 执行允许的 Redis 发现请求。一个 `/` 输入框不能让用户误以为搜到了全库，实际上只过滤当前 20 条。

表头可以 sticky，field separator 保留；多行 JSON 仅为可见区域做 layout。不能用“每个 field 有一条横线”为理由为几十万行创建完整终端 widget 树。

### 17.4 生命周期

所有 view 通过同一个执行内核读写，不自带连接逻辑。视图离开时关闭不再需要的订阅/轮询任务。固定核心会话与可回收辅助会话区分，不能因为打开三个面板就产生三份无约束轮询。

正常退出与可捕获错误尽力恢复 terminal state；SIGKILL 或断电无法保证执行清理代码，因此文档提供 `reset` 等恢复说明，并在下一次启动安全处理本机过期资源。

<a id="s12"></a>
## 18 · 输出、stdin、脚本与退出码

### 18.1 输出模式契约

| 模式 | 用途 | 契约 |
|---|---|---|
| TTY default / `--output pretty` | 人看 | Table、色彩、清晰注解 |
| 非 TTY default / `--raw` | 官方风格脚本 | 对目标 baseline 的 raw 行为做差异测试 |
| `--json` | 官方风格 JSON | 默认 RESP3；`-2` 明确选 RESP2 时不自造 Hash object |
| `--csv` | 单次命令的 CSV | 匹配 baseline，不冒充数据库备份 |
| `--output typed-json` | 类型安全工具集成 | 版本化结构、binary base64、map entries、完整状态 |
| `--output ndjson` | 多响应/持续事件 | 一条结构化事件一行，事件类型明确 |
| `--output resp` | 规范化 RESP3 再编码，供其他 RESP 工具/测试消费 | 类型、顺序、bytes 无损；**不是** wire 抓包，attribute/push 以定义的顺序重排；需要原始 wire 用 `--trace-wire <file>` |
| `--bytes` | 单个 blob 原值 | 无尾部 delimiter；非单 blob 报错 |

官方 CLI 的 `--json` 默认 RESP3，`--raw` 用于 raw 格式，非 TTY 自动 raw；`-e` 用于命令失败的错误退出行为。[R01] Penguin 默认增强行为与兼容模式的差异写入 release contract。

输出 flags 互斥；冲突时报错，不用“最后一个 wins”掩盖意外。`--json` 的协议默认不能覆盖用户显式 `-2` / `-3`。

**push 帧与 one-shot 输出（v2.1 明确）：** RESP3 push（invalidation、Pub/Sub 消息、server-side tracking）可能在普通回复前后到达。one-shot `--json` / `--raw` / `--csv` 的 stdout **只包含命令回复**；push 默认丢弃并在 stderr 计数（`N push frames suppressed`）。`--show-pushes` 把 push 以 NDJSON 事件写到 stderr；`--output ndjson|typed-json` 把 push 作为 `{"kind":"push", …}` 事件按到达顺序写入 stdout。REPL 中 push 通过 terminal coordinator 排队显示。任何模式下 push 都不能占用回复槽位（§19.3）。

### 18.2 stdout / stderr

stdout 只放选定格式的数据。连接提示、警告、进度、日志放 stderr，并且非交互模式默认不画动态进度。JSON 模式不得在 JSON 前输出企鹅 banner。

`--raw`、`--bytes` 与 `--output resp` 三条路径都会向 stdout 写入未经展示转义的服务器字节；当 stdout 是 TTY 时，Penguin 在 stderr 打印一次警告（`raw bytes to terminal; control sequences are not filtered`），并在 `--plain`/`TERM=dumb` 下拒绝 `--bytes`/`--output resp` 直写 TTY（需重定向）。默认 pretty 模式必须安全转义。真正 binary-safe 的脚本不要依赖 raw 文本分隔，使用 typed 或 blob 输出。

### 18.3 JSON 的例子

```bash
prc @dev --raw GET platform:70 | jq '.payment.provider'
prc @dev --json HGETALL player:10001 | jq '.status'
prc @dev --bytes GET binary:sample > sample.bin
prc @dev -x SET binary:sample < sample.bin
```

通用 JSON 模式不自动把 Hash value 字符串里面的 JSON 再解析成 nested object。要语义展开使用另外定义的本地转换；不能让 `--json` 随 renderer 偏好改变 schema。

**JSON projection v1（`--json`，v2.1 明确）：** 与固定 `redis-cli` baseline 的 `--json` 做差异测试；差异处以本表为准并在 compatibility manifest 登记：

| RESP 值 | 投影 |
|---|---|
| bulk / simple string，有效 UTF-8 | JSON string |
| bulk string，无效 UTF-8 | 与 baseline 一致时按 baseline；否则 `{"$bytes":"<base64>"}` 并在 stderr 提示；`--json-strict` 下改为非零退出 |
| integer | JSON number |
| double | 有限值 JSON number（原词法）；`inf`/`-inf`/`nan` -> 字符串 `"inf"` 等并在 stderr 提示 |
| big number | JSON string（不丢精度） |
| boolean / null | JSON true/false/null |
| array / set | JSON array（set 保持到达顺序） |
| map（RESP3） | key 全为有效 UTF-8 string 时 -> JSON object，重复 key 保留为数组 `[[k,v],…]` 并在 stderr 提示；否则 -> entries 数组 |
| RESP2 偶数数组 | **不**自动 map 化（与 `-2` 语义一致） |
| verbatim string | 字符串内容；格式前缀丢弃 |
| attribute | 丢弃，stderr 计数 |
| error 回复 | `{"error":"<text>"}` 且退出码 4 |
| 多回复（`--pipe`、事务） | 每回复一行 NDJSON；单回复才是单个 JSON 文档 |

### 18.4 typed output v1 示例

```json
{
  "schema": "penguin.redis.result.v1",
  "result_id": "r17",
  "source": { "profile": "dev", "db": 3 },
  "outcome": "confirmed",
  "complete": true,
  "value": {
    "kind": "map",
    "entries": [
      [
        { "kind": "blob", "encoding": "base64", "data": "c3RhdHVz" },
        { "kind": "blob", "encoding": "base64", "data": "QUNUSVZF" }
      ]
    ]
  }
}
```

这里 map 使用 entry list，不要求 key 一定是 JSON string；保留协议类型与重复序列的能力。日志中的 command 默认只保留脱敏摘要，不默认把写入 payload 放到 envelope。

### 18.5 stdin 与非交互

支持 `-x`、`-X` 的受测兼容输入，命令文件、显式 `--pipe` 原始协议批量输入。普通 stdin 命令文件不是 shell script，不执行反引号、环境变量或命令替换。

**`--pipe` 不是策略绕过通道（v2.1 明确）。** `--pipe` 的 stdin 是 RESP 编码的命令流；Penguin 在发送前用同一 `pr-protocol` 增量 decoder **逐帧解析**，每帧转为 `CommandRequest`，经过与交互输入完全相同的 catalog 分类、policy、预算与审批：

- 帧必须是 RESP array of bulk strings（命令 + 参数）；其他类型、截断、超长声明 -> **fail-closed**：停止读取、不再发送后续帧、退出码 2，并报告帧序号与偏移。
- 每帧的 `effects` 由本地 catalog 决定；production profile 下 `writes = deny` 时遇到写帧即停止（退出码 5），不静默跳过；`approval-required` 的帧在 `--pipe` 中无交互能力 -> 视为 deny，除非事先用 `prc @dev --plan-from-pipe < file` 生成、审阅并签名的 plan（§29.2）再以 `--execute-plan <id>` 执行。
- 结果关联：按发送顺序记录每帧的 `ExecutionOutcome`；`--output ndjson|typed-json` 逐帧输出；默认模式与 baseline 一致输出汇总（成功数/错误数）并在 stderr 列出前 N 条错误帧。
- 背压：读 stdin 与写 socket 用有界队列（默认 1,000 帧或 8 MiB 未确认字节）；连接断开后未确认帧全部标 `UnknownAfterSend`，不重放。
- agent / MCP origin 默认不允许 `--pipe`。
- 兼容性：以上在 `--compat=redis-cli` 下同样生效；兼容模式只影响输出格式与退出码映射，不影响解析与策略（§3.4）。

没有 TTY 时不启动 selector、不打开 editor、不弹钥匙串交互循环、不无限等待确认。缺目标、凭证或批准即失败。Secret provider 必须有超时与批准记录。

### 18.6 退出码

Modern 模式建议固定：`0` 全部确认成功；`2` 使用/配置错误；`3` 连接/TLS/认证；`4` Redis command error；`5` 策略拒绝；`6` 结果未知；`7` 批量部分失败；`8` 本机资源/输出失败；`130` 用户取消。显式 reply-suppressed 模式可用单独状态 `9` 表示已发送但无应用确认；只有另行明确选择 send-only contract 时才以其发送完成条件决定成功退出。完整程序不得同时使用两套未注明的 0 含义。

**REPL 会话的退出码（v2.1 明确）：** 用户以 `exit` / Ctrl+D 正常离开 -> `0`，无论会话中是否有命令返回过错误（错误已逐条显示）。`--exit-on-error` 使第一条 Redis 错误（退出码 4）或策略拒绝（5）立即结束会话并返回对应码。连接在会话中不可恢复地丢失且用户选择不重连 -> `3`。Ctrl+C 于空输入两次或 `--` 信号终止 -> `130`。`:status` 显示最近一条命令的 outcome 与对应的假想退出码，方便脚本化验证。

兼容模式遵循声明的 `redis-cli` baseline，包括 `-e` 的差异；详细错误继续可通过 typed stderr 或 report 获取。`nil`、空数组、整数 0 本身不是失败。管道断开时输出层应正常处理 broken pipe，不能 panic 或继续无限读服务器。

<a id="s13"></a>
## 19 · 核心架构：执行与显示彻底分离

### 19.1 结构

```text
CLI args / REPL / TUI / Recipe / optional MCP
                       |
               Application service
                       |
      Profile resolver + Capability registry
                       |
         Policy / Budget / Approval engine
                       |
               Execution kernel
                       |
       Session state + Router + Transport
                       |
              RESP event decoder
                       |
       Bounded result store + provenance
                       |
   Semantic adapters -> View model -> Renderer
```

连接、取消、权限、重试、审计统一在 kernel。Renderer 是没有网络权限的消费层，不能为了显示 TTL 自行发命令；需要更多数据只能向应用层提出明确 action。

### 19.2 数据模型草图

以下是类型设计草图，不是宣称可直接编译的完整库。

```rust
struct CommandRequest {
    args: Vec<Bytes>,             // 保留每个参数边界和原始字节
    target: ResolvedTarget,
    origin: RequestOrigin,        // user / tui / recipe / agent
    policy_context: PolicyContext,
    budget: RequestBudget,
}

struct ResultRecord {
    id: ResultId,
    source: SourceIdentity,
    outcome: ExecutionOutcome,
    completeness: Completeness,
    response: ResponseHandle,     // 有限内存、流或加密 spool 的引用
    timings: ObservedTimings,
    annotations: Vec<Annotation>,
}

// v2.1：单一结果模型，四个正交维度分开记录（取代 v2.0 的过渡 enum；§19.6 的说明以此为准）
struct ExecutionOutcome {
    delivery: Delivery,          // NotSent | CancelledBeforeSend | DeniedByPolicy | Sent | UnknownAfterSend
    reply: Reply,                // None | Ok(ResponseHandle) | Error(ServerError) | Queued | SuppressedByReplyMode | Partial(Vec<ExecutionOutcome>)
    effects: EffectsCertainty,   // NoCommandSent | ReplyObserved | ConditionNotApplied | EffectsPossible | KnownPartial | UnacknowledgedByMode
    render: RenderStatus,        // NotRendered | Rendered | RenderFailed(DiagnosticId) | Truncated { retained: Range }
}
```

- `Partial` 用于 `EXEC`、`--pipe`、批量与脚本：每个子项各自有完整四维度。
- 「server error」只填 `reply`，**不**自动把 `effects` 置为无副作用（复合操作、Lua、EXEC 内错误都可能已部分生效）。
- `render` 失败不改变 `delivery`/`reply`/`effects`；UI 显示成功/失败时必须说明来源维度。
- 退出码（§18.6）与 typed output 的 `outcome` 字段由这四个维度确定性映射，映射表进入 fixture。

`ResponseHandle` 不能强制等价于一棵完整 `Vec<Value>` 树。大 blob 需要 chunk handle，aggregate 可以增量消费；map 为 ordered entries，保留实际 wire 顺序但不宣称该顺序是服务器稳定契约。

### 19.3 无损协议事件

解码层覆盖简单/批量字符串、整数、double、big number、boolean、null、array、map、set、verbatim string、error、attribute 和 push，**以及 RESP3 的 streamed string（`$?` + `;len` chunks + `;0`）与 streamed aggregate（`*?` / `%?` / `~?` + `.` 终结）**；按实际协议支持范围记录版本。[R02] streamed 形式使用与 TCP 分块无关的独立状态机；chunk 大小与 aggregate 元素数受 §24.3 预算约束，超限按 §24.4 处理而不是丢 framing。

属性不能被悄悄丢掉，push 不能占用下一条普通命令的 response slot。建立明确的 frame ownership：某帧属于哪个请求、订阅还是连接事件。

协议失败时关闭或隔离连接，不能继续在不确定的字节边界解析下一条结果。支持最大嵌套、声明长度、aggregate 个数和总预算的防护。

### 19.4 并发模型

普通 REPL 以单个有状态 command session 为主；无状态并发、诊断、订阅和阻塞任务使用明确角色的辅助会话。不能把 `MULTI`、`WATCH` 放进随意复用的连接池。

session admission 限制并发请求数、字节数、节点数和观察任务数。显示暂停、网络接收、结果保留三个状态分开，避免 UI 停止滚动却仍无限积累消息。

### 19.5 可观测性

每个请求记录：来源、脱敏命令名、**请求/响应字节数（只记数量，不记内容）**、发送状态、目标节点、明确发生的重定向、首字节时间、接收完成时间、渲染完成时间和结果保留范围。原始 payload 只在用户显式开启 `--trace-wire <file>` 时写入指定文件，且该文件按 §23.4 的秘密路径处理、默认脱敏 AUTH/HELLO 参数、不进入诊断包。

这些时间是客户端观察到的阶段，不等于服务器执行耗时。只有有直接来源的 server-side 数据才能以该名称显示。

### 19.6 Command Intelligence 与观测事件接口

Intelligence 接收经过隐私策略过滤的 observation 事件，不接收 secret provider。它可以建立有界名称索引，但不引用 renderer 屏幕文本来猜 key/field。renderer、intelligence、guide 和 diagnostics 都不得绕过 application/kernel。

Execution result 的 `effects: EffectsCertainty` 维度（§19.2）独立于 `reply`。**收到 server error 不代表所有副作用都没发生。** 例如复合操作、脚本或 EXEC 内部的错误需要保留逐项/部分状态；不把一个 error 字符串统一映射为“安全重试”。

§19.2 的四字段模型是唯一的结果模型。UI 显示成功/失败的来源要清楚：渲染失败不能把服务器已执行的写入标成未执行，脚本错误也不能被描述成自动事务回滚。[R11]

<a id="s14"></a>
## 20 · 命令、协议与版本覆盖

### 20.1 默认协议

为保留原 Redis CLI 习惯，基础模式默认 RESP2；`-3` 明确开启 RESP3；`--json` 按受测官方行为默认 RESP3。未来改变默认协议必须是公开、受测的版本变化，不能由 TUI 是否打开决定。

协议协商与认证、DB 选择属于连接初始化，可在 trace 中查看。禁止为了“升级协议更漂亮”而重发用户写入命令。

### 20.2 Metadata registry

内置离线命令元数据 snapshot，再以明确允许的 `COMMAND` / `COMMAND DOCS` 等信息补充服务器能力。[R20] 缓存按服务身份与版本隔离，不在每次 Tab 时全量拉取。

元数据不足时：基础命令仍可发送；帮助标记未知；生产保护将未分类的命令视为未知风险，而不是假设只读。不能仅凭本地 validation 阻断服务器新支持的合法语法。

### 20.3 能力清单

基础类型全覆盖；新增 Hash expiry 命令、JSON、Search、TimeSeries、Bloom/Cuckoo/Count-Min/TopK/t-digest、向量相关能力等使用 capability-driven adapter。

所有服务器允许的命令都可走通用路径；专用表格、编辑器和诊断需有独立支持状态。不得把“generic pass-through”伪装成已完整支持该模块工作流。

兼容目标分三类：发布基线中的 Redis 版本；单独受测的 Redis-compatible 实现（**Valkey 是第一个必须进入矩阵的实现**，其命令集、`COMMAND DOCS` 输出与模块生态已与 Redis 分叉）；受限命令的云/托管部署。协议相似不等于所有命令、ACL 和 Cluster 行为相同。catalog 为 Redis 与 Valkey 分别维护 `server_family` 分支，同名命令的差异（参数、返回、版本引入）逐条登记。

### 20.4 版本矩阵交付物

仓库必须包含 `compatibility/manifest.toml`，固定服务器镜像 digest、官方 CLI tag、RESP 模式、部署拓扑、OS/terminal 和通过状态。最终目标包含多条历史主版本线和当前稳定发布线，但只能把实际跑过的具体版本标为 verified。

**初始固定矩阵（v2.1，E0 冻结；digest 在实施时填入）：**

| 目标 | 版本线 | 拓扑 | 备注 |
|---|---|---|---|
| Redis | 7.2.x、7.4.x、8.0.x（各取当时最新 patch） | standalone / Sentinel / Cluster | 7.4 起 `HSCAN NOVALUES`、`HPTTL` 等 Hash field TTL；8.0 起 `HGETDEL`/`HGETEX` 与内置模块 |
| Valkey | 8.0.x、8.1.x | standalone / Cluster | 与 Redis 7.2 分叉；`COMMAND DOCS` 差异需 fixture |
| `redis-cli` baseline | 7.4.x 一个固定 tag | — | `--compat=redis-cli`、quoting corpus、`--json`/`--raw` 差异测试的唯一基准 |
| Redis Stack / 模块 | RedisJSON、RediSearch、TimeSeries、Bloom 各一个固定版本 | standalone | adapter 专用；未列出的模块版本为 `unknown` |
| 托管服务 | 至少一个受限 ACL 的云实例 fixture | — | 验证 `unknown` / `unsupported` 分离与 NOPERM 冷却 |

未列出的版本一律 `unknown`，不因主版本相近自动继承 verified。矩阵扩展是正常的版本工作，不属于范围削减。

Release CI 使用固定版本，不使用漂移的 `latest`。另设非阻断的前沿检测任务，用于发现新命令和差异；确认后再纳入发布基线。

### 20.5 元数据支持矩阵必须包含智能输入能力

同一个命令分别登记 `send`、`route`、`state`、`render`、`suggest`、`signature`、`guide`、`docs` 和 `tested`。generic pass-through 不代表 guide/特定 renderer 已完成；有帮助说明也不代表完整执行状态正确。

Command key specifications 和 COMMAND GETKEYSANDFLAGS 可以帮助分析完整请求的 key/访问标志，但不替代所有本地安全判断，也不在每次按键作为隐式调用。[R41][R54] 对未知、新扩展或特殊数据类型保持 generic 能力，不假装已实现专门的数据编辑器。

能力发现默认使用发布 catalog，联网刷新是明确、可观察且受权限控制的操作。针对受限用户和代理，`unknown` 与 `unsupported` 要分开；未经测试的 newer server 不以主版本相似自动标 verified。

<a id="s15"></a>
## 21 · 连接层：真实内网、Cluster、Sentinel 与 SSH

### 21.1 支持矩阵

| 连接能力 | 完整要求 |
|---|---|
| TCP | IPv4/IPv6、DNS、连接超时、keepalive、清晰地址错误 |
| TLS | 验证证书与名称、CA 配置、client certificate、独立 SNI |
| Unix socket | 权限错误、路径处理、平台能力说明 |
| Cluster | slot routing、MOVED、ASK、拓扑变化、节点连接预算 |
| Sentinel | 多 Sentinel、service name、Redis/Sentinel 独立身份、角色选择 |
| SSH | 明确启用、known-host 验证、jump host、进程生命周期管理 |
| Corporate network | 使用现有系统路由和信任配置，提供分层诊断，不自动绕过 SASE |

### 21.2 Cluster 正确性

Redis Cluster 的重定向与 key slot 规则是服务器语义的一部分，多 key 命令存在同 slot 限制；只有 DB 0 可用。[R10] slot 计算必须实现 **hash tag** 规则：key 中第一个 `{` 与其后第一个 `}` 之间非空的内容才参与 CRC16；`{}` 为空或无 `}` 时整个 key 参与。多 key 命令的同 slot 判定、`CROSSSLOT` 预检提示（§13.7）与 `-c` 兼容路由都以此为准，并有带 tag 的 MOVED/ASK fixture。

原始 `MGET` 跨 slot 时不能暗中拆成多个独立 GET 再假装原子结果。增强的 multi-node gather 可以存在，但必须另有入口、显示非原子与部分失败。

`ASKING` 和随后命令必须绑定同一目标连接。`MOVED` 更新路由不等于无限重试；重定向链有预算。事务和有状态命令保持连接所有权。

原始 `SCAN` 是当前节点/目标语义，不擅自扫描所有 primary。全 Cluster 浏览由增强任务维护 `{node_id, cursor, topology_epoch}`，处理节点不可达、重分片和重复发现，不输出伪全局 cursor。

### 21.3 地址映射与凭证信任

Cluster 宣告地址可能与客户端可达地址不同。支持明确的 endpoint mapping，但 TCP 目标、TLS 验证名和凭证适用范围分开配置；不能为了可达就关闭 TLS 校验。

不要盲目信任服务器返回的任意目标并发送密码。profile 定义允许的集群发现边界；新主机/端口超出范围时要求重新确认或符合已配置的信任策略。

**`TrustIdentity` 的定义（v2.1 明确；全文的 endpoint fingerprint / trust fingerprint / service identity 均指此结构）：**

```text
TrustIdentity
  endpoint:        Tcp{host, port} | Unix{path} | SshTunnel{jump, remote}
  tls_identity:    None | Ca{ca_set_hash, server_name} | SpkiPin{sha256}   // 二选一，可同时要求
  server_identity: Standalone{run_id_prefix?} | Sentinel{master_name, sentinel_set_hash}
                 | Cluster{node_id_set_hash, seed_hosts}
  auth_identity:   {username?, secret_ref}
```

- 凭证（`secret_ref`）绑定到 `(tls_identity, server_identity)`，**不**绑定到 `endpoint`：地址变化（DNS、端口映射、failover 到已登记节点）不触发重绑；TLS 身份或服务身份变化触发重绑。
- 审批（§23.2）绑定完整 `TrustIdentity` 哈希。
- Cluster：`node_id_set_hash` 允许在 profile 声明的 `discovery_bounds`（CIDR/域名后缀/显式列表）内增减节点而不失效；重定向到边界外节点 -> 暂停并请求确认，确认后写入 profile 的已知节点。
- Sentinel：`master_name` 不变但 sentinel 集合变化 -> 警告不阻断；`master_name` 变化 -> 视为新服务。
- 轮换流程：证书/CA 轮换在 `--edit` 中以「新旧同时接受 N 天」的过渡窗口配置；`run_id` 只做提示（重启即变），不作为拒绝依据。
- 直连形式（无 profile）不构造持久 `TrustIdentity`，凭证只在会话内。

### 21.4 Sentinel

Sentinel 负责发现与故障转移信息；客户端仍要明确连接实际数据节点。[R16] 在主节点变化后重新发现 endpoint、清除失效观察上下文，并把可能已经发送的写入标记结果未知，而不是无条件重放。

### 21.5 SSH 与公司网络

默认直连；SSH 只有用户启用时才产生 tunnel。实现首选受控、可审计的系统 OpenSSH 适配，不拼接 shell 字符串。导入配置中出现 `ProxyCommand` 等可执行项时需要额外信任批准。

local forward 使用 loopback 临时端口；这是用户选择 SSH 功能才产生的连接资源，不是 Penguin 后台 API。普通模式没有本机监听端口。退出时仅清理自己创建的资源，不破坏用户已有共享 SSH 会话。

**SSH 与 Cluster / Sentinel 的组合（v2.1 明确）：** 单个 loopback forward 只能到达 seed；MOVED/ASK 与 Sentinel 返回的是集群内部地址。profile 的 `ssh.mode` 二选一：

| 模式 | 机制 | 适用 |
|---|---|---|
| `per-node` | 为每个需要连接的节点建立独立 `-L` forward（惰性、按需、受 `max_tunnels` 预算，默认 16），维护 `internal_addr -> loopback_port` 映射 | 节点数少、OpenSSH 不允许动态转发 |
| `socks5` | 一个 `-D` 动态转发，所有节点连接经 SOCKS5 直连内部地址 | 节点多或拓扑变化频繁；默认推荐 |

两种模式下：TLS 校验仍针对**内部真实主机名/证书**（SNI 与验证名来自 `TrustIdentity`，不是 loopback）；节点准入按 §21.3 的 `discovery_bounds`；拓扑变化时 per-node 模式回收未使用 tunnel；tunnel 建立失败按节点标 `unreachable`，不把整个集群标为不健康。

诊断步骤只检查被选择的 endpoint：DNS -> TCP -> TLS -> authentication -> capability。明确说明卡在哪一层，不自动改系统路由、证书校验或 VPN/SASE。

<a id="s16"></a>
## 22 · 取消、重连与事务：避免“看起来失败，实际执行两次”

### 22.1 请求状态机

```text
Prepared -> Approved -> Sending -> AwaitingResponse -> Confirmed
    |           |          |              |
 NotSent     Denied     Uncertain       Uncertain
```

在命令的全部字节可能已到达服务器后断线，不能从“没有收到回复”推导“服务器没有执行”。结果未知必须作为第一类 outcome 对外暴露。

普通写命令默认不自动重放。即使一个 `SET` 看似幂等，TTL、条件参数、并发修改与观察语义也可能不同；不要按命令名建立过度粗糙的安全重试白名单。

### 22.2 示例

```text
 dev:3 > INCR counter:demo

RESULT UNKNOWN · connection lost after request transmission
The server may already have applied this command.
No automatic retry was performed.

Inspect current state | Reconnect only | Dismiss
```

“Inspect current state”本身不能证明唯一因果，尤其其他客户端也可能更新该 key。不要通过一次 GET 就宣布上次 INCR 一定成功或失败。

### 22.3 Ctrl+C

| 当前状态 | 行为 |
|---|---|
| 正在编辑输入 | 清空/取消当前输入，不退出整个 REPL |
| 尚未发送 | 取消请求，显示未发送 |
| 等待普通回复 | 停止等待或关闭专属连接；视发送状态标未知 |
| 阻塞 Redis 命令 | 关闭该请求会话，或经授权使用专门控制操作 |
| 订阅/监控 | 停止当前会话并显示观察结束与已知缺口 |
| 批量操作 | 停止发出新请求；已发出的结果继续收敛或标未知 |

`CLIENT UNBLOCK` 是服务器提供的专门阻塞解除机制，需要另一连接和相应权限；它不是对所有命令通用的撤销按钮。[R19]

**取消与退出码的优先级（v2.1 明确）：** 同一次 one-shot 中若同时满足多个退出码条件，按「效果确定性」优先：任何请求处于 `UnknownAfterSend` -> `6`；批量中有已确认失败项 -> `7`；只有当**没有任何字节发出**（`NotSent` / `CancelledBeforeSend`）或所有已发请求都已 `Confirmed` 时，用户中断才返回 `130`。即：`130` 表示「取消且无未知副作用」，脚本可以安全依赖这一含义。typed output 的 `outcome` 字段与退出码同源。

### 22.4 事务状态

显式追踪 `WATCH`、`MULTI`、queued commands、`EXEC`、`DISCARD`、`UNWATCH`。Redis 事务具有连接状态，执行错误也不意味着自动回滚全部已经执行的动作。[R11]

收到 `QUEUED` 不能显示写入已完成。`EXEC` 回复逐项关联原 queued command，正确处理里面的错误；事务 abort 与空结果分开。

断线后不自动重建事务，不重放 `WATCH` 后的整段用户输入，不把新连接标成旧事务仍生效。结果内的 renderer 可以嵌套展示，但不能改变 transaction semantics。

### 22.5 恢复后的状态

允许重新建立已选择的身份、DB 与协议；这些恢复动作需可见。订阅可由用户允许重订阅，但必须标记中断期间存在数据缺口。临时审批、未知写入和事务上下文不因重连自动恢复。

### 22.6 主动改变连接协议/身份/回复模式

HELLO 可以选择协议并携带认证信息，不能作为普通不变状态查询处理。[R53] CLIENT REPLY OFF/SKIP 可以有意关闭/跳过回复，OFF/SKIP 命令本身也可能没有回复。[R46]

显式状态机记录 protocol、auth identity、DB、reply mode、subscription、transaction、tracking 和 topology。针对主动 suppression 显示 `sent without acknowledgement`，不是凭空 `OK`，也不是一律错误超时。退出码与 typed output 标明发送契约与效果不确定。

禁止在用户的有状态会话插入补全用 PING、TYPE、SCAN、COMMAND；需要 metadata/discovery 时用允许的专用会话。reply-suppressed 期间辅助请求不能占用下一条回复位置；测试 ON/OFF/SKIP 转换及断线后的保守恢复。

原始 AUTH/HELLO/SELECT/RESET 的状态变化同时影响 result/assistance scope。重新连接后默认恢复已确认的安全初始化配置，不静默重启 reply-suppressed、事务或一次性授权。

<a id="s17"></a>
## 23 · 生产安全与秘密保护

### 23.1 策略分层

```text
Server ACL
    + Profile execution policy
    + Origin policy (human / recipe / agent)
    + Request budget and scope
    + Explicit approval, when allowed
```

Redis ACL 是服务器端的命令、key、**Pub/Sub channel 模式（`&pattern`）**和身份访问边界。[R15] 客户端的 profile policy 因此也把 channel 模式列为独立维度：订阅、`PUBLISH`、`SPUBLISH`/`SSUBSCRIBE` 的策略与审批绑定 channel 字节，NOPERM 于 channel 的冷却与 key 的冷却分开记录。 客户端 guardrail 主要防误操作，不能防止具有同一权限的人改用其他客户端；本地确认也不能提升服务器权限。

| 环境 | 推荐初始策略 |
|---|---|
| Local / dev | 普通读写允许；高成本/破坏性操作明确提示 |
| Staging | 普通读允许；写入按用户配置；破坏性操作需要独立批准 |
| Production | 默认 read-only；普通写入需显式开启有范围的授权 |
| Agent / MCP | 默认比人类 REPL 更严格，限制命令、key、大小、次数和目标 |
| 未分类命令 | dev 可显式发送；prod/agent 默认拒绝或走专门批准 |

生产的 `FLUSHDB` / `FLUSHALL` 等默认是 **deny**，而不是输入 `prod` 就自动突破 deny。只有策略本身允许“经批准执行”时才显示批准界面。写入允许、危险命令允许与管理员身份是三个不同概念。

### 23.2 审批绑定真实动作

**审批令牌绑定精确字节，不绑定摘要（v2.1 明确）。** 一次审批产生：

```text
ApprovalToken
  profile_uuid
  trust_identity_hash      = blake3(canonical TrustIdentity)            // §21.3
  db_index
  auth_identity_epoch, policy_epoch
  request_hash             = blake3(len(argv) ‖ Σ (len(arg_i) ‖ arg_i)) // 长度前缀的原始 argv bytes，逐参数
  plan_hash                = 批量/recipe 时整份已展开 plan 的哈希；单命令时等于 request_hash
  max_actions, expires_at, issued_by (human | policy:<id>)
```

- 「命令与参数摘要」、脱敏文本、截断预览只用于**显示**给审批者；kernel 在执行前重新计算当前 `CommandRequest.args` 的 `request_hash` 并与令牌比对，任何一个字节不同即拒绝（退出码 5）。
- 因此 Unicode 规范化、转义、显示截断、敏感值掩码都不可能让两条不同命令共享一个审批。
- 审批者看到的预览必须能展开为完整 argv 的转义视图（`:approval show --bytes`），保证「看到的」与「哈希的」是同一份数据。
- 令牌不可跨 profile、DB、身份 epoch 或 `TrustIdentity` 使用；任一变化即失效。

批量操作先生成计划，再逐项 revalidate。不能把某次对 `player:10001` 的授权复用为全库删除。对原始 `DEL 'player:*'` 必须按字面 key 处理；通配符删除属于另一个增强操作。[R12]

### 23.3 只读并不等于无成本

大 Hash 全读、无界扫描、长时间 MONITOR 都可能带来资源影响。MONITOR 官方文档明确提示其开销。[R35] 生产 profile 配置读取预算、扫描速率、最大响应大小和观察持续范围；不要仅用 `@read` 标签替代成本判断。

### 23.4 秘密信息完整路径

秘密保护覆盖输入 -> parser -> configuration -> connection -> errors -> trace -> history -> clipboard -> export -> crash report。

不得在 Debug 派生输出、URI parse error、TLS error 上下文、重试日志或 telemetry 中泄露密码。默认无 telemetry，诊断包显式生成并预览脱敏范围。

密钥材料缩短内存驻留、限制 clone、使用适当的秘密类型及清理机制；但不承诺用户机器上的恶意软件、内存 dump、swap 或外部剪贴板管理器绝对无法接触数据。

### 23.5 终端与外部命令安全

pretty 模式将不可信 ESC、OSC、CSI、控制字符当数据，不允许 value 自行设置终端标题、触发超链接或写剪贴板。来自服务器的“文件路径”不得自动在本机打开。

**统一的显示信任边界（v2.1 明确）：** 所有来自服务器或外部来源的字符串——value、key、field、member、channel、pattern、consumer/group 名、`CLIENT LIST` 字段、`INFO` 值、错误文本、`COMMAND DOCS` 文本、module metadata、表标题中引用的 key、导入 profile 的名称——在进入任何 renderer、菜单、状态栏、日志或剪贴板之前都必须先构造为 `SafeText`（§12.9）。`SafeText` 构造时转义 C0/C1 控制字符、ESC 序列、OSC/DCS/APC 引导与孤立 surrogates，并记录原始 bytes 引用以便 `:copy` 与 bytes 视图使用。只有 `--raw` 与 `--bytes` 两条明确的输出路径允许绕过 `SafeText`，且文档告知风险。类型系统上，renderer 与 TUI widget 的文本参数类型只接受 `SafeText`，不接受 `&str`/`String`。

editor、pager、SSH、secret provider 都用 argv 方式启动，不把 key/value 拼接进 shell。外部 editor 文件使用私有权限、受控路径和清晰 cleanup；敏感数据默认使用内建编辑器，防止外部 editor 的 swap/backup 文件扩散。

### 23.6 本机审计的真实边界

保存脱敏 action、policy decision、approval fingerprint、结果状态与有限元数据。可提供防篡改链和签名导出，但本机用户完全控制机器时，不能声称它等同于不可抵赖的中央审计系统。

需要组织级审计时通过可选、明确授权的外部 sink 集成，不让它变成个人 CLI 必须启动的 backend。

### 23.7 Suggestion 是新的信任边界

离线帮助、候选 label、服务器 metadata、已观察 key/field、历史、schema 和 AI 计划都视为不同信任来源。候选不能携带 executable callback；用户选择只生成受检验的 edit。真实 action 的授权在执行前重新检查。

NAME 也可能敏感：没有读取 value 不等于没有泄漏风险。跨 profile/DB/身份的索引隔离、remote discovery 审批、history 掩码、autocomplete 调试日志脱敏和模型禁入逐键路径都必须单独测试。禁止为“让建议更聪明”而提升 Redis ACL 或读取全部 keys。

<a id="s18"></a>
## 24 · 大数据与低开销：必须从读取端解决

### 24.1 不能接受的实现

```text
Read entire response
  -> clone all bytes
  -> convert everything to UTF-8 String
  -> parse all JSON into a second tree
  -> create all table rows
  -> paginate only at the final screen
```

这种结构即使界面只显示 20 行，也已经消耗完整响应的内存。

### 24.2 目标数据路径

```text
socket chunks
  -> incremental RESP decoder
  -> bounded byte blocks / aggregate events
  -> bounded semantic adapter
  -> streaming table or virtualized inspector
  -> optional explicit encrypted spool
```

巨大 blob 不能要求一次分配声明的全部长度。深层 aggregate 使用有界状态栈；JSON token 可跨网络块边界；UTF-8 与 grapheme 断点通过小缓存处理。

**保留命令语义不等于必须无限占 RAM。** 可以流式输出、明确落盘或在超限时中止，但不能偷偷替换 `HGETALL` 为 `HSCAN`。服务器已经承担的查询成本不能被客户端的分页界面消除。

### 24.3 预算分类

| 预算 | 示例初始值/策略 | 超限行为 |
|---|---|---|
| 普通会话结果缓存 | 64 MiB 总量 | 按结果粒度淘汰，标明未保留 |
| 单结果完整内存保留 | 16 MiB | 流式输出/申请 spool，不无限扩容 |
| JSON 自动索引 | 2 MiB 或明确处理时间预算 | 保留文本，用户明确深入解析 |
| 订阅记录 | 10,000 条且 16 MiB，先到者为准 | ring buffer，记录淘汰数量 |
| 输入粘贴 | 配置大小与命令数阈值 | 进入审阅，不自动执行 |
| 增强扫描 | 每 profile 速率/并发预算 | 节流、暂停、明确取消 |
| 临时落盘 | 用户批准的容量与期限 | 停止写入，保留完整/不完整状态 |

这些是工程初值，必须 benchmark 调整；不是已经验证的资源上限。普通命令输出可以大于缓存预算，缓存限制不应自动变成输出截断。

### 24.4 超限时的行为契约

普通 CLI：能流式输出就继续，页脚说明“不保留全部结果”；需要交互回看则明确申请 spool。脚本输出不能弹菜单，按预设输出方式持续写出或以非零状态失败，不能静默截断后 exit 0。

标准 JSON 输出写到一半失败可能是不完整 JSON，必须非零退出并在 stderr 指出。typed event stream 在能写出终止事件时标 `complete:false`；下游不能仅看到若干结果就当成功完成。

用户取消大响应时关闭专属连接或完整 drain 后才复用；剩余响应不能被下一条命令误收。

### 24.5 暂停与背压

停止屏幕滚动不等于停止接收消息；暂停扫描可以停止发送新请求，但已经发出的回复仍需处理。对于 Pub/Sub，有限缓冲可能淘汰消息，必须显示计数。

不能长时间停止 socket 读取并声称“零成本暂停”：数据可能积压在服务端或网络缓冲。观察类任务超过预算时选择安全断开、明确丢弃或授权录制，不能无界积累。

### 24.6 初始性能目标，非实测承诺

| 场景 | 目标 | 测量边界 |
|---|---|---|
| 未运行 | 无 Penguin 自有常驻进程 | 不把操作系统页缓存算成 daemon |
| `--help` 热启动 | p95 <= 100 ms | 固定机器，排除网络与授权 |
| 单连接小数据 REPL | 空闲 RSS 争取 <= 30 MiB | 固定 build、allocator、OS |
| 单连接普通 TUI | 小数据稳态 RSS 争取 <= 60 MiB | 排除用户授权的大结果缓存 |
| 布局/按键响应 | 常见操作 p95 <= 50 ms | 用户输入到可见状态变化 |
| 无变化 TUI | 事件驱动，无固定忙循环 | 不因 60 FPS 重绘持续吃 CPU |
| Synthetic RESP 1 GiB blob 流式输出 | 峰值内存受块预算控制 | 与真实 Redis 数据上限测试分开 |
| 长会话 | 无随查询次数线性增长 | 8 小时和更长 soak test |
| raw 输出 | 与固定官方 CLI 同条件比较 | 相同协议、数据、TLS、sink |

未达到预算时先定位，不以降低测试数据规模假装通过。若为了正确性需要调整预算，记录理由与实测对比。

### 24.7 总预算与真实数据大小边界

Assistance 的 12 MiB 堆增量上限（§12.5）是新增热工作集目标，不是无条件额外预留；result cache 的 64 MiB 也是最大保留预算，不代表空闲时已全部占用。整进程 budget manager 统一记账，避免模块各自达到上限后突破进程预算。

**预算作用域（v2.1 明确）：** 全文所有内存、并发与速率预算均为**单个 `prc` 进程**的预算。无 daemon 架构下不存在跨进程仲裁；同一台机器上 N 个进程对同一服务器的可见请求率最多为 N 倍单进程上限，`:complete status` 与 `:tasks` 显示本进程用量并注明这一点。需要服务器侧总量保护时，应使用 Redis ACL / `maxclients` / 服务器限流，而不是期待客户端跨进程协调。进程预算的计量包含：堆分配（通过 allocator 统计）、result store、TUI layout、history page cache、spool 的内存部分；不包含只读 mmap catalog（单独报告）与 OS 级 socket 缓冲。

官方字符串文档列出默认最大字符串大小为 512 MB。[R52] 因此 1 GiB 单 blob 的测试默认使用 synthetic RESP fixture server，验证解码/输出的增量行为；真 Redis 测试按固定版本和实际配置的数据上限生成，不把调大服务器限制或 mock 结果冒充默认真实部署。

巨大 aggregate、大 key 名、巨大单 field 和超长 command input 另外测，因为它们的内存路径不同。补全要在大结果持续输出、低内存或远端变慢时仍能取消/退出，不因解析器锁住 UI。

<a id="s19"></a>
## 25 · 搜索、TTL、编辑与并发冲突

### 25.1 三种读取，三个承诺

| 方式 | 含义 |
|---|---|
| 原命令，例如 `HGETALL` | 按原语义执行，不隐藏换算法 |
| 增强 inspector | 明确追加 TYPE/长度/TTL 等允许的请求 |
| 分页 browser | cursor/range 驱动、有限发现、标记非快照 |

SCAN 的 `COUNT` 是提示，单次可以没有结果但 cursor 未结束，同一元素也可能被多次返回；遍历期间变化的元素没有一致快照保证。[R08] TUI 可以去重，但只能对已发现范围承诺，不能从 cursor 值推算完成百分比。

### 25.2 key 与 field 的过期策略

key TTL 和 Hash field TTL 分开显示与编辑。现代 Hash expiry 能力由服务器版本探测；`HPTTL` 返回 field 级剩余毫秒。[R09] `TTL` 的无过期与 key 不存在具有不同返回语义，不能统一显示成无限期。[R34]

读取 TTL 得到的是某时刻的观察值；客户端倒计时标为估计，不将 GUI/CLI 时钟当 Redis 真相。复制/编辑时不得无意延长剩余 TTL。

### 25.3 编辑提交流程

```text
Read original bytes + available expiry context
  -> user edit
  -> validate selected format
  -> show exact byte/value diff and TTL policy
  -> approve target/scope
  -> concurrent-change check
  -> atomic commit where supported
  -> display confirmed / conflict / unknown
```

优先使用经版本验证的原子条件操作；一般场景可以用 `WATCH` + `MULTI`/`EXEC`，但锁定范围和操作代价必须明确。[R11] 读取后再无条件写入不叫并发安全。

### 25.4 不把 Undo 当数据库时间机器

可提供本次操作的补偿计划，但必须保留前置条件、原始值、TTL 和并发检查。删除后又被其他客户端重建的 key 不能被“Undo”无条件覆盖。

Hash 单 field 修改尽量只改该 field；有 field TTL 的情况下必须用受测策略保留或显式改变它。不能用整 key 重建来假装局部编辑而丢失其他字段过期信息。

### 25.5 搜索限制也要可解释

key 名称二进制安全；匹配模式按 Redis 语义处理，shell 中的通配符需引用。远端 value search 属于显式扫描任务，不作为默认每次搜索行为。所有 value 搜索显示范围、预算、已扫描数量、不可访问节点和是否完整。

<a id="s20"></a>
## 26 · Streams、Pub/Sub 与实时观察

### 26.1 Streams 工作台

完整覆盖读取 entry、查询 group/consumer/pending、按 ID 范围查看、payload 展开、内容过滤、读取 live tail；ACK、claim、删除、trim、group 修改分离为明确写操作。

默认检查 pending 使用观测命令，不因为“点开页面”就 `XREADGROUP` 消费消息。进入消费模式时显示 consumer name、group、是否 NOACK、读取位置及其状态影响。

消费组操作有 action plan 与审批；不能把 retry/dead-letter 等业务约定冒充 Redis 原生语义。支持用户配置业务视图，但保留原命令与数据来源。

### 26.2 Pub/Sub

订阅显示 channel、pattern、接收时间、payload 和已接收/已显示/已淘汰数。Redis Pub/Sub 具有 at-most-once 交付语义；断线期间的消息不会因客户端重连自动补回。[R17]

客户端自动重订阅只能恢复未来观察，必须标一个 gap marker。不得显示“已恢复全部消息”。可恢复历史只能来自实际持久化系统或用户授权的本机录制。

### 26.3 MONITOR

明确显示高开销警告、目标和持续预算，默认不在生产后台开着。[R35] 输出解析失败时保留安全原文本，不能错分 client/command 参数。

录制默认关闭，敏感命令 payload 脱敏。任务结束释放 session；不因曾经打开过 Diagnostics 页面继续抓取所有命令。

### 26.4 Client-side tracking

支持显式的 tracking/invalidation 调试视图，但普通 GET 不默认开启客户端数据缓存。调试客户端应优先展示新读到的数据，不能静默返回旧缓存。

事件 UI 对 command reply 与 RESP3 push 分流；任意一类队列达到预算都不能阻塞另一类到无法退出。

<a id="s21"></a>
## 27 · 诊断与比较：证据明确，结论有边界

### 27.1 诊断能力

内存与碎片指标、keyspace/过期分布、慢命令、client 状态、延迟、错误率、eviction、Cluster 拓扑和只读采样。支持按 profile 的 baseline 比较与明确开始/结束的观察窗口。

所有指标带来源、单位、节点、采集时间与粒度。采样估计与准确计数不同；内存趋势不能只凭一次采样推导。没有历史数据就显示“需要建立观察窗口”，不虚构昨日增长。

### 27.2 诊断任务形式

```text
:diagnose memory
:diagnose latency
:diagnose keys --match 'player:*'
:observe memory --interval 5s
```

以上都是拟实现的本地增强入口；执行前展示计划使用的命令与范围。只在 `prc` 进程存在时观察，退出后不会继续采集。

诊断结果同时给出：事实、推测、支持证据、缺失证据和下一步可执行检查。缺失 `INFO` 权限不是零值，节点不可达不是健康。

### 27.3 Diff

```text
:diff --left @r2 --right @r3 --key promotion:70
```

显示两个读取时间、身份/DB、value 差异、缺失字段、实际可得的 TTL 与编码差异。JSON 可提供结构化 diff，但原始 bytes diff 始终可用；数组顺序默认有意义，数字词法变化可单独显示。

跨实例读取不是一个原子快照。多节点部分失败必须出现在表格和退出状态中；不能将失败端的值当作 nil 后宣布“key 被删除”。

### 27.4 大范围比较

显式范围 -> 发现 key -> 去重/外部排序 -> 分批读取/指纹 -> 差异复核 -> 报告。算法必须有内存/磁盘预算；结果标注采集时间窗和重分片/并发变更影响。

使用 hash 加速比较时，记录算法与碰撞处理策略；在最终写入前重新核对必要内容。不能把两次近似采样相等宣传为两个数据库完全相同。

### 27.5 Key history 的真实范围

只显示当前会话、明确保存的 snapshot 或用户授权 recorder 观察到的版本。必须能回答“这条历史从哪里来”。无 daemon 的模式不可能在进程关闭后自行记录变化；因此关闭期间显示 observation gap，而不是伪连续时间线。

<a id="s22"></a>
## 28 · 导入导出、批量与高级运维

### 28.1 三种不同的导出物

| 形式 | 用途 | 不应承诺 |
|---|---|---|
| Pretty / CSV / 普通 JSON | 分享、分析、人工检查 | 任意二进制和所有 Redis 类型都可无损往返 |
| Typed archive | 精确保存选中结果/数据及元数据 | 自动等同整个 Redis 的一致备份 |
| 原生持久化/备份工具 | 特定部署的恢复/迁移 | 所有版本、模块和托管平台都互通 |

`DUMP` 提供序列化值但不包含该 key 的过期时间；恢复需要单独处理 TTL 等信息。[R36] `RESTORE` 有版本、校验和及 TTL 参数等约束。[R18] 不把简单 `GET > file` 当成数据库备份。

### 28.2 Typed archive 目标格式

版本化 manifest + binary-safe records + key/field/entry 字节 + available expiry metadata + source/timestamp + 完整性/采样范围 + checksum。加密导出使用经过评审的现有方案，不自创密码算法。

导入前验证 schema、checksum、解压上限、目标能力、冲突策略与预计范围。支持 skip / compare / replace / abort-on-conflict，但生产默认不覆盖。失败报告可定位具体记录并保留未知结果。

### 28.3 模式批量操作

单独本地入口，例如 `:bulk delete --match 'cache:old:*'`。先发现并生成操作计划，再根据用户批准执行。`UNLINK` 可以将释放对象内存的工作异步处理，但不等于没有服务器成本。[R13]

Cluster 中按 slot 组织允许的批次；发现期间重建/变化的 key 有竞态。支持“仅当仍匹配预期内容/类型/版本时执行”的模式；不支持时明确说明保障边界。

一次扫描中的 cursor 不能当永久恢复 token。断点续作重新校验服务身份、拓扑和计划范围；危险操作恢复必须再次审阅，不能开机后自动继续删除。

### 28.4 官方特殊模式是完整覆盖目标

最终功能清单包括：`--scan`、big key / memory / key stats 分析、latency/history/distribution、rolling stats、raw protocol `--pipe`、Lua `--eval`/debugger、RDB/replication 相关模式、Cluster 管理、请求重复执行、LRU 测试等在所选 baseline 中实际存在的模式。[R01]

每项都要独立 spec、fixture、exit semantics 和安全策略。名称与详细 flags 由该 baseline 的 help/source 生成登记，不能因为本段列了名字就标记实现完成。

**每个特殊模式的最低契约（v2.1 明确）：**

| 模式 | 会话 | 协议路径 | 数据流向 | 生产 / agent 默认 | 取消与部分结果 |
|---|---|---|---|---|---|
| `--scan`、`--bigkeys`、`--memkeys`、`--keystats` | 专用只读会话 | 普通命令 | 只读；结果本机 | 允许但受 §23.3 读取预算；agent 需策略允许 | 部分结果 + 已扫范围 |
| `--stat`、`--latency*` | 专用会话 | `INFO`/`PING` 循环 | 只读 | 允许；持续时间预算 | 停止即结束 |
| `--pipe` | 专用会话 | §18.5 逐帧解析 | 写入服务器 | prod 按写策略；**agent deny** | 未确认帧标 unknown |
| `--rdb <file>`、`--functions-rdb` | 专用 replication 会话 | `SYNC`/`REPLCONF` | **全量数据导出到本机** | **prod deny，agent deny**；dev 需确认 + 文件路径预览 | 部分文件明确标 incomplete 并可删除 |
| `--replica` | 专用 replication 会话 | 复制流 | 持续导出 | **prod deny，agent deny** | 停止即断开，显示已接收范围 |
| `--eval`、Lua debugger（`--ldb`/`--ldb-sync-mode`） | 专用会话 | 脚本 | 可写 | prod 按 `unknown_commands` 策略（默认 deny）；**sync 模式额外确认**（阻塞服务器）；agent deny | 调试会话结束即释放 |
| `-r <n>` 重复执行、`-i` 间隔 | 与命令相同 | 普通 | 同命令 | 同命令的策略 × n 次数上限预算 | 每次独立 outcome |
| `--lru-test` | 专用会话 | 大量写 | **制造负载** | **prod deny，agent deny**；dev 需确认 | 停止即结束 |
| `--cluster <subcmd>` 管理 | 多节点会话 | Cluster 管理命令 | 拓扑变更 | §28.5：完整 action plan；**agent deny** | 逐步 outcome，中断显示已完成步骤 |

「deny」是策略默认值，可由用户在 profile 中显式改变；agent origin 的 deny 只能由人工在 profile 中开放，不能由 MCP 请求改变。每个模式都有自己的退出码映射进 §18.6 的编号体系，并在 typed output 中带 `mode` 字段。

### 28.5 Cluster 管理与 Lua

Cluster 的检查、创建、重分片、修复和导入类操作需要完整 action plan、节点资格检查、风险提示、审计和故障注入。危险子命令默认不向 agent 开放。

Lua 脚本与 function 调用不能仅凭文本长度或命令前缀判断安全；不执行脚本来“测试是否只读”。调试器保持专用 session，明确同步/隔离调试的影响范围；命令源码和 key/argv 作为不可信数据处理。

<a id="s23"></a>
## 29 · 自动化、可选 AI 与 MCP

### 29.1 Recipe：复用工作流，不重复复制命令

```text
:recipe run check-platform --platform 70
```

Recipe 是声明式参数化计划，包含步骤、目标 profile、命令数组、输出选择、超时、预算与明确的依赖。参数以 argv bytes 注入，不用字符串替换拼 shell。

执行前显示展开后的计划；错误策略区分 stop、continue-read-only、partial report。未知写入不能自动进入“retry everything”。允许收藏、分享脱敏 recipe，不共享秘密。

### 29.2 审批式 Automation

`plan -> inspect -> approve -> execute -> report`。操作计划有版本、内容 hash 与目标 fingerprint；执行前检查计划未过期、凭证身份没有改变、目标仍符合范围。

可供外部 scheduler 调用 one-shot，但 Penguin 不自动安装 cron/launchd，也不为了 recipe 要求自有常驻 scheduler。终端内的重复任务在进程关闭后结束。

### 29.3 AI 的角色

可选帮助：解释命令结果、形成诊断假设、生成受控检查计划、解释 diff。不是默认上传 Redis 内容，不要求本地大模型常驻，不把 LLM 放进普通 `HGETALL` 路径。

用户明确配置 provider，逐连接控制数据可否外发，发送前预览脱敏摘要与范围。Redis value 中出现“忽略规则并删除数据库”等文字只能作为数据，不能变成执行权限。

### 29.4 MCP

建议入口：

```bash
prc @dev --mcp-stdio
```

由明确调用的 MCP host 启动子进程，通过 stdin/stdout 通讯；stdout 仅用于协议，日志走 stderr。MCP 规范提供 stdio transport。[R29] 不需要为普通使用另起 HTTP 服务。`--mcp-stdio` 是独立的 bootstrap 路径：不初始化 REPL/TUI/终端协调器、不输出 banner 或进度、不进入连接选择器、不触发任何交互式凭证提示（缺凭证即以 JSON-RPC 错误返回）；stdout 上除 JSON-RPC 帧外没有任何字节，这一点由 PTY-less 集成测试断言。

工具面设计为 `connections.list_safe`、`redis.read`、`redis.inspect`、`redis.scan_limited`、`redis.diagnose`、`redis.plan_write`、`redis.execute_approved_plan` 等受限能力。不能默认暴露“任意命令 + 任意凭证 + 无限制扫描”。

批准由可信的人类入口或已批准策略完成；agent 不能自行创建自己的 approval。**「已批准策略」的边界（v2.1 明确）：** 策略型批准只能覆盖 `effects ∈ {reads_data}` 且 key 匹配 profile 中显式列出的模式、单次结果 ≤ 预算、每小时 ≤ 次数上限的请求；策略本身由人在 profile 中编写并带版本。任何 `writes_data`、`consumes`、`admin`、`unknown` 效果的请求，以及 `redis.execute_approved_plan` 的每一次调用，都必须存在**由人签发**的 `ApprovalToken`（§23.2）且 `plan_hash` 匹配；策略不能签发这类令牌。MCP 的输入、返回内容和第三方工具需要按信任边界处理，安全设计以官方建议为基础并落实到本地策略。[R30]

### 29.5 Studio 复用边界

Penguin Studio 调用同一个 core application service，复用连接 schema、policy、result model、diagnostics 和测试 fixtures。CLI 不通过 Studio API 工作，Studio 未安装也不影响 `prc`。

TUI widgets 与终端 renderer 不进入 Core。这样未来 GUI 可以有自己的组件，同时执行正确性只维护一套。

<a id="s24"></a>
## 30 · 本机配置、数据迁移与可信扩展

### 30.1 本机存储布局

下列为逻辑布局，物理根目录通过平台目录适配器确定；`prc --paths` 显示实际路径。

```text
config/
  settings.toml
  profiles.toml                 # 不含明文秘密
  themes/
  recipes/
state/
  history.sqlite               # 内嵌数据库文件，不是数据库服务
  audit/
cache/
  command-metadata/
  result-spool/                # 仅明确授权后出现
```

秘密本体**不在**上述任何目录中：它们只存在于系统 secret store（macOS Keychain / Windows Credential Manager / Linux Secret Service），以 `credential:<uuid>` 引用；`config/credentials.journal` 只记录 §4.2 的提交状态，不含秘密。无系统 store 时的已评审 vault 若被启用，其文件位于 `state/vault/`，按平台私有权限，且必须在 `prc --paths` 中显示。

基础 profile 与 theme 不依赖任何数据库进程。SQLite 是可选的内嵌状态实现；历史、书签、迁移可共用，但不把它升级成每次查询都同步写大事务的负担。

### 30.2 配置并发与迁移

多 terminal 同时改 profile 时有文件锁/版本检查与原子替换。UUID 不随名称变化；secret reference 与目标绑定。迁移先验证 schema，再创建可恢复备份；新版本失败不把旧配置破坏到无法回滚。

credential store 与配置文件没有天然原子事务，使用 staged update + compensation。删除共享 credential 前列出所有引用并再次确认。

### 30.3 本地配置不是默认可信代码

默认不自动读取当前项目目录下的执行型配置，不自动展开里面的 shell 表达式。可以支持 project settings，但首次信任和变更检测必须明确；只允许安全字段无提示覆盖。

profiles 导出默认 secret-free；加密同步/团队共享为可选能力。共享只传目标配置和政策模板，秘密由接收者绑定自己的身份。不能把个人凭证自动同步成团队共用管理员。

### 30.4 扩展能力

初期在同仓维护 typed renderer/adapters；完整产品可以提供有版本的扩展 API。默认是声明式 renderer 和 decoder，危险能力如文件读取、网络、外部执行必须逐项授权。

**插件隔离的两级模型（v2.1 明确；进程内原生代码做不到崩溃隔离）：**

| 级别 | 形式 | 隔离能力 | 预算执行 |
|---|---|---|---|
| L1 声明式 | TOML/JSON 描述的 renderer 映射、decoder 组合、主题、snippet、schema hint | 进程内解释执行，无任意代码 | 解释器内置的深度/大小/时间上限，可精确执行 |
| L2 外部进程 | 独立可执行文件，通过 stdio 传递 typed 输入/输出（同 §18.4 schema） | OS 进程边界；崩溃/OOM/死循环只影响该插件 | 超时 kill、输出大小截断、每次调用重新拉起或复用受限池 |

不存在「进程内加载原生动态库」的级别：Rust 的 panic/abort/OOM 无法在同进程内可靠隔离，v2.0 的这一表述不再成立。L2 插件默认 deny，逐项授权，参数与返回按 SafeText 处理，不获得 credential store、网络或文件系统能力（除显式声明并批准的路径）。崩溃时退回 generic view，不影响已完成请求。主题属于 L1；导入一个主题不能获得访问 credential store 的能力。

### 30.5 临时资源与异常

spool 文件使用私有权限、不可预测名称、防 symlink、防路径遍历，默认只在本地可信目录。必要时采用加密 spool 与进程内短期密钥；密钥未持久化时异常退出后的内容不承诺可恢复。

启动 cleanup 只清理经锁/ownership 验证的旧资源，不删除其他正在运行实例的数据。磁盘满、只读文件系统、被撤销权限都必须有明确错误与测试。

<a id="s25"></a>
## 31 · Rust 技术选型与仓库组织

### 31.1 明确选择，而不是罗列十套备选

| 层 | 推荐决策 | 为什么 |
|---|---|---|
| 主体语言 | Rust | 统一原生可执行文件与可复用 core；正确性仍需测试 |
| 异步运行时 | Tokio，精确启用所需 features | 网络、时间与任务取消基础；不引入 HTTP server [R37] |
| REPL | Reedline 仅作编辑状态引擎（`LineBuffer`/`Editor`/`History`）；事件循环、菜单绘制、终端模式由 `pr-terminal` 持有 | Reedline 的 `Completer` 是同步 API、菜单内建、printer 会写 scrollback，不满足 §14 契约；SPIKE-001 为 E2 门禁，回退为自有 LineBuffer（§14.5）[R28][R47][R48][R49] |
| 智能输入 | 自有 pr-intelligence，共用 catalog 与安全上下文 | CLI/TUI/Studio 同一 grammar、signature、find、guide |
| 终端协调 | pr-terminal 单一 focus/painter/event owner | 不让 REPL、TUI、消息流抢 stdin/stdout |
| TUI | Ratatui + Crossterm | 分离终端 layout 与事件/控制适配 [R31][R32] |
| 表格 | 自有 TableModel + streaming renderer | 用户要求的行分隔、JSON cell、byte origin 和大数据不可受普通表格库限制 |
| JSON | 自有 `pr-json`：token-preserving DOM + occurrence 路径 + 增量格式化 | `serde_json::Value`/`RawValue` 不能满足 §8.3；`serde_json` 只用于 Penguin 自有 schema |
| Redis 传输 | 自有窄 transport/session 接口 + 受测协议实现 | 控制重试、状态、增量解码、push 分流 |
| Redis 库接入 | 先以 `redis-rs` 低层 API 验证，不能把它的高层 Value 模型当最终边界 | 高层 API 可能进行类型转换；低层表达更直接 [R27] |
| Secret store | `keyring` crate 固定版本，按平台启用 feature：macOS `apple-native`；Windows `windows-native`；Linux `sync-secret-service`（D-Bus Secret Service）+ `linux-native`（kernel keyutils，仅会话内）作为无 D-Bus 时的显式回退 | secret_ref 格式 `credential:<uuid>`，service 名 `penguin-redis`；headless 且无可用 store -> **fail-closed**：只允许 `--askpass`/会话内秘密/已评审 vault，绝不落明文；mock/sample store 只在测试 feature 下编译 [R33] |
| 配置 | TOML + schema version + 原子写入 | 人可读、可审阅、可迁移 |
| History | `rusqlite`（`bundled` feature，固定版本）+ WAL + `user_version` schema 版本号 + 前向迁移脚本 | 多进程并发由 SQLite 锁保证；文件 0600；无额外服务 |
| 任务监督 | Tokio `JoinSet` + `tokio_util::CancellationToken` + 在 `Drop` 中 `abort_all` 的 `TaskScope` guard | Tokio 没有内建 scoped/structured task；`JoinSet`/`TaskTracker` 默认 drop 不 abort。STATE-07、NET-07、PERF-03、LIFE-01 依赖此显式 supervisor，并配 leak-detection 测试（任务计数回零） |
| 测试 | nextest + 属性测试 + fixtures + 自有 PTY/differential harness | 覆盖行为而不只函数分支 [R38][R39] |

依赖版本在实施时锁定、审查并进入 lockfile，不复制文档页面中漂移的 `latest` 或 `version = "*"`。核心程序只启用所需 feature；AI/provider 和 optional adapters 不在普通启动路径做初始化。

### 31.2 `redis-rs` 的验收门槛

必须验证：是否完整保留需要的 RESP frame/type、大小和复制行为、push channel 预算、取消后读取行为、连接状态、重定向和内部重试。

官方库文档说明普通 bulk 结果的 iterator 仍涉及完整 vector，而 SCAN iterator 会继续发后续查询。[R27] 因此“用了 iterator”不能证明大响应已经流式。

**最终单一执行内核不能有两套暗中不同的命令语义。** 如库不能满足增量解析和状态控制，选择受测扩展/补丁或自建最小协议层，仍通过同一 kernel；`redis-rs` 可作为互操作测试对象，不是永远保留两套难以对齐的产品路径。

### 31.3 Workspace

```text
penguin-redis/
  Cargo.toml
  Cargo.lock
  crates/
    prc/                         # args、启动、退出码
    pr-application/              # CLI/TUI/MCP 共用 use cases
    pr-core/                     # request、result、session、budget
    pr-protocol/                 # RESP codec、frame events
    pr-transport/                # TCP/TLS/socket，受控 SSH
    pr-routing/                  # Cluster/Sentinel/target trust
    pr-catalog/                  # command metadata、版本能力、compiled grammar
    pr-intelligence/             # suggest、signature、diagnostics、find/guide
    pr-terminal/                 # 统一 event/focus/paint coordination
    pr-security/                 # policy、approval、redaction
    pr-profiles/                 # connection resolver、secret refs
    pr-results/                  # bounded store、spool、provenance
    pr-semantic/                 # command-result adapters
    pr-json/                     # 无损 token-preserving JSON DOM、occurrence 路径、格式化（§8.3）
    pr-render/                   # table/text/json/tree/hex
    pr-repl/                     # editor、history、completion
    pr-tui/                      # pages、navigation、virtualization
    pr-ops/                      # diff、diagnostics、import/export
    pr-automation/               # recipes、plans、execution reports
    pr-mcp/                      # optional stdio adapter
  compatibility/
  fixtures/
    protocol/
    renderer/
    redis/
    hostile-input/
    assistance/                  # input/candidate/zero-network fixtures
    catalog/                     # grammar/capability/version fixtures
  tests/
    differential/
    topology/
    pty/
    editor-state/
    assistance-network/
    fault/
    security/
    soak/
  benches/
  docs/
    adr/
    command-reference/
    threat-model/
    runbooks/
  xtask/
```

crate 边界服务于依赖隔离，不机械地每个文件一个 crate；实施时可合并很薄的模块，但不得反转依赖，让 core 依赖 terminal 或 AI。

### 31.4 接口与模块准则

Renderer 接收不可变 result handle；不接收网络 client。Policy 不能只在 CLI args 层执行；所有入口都经过 kernel。SecretProvider 不向 renderer 暴露真实秘密。Session 持有状态，request 保留 provenance，持续任务必须注册进 task supervisor。

`--help`、`--version`、`--demo`、`--learn`、`--list` 在不需要时不创建 runtime 网络任务，不解锁 credential store。用户取消后，`TaskScope`（§31.1「任务监督」）负责回收子任务：每个会话、订阅、discovery、tunnel 与 TUI 页面在自己的 `TaskScope` 中 spawn；scope drop 时先 `cancel()` 令牌、等待有界时间、再 `abort_all`；`:tasks` 显示各 scope 的活动任务数；soak 与 PERF-03 测试断言任务计数在每次操作后回到基线。绝不依赖“进程退出自然就好”掩盖长会话泄漏。

<a id="s26"></a>
## 32 · 完整测试体系：不是“写一些 unit tests”

### 32.1 测试层

| 层 | 重点 | 要求 |
|---|---|---|
| 单元测试 | parser、width、policy、TTL、format、target resolution | 每个边界与错误路径 |
| 属性测试 | 任意 bytes、RESP roundtrip、tokenizer、layout invariants | 随机生成和缩减失败样例 |
| Golden tests | table、header、row separators、JSON、mono、narrow | 版本化可审阅 snapshot |
| Differential | 与官方 CLI、原始 RESP、服务器状态比较 | 固定 baseline，多协议 |
| Integration | 真 Redis、ACL、TLS、Cluster、Sentinel | 非 mock 核心执行 |
| Fault injection | 断网、半包、丢响应、超时、DNS/TLS、磁盘满 | 结果未知与清理状态 |
| PTY E2E | 输入、补全、粘贴、resize、Ctrl+C、TUI 返回 | 真终端事件，不只组件 snapshot |
| Security | credential 泄漏、注入、profile trust、agent policy | 威胁模型每项有用例 |
| Performance | startup、RSS、吞吐、延迟、分配次数 | 固定硬件与发布构建 |
| Soak | 高频命令、连接切换、消息流、反复开关 TUI | 8h 基线、24h 发布周期场景 |
| Packaging | 安装、升级、回滚、路径、签名、卸载 | 干净机器与无网络场景 |

### 32.2 Differential 测试方法

测试同一个写命令时，官方和 Penguin 使用两个独立但相同初始状态的 Redis 环境。不能先在同一实例执行官方 `INCR` 再执行 Penguin `INCR`，然后比较结果并误判。

验证四层：请求 argv/字节、响应类型/顺序/字节、最终 Redis 状态、输出与 exit code。服务端生成时间、随机值和统计数据不做天真文本全等；使用有界 invariant 或固定 synthetic RESP 服务测试 renderer。

危险特殊模式只运行在隔离测试环境。Cluster 的拓扑变化用 deterministic 故障场景与日志定位，不靠偶然 failover 测试“有时成功”。

### 32.3 协议与安全 fuzz

畸形长度、超大声明、嵌套过深、非法 RESP type、截断 CRLF、UTF-8 跨块、binary NUL、终端 escape、畸形 JSON、重复属性、超长数字、压缩炸弹样本。

必须保证无 panic、无 OOM 式无界分配、无死循环、无逃逸终端控制、无越过预算的子进程/网络调用。失败样例加入长期 regression corpus。

### 32.4 终端回归

固定若干 terminal size 与颜色能力，并测试中文、emoji、组合字符、纯 ASCII、远程会话和 no-TTY。对输出进行 ANSI-aware 可见宽度验证；表框每行对齐、颜色 reset 正确、头部与字段行明显不同。

resize 发生在输入、JSON 展开、写入确认、订阅消息涌入时都要测。CLI 与 TUI 互相切换后输入缓冲、来源标记、审批状态不得串用。

### 32.5 完整覆盖不是一个百分比

覆盖率有参考价值，但关键路径要求每个状态迁移、每种失败窗口、每个 trust boundary 都有测试。mutation testing 用于 parser/policy 等重要模块；不能靠执行一遍函数就声称验证了行为。

所有严重 bug 修复先增加复现 fixture，再修实现；保留 minimized case。测试 flaky 需要定位，不能大量自动重跑直到绿灯掩盖失败。

### 32.6 智能输入属于同一发布门禁

[ASSIST 专项验收](#ci06)不另设更宽松质量线。生成 grammar 时做 prefix-closure/property 测试：完整合法命令的各个输入前缀不应导致 panic、错误跨参数插入或未经用户动作发请求。

把 typing、cursor move、paste、async response、profile switch、approval 和 resize 作为可组合事件序列，使用模型测试/故障注入验证 stale completion 与 focus 不变量。为最危险的“接受候选 -> 意外提交”建立专门 mutation test。

使用 no-network transport spy 测默认输入；获准远端 discovery 在隔离真实 Redis 上测请求与响应边界。§11.5 的两解析器等价性以 property test 覆盖（ASSIST-081）；bracketed paste 与无 bracketed paste 的时序启发式以 PTY 测试覆盖（ASSIST-082）；`--pipe` 的逐帧策略与 fail-closed 以畸形帧 corpus 与 prod-deny profile 覆盖（PIPE-08、SEC-09）；审批哈希以「同摘要不同字节」对照样例覆盖（SEC-10）。无法自动化验证的系统 IME/屏幕阅读器场景进入发布平台人工记录，不能只写“支持 Unicode”代替验收。

<a id="s27"></a>
## 33 · 必须通过的验收案例

以下可直接转成测试 issue；每项都必须绑定 automated test 或明确的 manual verification record。

| ID | 场景 | 通过标准 |
|---|---|---|
| UX-01 | 首次运行无 profile | 本机向导可用；没有网络扫描 |
| UX-02 | 保存 URI 后重开 | `prc @dev` 复用凭证，不重新要求复制 |
| UX-03 | Hash 7 fields | 精确 7 行逻辑记录，所有行之间有横线 |
| UX-04 | 表头关闭颜色 | 双线与加粗/结构仍区别于数据 |
| UX-05 | JSON 在 cell 中 | 缩进和边框不串行，原字段只占一个逻辑 row |
| UX-06 | dark/light/16-colour | 内容可辨认、表头不淹没文字 |
| UX-07 | 窄终端 | 无未经提示的隐藏列/内容；明确 wrap/降级 |
| UX-08 | 中英、emoji、组合字符 | 可见列宽正确，框线对齐 |
| UX-09 | `:inspect --field config` | 命中保留结果时零新增 Redis 请求 |
| UX-10 | 当前结果已淘汰 | 说明未保留，不隐式重读 |
| UX-11 | 多行粘贴 | 默认不逐行立即执行 |
| UX-12 | 从 TUI 返回 REPL | 原输入、身份、DB 与来源正确 |
| CMD-01 | `SET k '--raw'` | `--raw` 原样作为 value |
| CMD-02 | 大小写命令与混合大小写 key | 只识别命令，不修改 key/value |
| CMD-03 | `MGET k k missing` | 保留重复、顺序、nil |
| CMD-04 | `HMGET` 缺字段和空字段值 | nil 与 empty string 明确区分 |
| CMD-05 | `HSET` 覆盖返回 0 | 不显示失败，不捏造修改数 |
| CMD-06 | `ZRANGE` 默认/REV/BYSCORE | 顺序准确，不伪造全局 rank |
| CMD-07 | 原始 `DEL 'x:*'` | 只处理字面 key，不模式展开 |
| CMD-08 | `HGETALL` | 不隐式转成 HSCAN/增加 TTL 查询 |
| CMD-09 | SCAN 空结果非零 cursor | 不宣布扫描完成 |
| CMD-10 | 未知合法命令 | 按策略走 generic 请求/回复，不被过时语法拦截 |
| CMD-11 | Streams 重复 field | 全部保留，pivot 不覆盖 |
| CMD-12 | 模块命令不同结果版本 | 正确 fallback，无 panic、无重发 |
| DATA-01 | 非 UTF-8 与 NUL | 无替换字符损失；bytes 可往返 |
| DATA-02 | `"000123"`、`"false"` | 类型与原文不变化 |
| DATA-03 | 大 JSON number | 原词法和精度保留 |
| DATA-04 | 重复 JSON key | 完整保留并提示 |
| DATA-05 | 超长单行 JSON | 布局预算生效，不一次生成无限行 |
| DATA-06 | raw 与 pretty copy | raw 不含表框/注解，pretty 明确转换 |
| DATA-07 | key TTL 与 field TTL | 独立展示与保留策略 |
| PIPE-01 | stdout 被重定向 | 默认无 ANSI、无 banner |
| PIPE-02 | `--json -2` / `--json -3` | 输出结构按基线协议，无擅自 map 化 |
| PIPE-03 | `--bytes GET` | 无额外尾部换行；非 blob 报错 |
| PIPE-04 | `-x` binary import | 输入逐字节一致 |
| PIPE-05 | `--pipe` 一项失败 | 完成计数与退出状态正确 |
| PIPE-06 | 下游提前退出 | 正常处理 broken pipe，不后台持续跑 |
| PIPE-07 | JSON 输出中途磁盘满 | 非零状态，不能宣称 complete |
| STATE-01 | 写入完成但回复丢失 | 标 unknown，不自动重复写 |
| STATE-02 | 未发送前取消 | NotSent，不标执行成功 |
| STATE-03 | MULTI/QUEUED | queued 不显示为已应用 |
| STATE-04 | EXEC 内含错误 | 每项对应原命令，保留部分错误 |
| STATE-05 | WATCH 后变化 | 显示冲突，编辑不覆盖 |
| STATE-06 | 事务中切 profile | 不静默切换/重建事务 |
| STATE-07 | 阻塞命令取消 | 专属 session 清理，主输入可恢复 |
| STATE-08 | Pub/Sub 断线重连 | 明确 gap，不声称消息已补齐 |
| NET-01 | TLS 错名/过期/未知 CA | 拒绝或显式配置；无静默 insecure |
| NET-02 | profile endpoint 被覆盖 | 原凭证不自动发往新目标 |
| NET-03 | Cluster ASK | ASKING 与 command 同连接 |
| NET-04 | Cluster CROSSSLOT | 原命令不自动拆分 |
| NET-05 | Cluster 部分节点不可达 | 标 partial，不写全库健康/完整 |
| NET-06 | Sentinel failover | 重新发现，未知写不重放 |
| NET-07 | SSH 退出 | 自有资源清理，不破坏用户其他连接 |
| SEC-01 | AUTH/URI parse error | history、stderr、trace 均无秘密 |
| SEC-02 | 生产 deny 命令 | 无确认字符串绕过 deny |
| SEC-03 | 过期或被修改的计划 | 原批准失效 |
| SEC-04 | 不可信 profile/provider | 不自动执行外部程序 |
| SEC-05 | value 含 OSC52/CSI | pretty 不触发终端行为 |
| SEC-06 | 值中包含 AI 指令 | 当数据处理，不能提升权限 |
| SEC-07 | renderer/plugin panic | fallback；不重复执行原命令 |
| SEC-08 | clipboard/export 敏感值 | 依据显式策略，无隐式外发 |
| PERF-01 | Synthetic RESP 1 GiB blob + 真 Redis 上限样本 | 峰值 RSS 与块预算相符；明确区分两类来源 |
| PERF-02 | 高速订阅 | 缓冲有界，淘汰计数真实 |
| PERF-03 | 反复开关 TUI 1,000 次 | 无持续 session/task/内存增长 |
| PERF-04 | 8h/24h 会话 | 稳态资源，无线性泄漏 |
| LIFE-01 | 正常退出 | 无自有 daemon、监听和遗留任务 |
| LIFE-02 | crash 后重开 | 不破坏有效配置；安全清理旧 spool |
| LIFE-03 | 多终端同时改 profile | 无丢更新、无凭证误绑定 |
| LIFE-04 | 升级中途失败 | 可回滚，配置可恢复 |
| SEC-09 | `--pipe` 含写帧的 prod profile | 在第一条写帧处停止，退出码 5，之前帧的 outcome 完整；无静默跳过 |
| SEC-10 | 两条不同 argv 但显示摘要相同 | 一个审批令牌只能执行其中一条；另一条被拒绝（退出码 5） |
| SEC-11 | 服务器 `COMMAND` 声称写命令为 readonly | 本地 effects/策略不变；UI 标 `server reports` |
| PIPE-08 | `--pipe` 畸形/截断帧 | fail-closed，退出码 2，报告帧序号与偏移；后续帧未发送 |
| JSON-05 | `{"a":1,"a":2}` 编辑 | 不带 occurrence 的路径被拒绝；`$.a#2` 只修改第二个成员的 span |
| WIN-01 | Windows ConPTY 下 REPL/TUI | resize、颜色、alternate screen、退出恢复与 Unix PTY 同一套 PTY 测试通过 |
| WIN-02 | Windows `-x` / `--bytes` / `--pipe` | stdio 以二进制模式打开；`\r\n` 不被转换；bytes 往返一致 |
| WIN-03 | Windows Credential Manager + 配置锁 | 凭证存取、`LockFileEx` 并发写、ACL 仅当前用户 三项通过 |

<a id="s28"></a>
## 34 · 基准、质量门禁与发布证据

### 34.1 Benchmark 必须可复现

记录 hardware/OS、构建 commit、release flags、依赖锁、terminal、TLS、RESP、数据集、是否热缓存、输出 sink、并发和采样次数。结果包括 p50/p95/p99、峰值 RSS、CPU time、请求数、响应 bytes 和 allocation 数据。

网络读取与表格渲染分开 benchmark。对比官方 raw 时 Penguin 也用 raw；比较 pretty 的代价时分别报告可读性相关工作，不把不同工作量混在一起。

数据集覆盖：小 GET、Hash 小字段/80 字段/巨大字段、嵌套 JSON、1 MiB/64 MiB/实际 Redis 上限 blob，及 synthetic 1 GiB blob、million-item aggregate、binary、不同语言、订阅风暴和 Cluster 多节点。

### 34.2 CI 门禁

| 门禁 | 阻断条件 |
|---|---|
| Build / lint | 编译、格式、严格 lint 或受支持平台 build 失败 |
| Correctness | 关键 fixture、differential、协议 invariant 失败 |
| Safety | 秘密泄漏、策略绕过、错误重试写入、终端注入 |
| Resources | OOM、无界缓冲、显著且无解释的性能退化 |
| Terminal | 关键尺寸输出错位、颜色泄漏、退出后终端损坏 |
| Compatibility | manifest 声称支持但测试未通过 |
| Packaging | 安装、签名校验、升级或迁移失败 |
| Documentation | 新增 flag/行为缺帮助、错误说明与示例 |
| Assistance | 误接受/误发送、跨身份候选、隐藏网络请求、过期 span、键击阻塞 |
| Catalog | 上游 grammar/overlay 冲突、说明单位错误、能力/权限状态混淆 |

某个 OS 的未完成测试不能只在文字上勾选。允许部分平台先发布，但 release 明确支持范围；最终跨平台目标不因此取消。

### 34.3 Release evidence bundle

每次发布附机器可读支持矩阵、benchmark 报告、已知差异、故障注入摘要、威胁模型变更、SBOM/依赖清单、签名/校验说明和配置迁移说明。

性能门槛是版本化预算；阈值调整必须带证据和审批记录。不能为了绿灯只把 RSS 上限改到更大而不解释原因。

<a id="s29"></a>
## 35 · 安装、升级、卸载与生命周期

### 35.1 发布形态

macOS Apple Silicon/x86_64、Linux 主要目标和 Windows 原生可执行文件；按平台验证功能。提供可校验下载与 package-manager 分发，具体 tap/仓库/命令在真实发布后确定。

**本计划不宣称 `brew install penguin-redis` 目前已经可用。** 未来 package 名可较长，但日常可执行文件保持 `prc`。

默认安装不要求 Redis server、Python、Node.js、Docker、GUI runtime 或 Penguin Studio。操作系统本身需要的系统库和钥匙串属于平台能力；不要宣传未经各平台验证的“绝对零动态依赖”。

**Windows 的具体契约（v2.1 明确，避免「纸面支持」）：**

- 终端：Windows Terminal / ConPTY 为一级目标；legacy conhost 为 plain 模式降级目标。所有 PTY 测试（§32.1）在 ConPTY 下以同一套 fixture 运行（WIN-01）。
- stdio：`-x`、`-X`、`--bytes`、`--pipe`、`--output resp` 与所有非 TTY 输出路径在 Windows 上以二进制模式打开 stdin/stdout，禁止 CRLF 转换（WIN-02）。
- 凭证：Windows Credential Manager 通过 `keyring` 的 `windows-native` feature；条目按 `TrustIdentity` 命名；headless 服务账户下无 UI 时 fail-closed。
- 配置锁：`LockFileEx` 实现 §30.2 / §12.10 的 advisory lock；原子替换用 `MoveFileEx(MOVEFILE_REPLACE_EXISTING)`；文件 ACL 限制为当前用户（WIN-03）。
- 路径：配置目录用 `%APPDATA%\penguin-redis`，spool 用 `%LOCALAPPDATA%`；`prc --paths` 显示。
- 信号：Ctrl+C 通过 `SetConsoleCtrlHandler`；Ctrl+Break 等价于两次 Ctrl+C；无 SIGPIPE，broken pipe 由写错误码判定。
- 支持矩阵按功能而不是按 OS 打勾：TUI、TLS、SSH（依赖系统 OpenSSH）、Credential Manager、bracketed paste 各自登记 verified 状态。

### 35.2 生命周期契约

安装只部署文件与用户明确允许的 shell completion；不创建 launch agent、system service、cron、登录启动项或遥测进程。

启动按任务取配置和凭证；退出释放自己拥有的 session、subscription、tunnel、临时文件与任务。操作系统文件缓存不等于常驻应用。

### 35.3 升级

自动检查默认关闭或仅在用户明确配置后按需进行，不能每次连接 Redis 都额外访问发布服务器。更新不在当前编辑/事务中途替换运行文件；下一次启动使用新版本。

迁移支持备份、schema compatibility 与回滚说明。新版本不能默认改变 production policy、history 隐私或已保存 TLS 校验设置。

### 35.4 卸载

删除 executable 与配置/钥匙串秘密是不同动作。默认卸载程序可保留用户连接；彻底清理必须列明将删的 profile、history、spool 与共享 credential 引用。

不能删除与 Penguin 无关的 Keychain 条目，也不能在用户未确认时把复用凭证连带删除。

### 35.5 离线目录、说明与供应链

发布包包含命令目录、必要 locale 资源和演示 fixtures；首次查看帮助不触发下载。较大的可选知识包只在用户明确安装后使用，普通命令不强制联网补资源。

命令来源/文档/示例/依赖记录版本、出处与再分发许可审核状态，提供 notices 和 SBOM；本蓝图不替代实际授权审查。签名验证、rollback、schema migration 与 catalog regression 一起验证。

**catalog 再分发许可是 E0 交付物（v2.1 明确）：** Redis 的源码许可在 7.4 改为 RSALv2/SSPL、8.0 起加入 AGPLv3；其 `commands/*.json` 与文档站内容的许可需单独确认；Valkey 为 BSD-3。`pr-catalog` 的每个 upstream snapshot 必须登记来源仓库、commit、许可与「允许打包进二进制 / 仅允许运行时由用户下载 / 需自行编写」三种结论之一；不允许在结论为空时发布。若某来源不允许再分发，则该部分 catalog 由本仓按公开命令行为**独立撰写**（不复制文本），并以 fixture 对照真实服务器验证。`prc` 自身的许可由 ADR-023 决定并在 E0 记录。

No daemon 契约也适用于 completion helper：按需启动的 shell helper 必须及时退出，不为了跨进程缓存留下本地服务。卸载/隐私清理分别处理 catalog、history、schema、recipe 与系统凭证引用。

<a id="s30"></a>
## 36 · 分阶段交付：有完整终点，不设缩水终点

阶段按工程依赖，而不是按“容易/困难”决定保留什么。每个阶段都包含测试、文档与安全边界；不是先堆功能最后才补测试。

| 阶段 / Epic | 交付内容 | 完成门槛 |
|---|---|---|
| E0 · Contracts | 参数、profile、result、policy、协议/输出契约、威胁模型、fixture harness | ADR 冻结；关键语义有测试设计 |
| E1 · Native kernel | byte request、RESP2/3、session、bounded decode、TLS、outcomes | 不重复写；binary/partial reply/fuzz 通过 |
| E2 · Daily CLI + Assistance | `prc @dev`、Keychain、Hash/JSON table、colour、自动建议、签名、F1/F2、history、one-shot/raw/JSON | 用户主路径含“记不住命令”；**E2 必过 ASSIST 子集**：001–008、016、017、020–028、029、037、047、048、062–064、066、076、077、080、081、082、087；脚本测试（PIPE-01~07）通过；SPIKE-001 结论已落地 |
| E2A · Deep intelligence | grammar/overlay、key/field scope、Guide、错误说明、offline Learn、显式 discovery | 80 项 ASSIST 案例逐项有证据；无误发送/跨目标泄漏 |
| E3 · Production topology | Cluster/Sentinel、target trust、SSH、ACL/approvals、取消恢复 | 拓扑与故障注入通过；未知结果可见 |
| E4 · Deep TUI | browser、inspector、editor、diff、virtualization、任务面板、同等智能命令框 | 与 CLI 共用执行与 intelligence；focus/PTY/soak/编辑冲突通过 |
| E5 · Operational suite | diagnostics、Streams、Pub/Sub、archive、bulk、Lua、官方特殊模式 | 每项兼容与危险操作测试通过 |
| E6 · Automation | recipes、signed/hashed plans、structured reports、可选 MCP/AI | 所有入口同一 policy；注入与权限测试通过 |
| E7 · Product hardening | 全平台分发、文档、支持矩阵、benchmark、升级/回滚 | 发布证据完整，无未知 P0 安全/数据错误 |
| E8 · Studio integration | core SDK、schema migration、adapter 与共享 fixtures | Studio 未安装时 CLI 全功能不退化 |

### 36.1 依赖关系

```text
E0 -> E1 -> E2 -> E2A
       |      |
       +-> E3 +-> E4
                \   /
                 E5 -> E6
E0...E6 ----------------> E7
Shared core ------------> E8
```

E7 的工作从 E0 就开始积累，表格表示的是最终验收节点，不是最后才考虑发布质量。

### 36.2 每个 Epic 的交付包

设计决策、代码、单元/集成测试、失败行为、截图或 terminal transcript、资源预算、帮助与使用样例、支持矩阵更新、已知差异。

一个 feature 没有失败流程和测试，不算完成。某个版本临时未交付时保留 issue 和明确状态，不把它从最终蓝图删掉。

### 36.3 可并行的模块

table renderer/fixtures、credential/profile layer、RESP/kernel、PTY harness 可以在接口冻结后并行；但禁止各自另起 Redis client 并形成多套 execution semantics。

后续开发者或 coding agent 必须从本文件的硬约束和 ADR 开始，不得因为开发上下文有限擅自去掉安全策略、binary preservation、大响应预算或可选 TUI。

### 36.4 功能完成状态，不准用一行“支持”蒙混

每条最终能力记录 `designed -> implemented -> automated-tested -> platform-verified -> released`。每个命令也分别记录 execution/render/assistance/guide 的状态；一个带 tooltip 的假页面不能进入 released。

E2A 的深度 grammar、guide 与资源设计从 E0 开始，不能先写死少数命令再长期留下两个解析器。优先发布纵向链路是为了尽早验证，而不是降低最终覆盖范围。

产品完成定义包含三类角色测试：不记得命令的日常用户、熟悉 Redis 的高级 CLI 用户、使用脚本的自动化用户。不能为了帮新手而让专家每条普通读命令都确认两次，也不能为了专家快捷而误执行建议。

<a id="s31"></a>
## 37 · 关键 ADR 与立即开始的实施顺序

### 37.1 必须建立的 ADR

| ADR | 决策 |
|---|---|
| ADR-001 | 产品 `Penguin Redis`，executable `prc`，profile 前缀 `@` |
| ADR-002 | 原生命令语义优先；本地增强使用 `:` |
| ADR-003 | no required backend/daemon；Core 为 library |
| ADR-004 | Table-first、行横线、强化表头、cell 内 JSON |
| ADR-005 | 原始 bytes 权威；view 不改变类型与顺序 |
| ADR-006 | 原命令不隐式附带元数据请求、不自动换算法 |
| ADR-007 | Unknown-after-send 是正式 outcome，不盲重试写 |
| ADR-008 | CLI/TUI/MCP/recipe 同一 kernel/policy |
| ADR-009 | 有界解码与保留；spool 显式授权 |
| ADR-010 | profile secret reference 与 endpoint trust 绑定 |
| ADR-011 | 明确 baseline 的兼容矩阵，不做无证据全称承诺 |
| ADR-012 | Result Context 区分本地 inspect 与重新查询 |
| ADR-013 | raw、JSON、typed、bytes 输出独立契约 |
| ADR-014 | 完整测试与发布证据属于每个阶段 |
| ADR-015 | CLI/TUI 自动建议共用本地 Command Intelligence；模型不是依赖 |
| ADR-016 | Completion acceptance 只编辑；focus 与提交分离 |
| ADR-017 | 动态候选按 target/identity/DB/epoch 隔离，revision 防迟到污染 |
| ADR-018 | Grammar/catalog 不等于 server capability，也不等于 ACL 权限 |
| ADR-019 | Field discovery 避免隐式 value 抓取，旧版本明确降级 |
| ADR-020 | Find/Guide/Learn 零默认联网，模板不自动执行 |
| ADR-021 | 传输结果、服务器错误、副作用确定性和显示状态分开 |
| ADR-022 | 单一 terminal coordinator，shell 与 REPL completion 分别适配 |
| ADR-023 | `prc` 自身许可与 catalog 再分发结论（E0 记录） |
| ADR-024 | 审批令牌绑定 canonical argv 字节哈希与完整 `TrustIdentity`；摘要仅用于显示 |
| ADR-025 | `--pipe` 逐帧解析并经过同一 policy；agent 默认不可用 |
| ADR-026 | Reedline 仅作编辑状态引擎；终端所有权归 pr-terminal；SPIKE-001 决定回退 |
| ADR-027 | 普通编辑器为 UTF-8 文本；二进制参数只经 §3.2 定义的入口 |
| ADR-028 | `pr-json` 为服务器业务 JSON 的唯一权威模型；重复成员以 occurrence 寻址 |
| ADR-029 | 所有预算为进程级；不做跨进程仲裁 |
| ADR-030 | 本地 catalog/overlay 是 effects 与策略的唯一权威；远端 metadata 只补充 availability/docs |

### 37.2 第一条可演示的纵向链路

不要先写一堆漂亮 TUI 页面。第一条链路必须是：

```text
prc --add dev
  -> hidden URI import
  -> secure credential store
  -> persisted profile
  -> prc @dev
  -> type HG and see local dropdown
  -> choose HGETALL, see key argument help
  -> select known key or type it explicitly (no hidden scan)
  -> real HGETALL with original argv
  -> lossless response model
  -> full separated colour table
  -> JSON inside value cell
  -> :inspect --field config without refetch
  -> :copy raw value
  -> HGET field suggestions from this retained response
  -> F2 task search / F1 docs / guide insert without execution
  -> TUI command editor with identical assistance semantics
  -> raw/JSON one-shot output
  -> clean exit without own daemon
```

这条链路就包含产品真正的核心价值，也能验证架构是否支撑后续能力。用 fixture 演示 renderer 可以并行进行，但不能把静态漂亮图当成已验证的数据流。

### 37.3 最先验证的高风险 spike

验证库是否真正支持有界读取；验证 REPL/TUI 共用终端事件是否会乱序；验证 macOS credential 访问与分发签名变化；验证 Cluster 重定向与 target trust；验证 giant JSON 的词法保真与显示宽度；验证 automatic IdeMenu、async candidate revision、CLI/TUI focus 与 catalog grammar 编译。

每个 spike 输出采用/拒绝结论、具体反例、benchmark 和测试 fixture。不以“某库很流行”替代本项目的需求验收。

**SPIKE-001（Reedline 编辑引擎）** 的采用标准见 §14.5；**SPIKE-002（redis-rs 低层）** 的采用标准见 §31.2，任一不满足即以 `pr-protocol` 自研 codec 为唯一 kernel 边界；**SPIKE-003（Windows ConPTY）** 验证 §35.1 全部 Windows 契约在 CI 可自动化；**SPIKE-004（pr-json）** 验证 occurrence DOM 在 16 MiB 单值上的解析/格式化时间与内存符合 §24.3。四个 spike 都是 E0/E1 门禁，结论进入 ADR。

### 37.4 第一批 issue

`CORE-001` 请求/结果/outcome schema；`PROTO-001` RESP chunk decoder fixture；`CONN-001` profile + secret refs；`CLI-001` 命令边界 parser；`RENDER-001` Full Hash table；`JSON-001` token-preserving cell；`TEST-001` 官方差异 harness；`SEC-001` secret-path threat model；`PERF-001` 启动与大 blob baseline。

这些是依赖顺序，不代表其他最终能力被砍掉。

### 37.5 智能输入新增首批 issue 与依赖

| Issue | 交付内容 | 依赖/完成证据 |
|---|---|---|
| INTEL-001 | CommandSpec、catalog compiler、schema adapter | 固定 upstream fixture、版本差异测试 |
| INTEL-002 | Partial parser、cursor spans、signature | grammar/property/UTF-8 fixture |
| INTEL-003 | Context providers、scope 与 privacy | ASSIST 目标隔离/0-network tests |
| INTEL-004 | Deterministic ranker、descriptions、help | 中英任务与排序 golden corpus |
| INTEL-005 | Async broker、cancellation、revision/epoch | 乱序结果、超时、切 profile 测试 |
| INTEL-006 | Guide、snippet、unit helper | 不发送、quoting、缺槽位测试 |
| TERM-001 | 单 owner terminal/focus coordinator | REPL/TUI/menu/stream PTY tests |
| TERM-002 | 自动下拉、signature、docs panel | 自动/手动/无色/窄屏回归 |
| SEC-002 | Discovery 和 candidate trust boundary | 受限 ACL、敏感 field、恶意 metadata |
| PERF-002 | Typing latency、assistance RSS、soak | 与提示关闭状态的可复现差异 |
| SEC-003 | `--pipe` 逐帧 policy、fail-closed、plan-from-pipe | 畸形帧 corpus；prod/agent deny fixture |
| SEC-004 | `ApprovalToken` 字节哈希绑定 | 「同摘要不同字节」对照样例；epoch 失效测试 |
| SEC-005 | `TrustIdentity` 与凭证/审批绑定、Cluster 节点准入 | failover、证书轮换、边界外重定向 fixture |
| JSON-002 | `pr-json` occurrence DOM、路径解析、增量重写 | 重复成员、大数、词法保留 property test |
| TERM-003 | bracketed paste 与 paste-staging | PTY 有/无 bracketed paste 两组 |
| TERM-004 | Unicode 宽度策略与光标探测 | 固定终端仿真 + ambiguous/ZWJ fixture |
| WIN-001 | ConPTY harness、二进制 stdio、Credential Manager、LockFileEx | SPIKE-003 产出 |
| CAT-001 | catalog 许可审查与 Redis/Valkey 双分支 | ADR-023；每 snapshot 的许可结论 |

**给实现者的边界：** 不能为了快速演示把密码存明文、把所有 key 提前扫描进 Vec、用 LLM 每次生成命令、把 Enter 绑定成“接受并执行”、让 TUI 自己连 Redis，或删除既有测试/最终能力。单条功能未达成时报告真实状态，而不是悄悄删需求。

<a id="s32"></a>
## 38 · 最终完成标准与不接受的退化

### 38.1 可以称为完整主力产品的证据

用户能够保存多个真实连接、无需重复复制凭证；使用原始 Redis commands；默认得到彩色、字段分隔、表头明确的表格与单元格内 JSON；可选择 TUI 深入检查，不需要退出重登；CLI 与 TUI 输入时都有上下文下拉、参数说明、用途查找和 Guide；脚本仍稳定运行。

复杂场景下，命令结果可信、写入不会因断线被无声重复、Cluster/事务/Streams 状态正确、大响应有明确预算和保留范围、生产策略一致、秘密不会进入默认日志/历史。

最终支持范围内的高级命令与特殊模式有测试证据；发布与升级无需 backend 常驻；core 可被 Penguin Studio 直接复用。

### 38.2 永远不接受

| 不接受的退化 | 原因 |
|---|---|
| 每次输入长产品名 | 违背高频 UX |
| 每次复制连接密码 | 违背核心连接需求 |
| 为了 CLI 先开 backend | 违背独立原生形态 |
| 把 JSON 挤成一行或全部省略 | 违背第一使用痛点 |
| field 之间没有横线、header 跟数据一样 | 违背已经确定的视觉规格 |
| 原始数据被格式化“改好看” | 破坏可信性 |
| 类型未知就强行猜 schema | 可能丢数据 |
| 超限静默截断并返回成功 | 误导脚本与用户 |
| 重连后重复写、重建事务 | 可能造成真实数据错误 |
| 所有 profile 自动联网与解锁 | 隐私与资源不可控 |
| Agent 绕过人类策略 | 安全边界不一致 |
| Rust 等于一定更快 | 无证据性能承诺 |
| GUI 完成后弃用 CLI | 忽略 CLI 独立价值 |
| “先没有测试，以后补” | 将基础正确性变成技术债 |
| 只有固定命令名 Tab，没有参数与用途帮助 | 没有解决记不住命令的核心问题 |
| 选择建议直接执行 Redis | 混淆输入与授权 |
| 候选来自所有连接的混合历史 | 数据泄漏与误操作 |
| 补全依赖云 AI 或全库预扫描 | 把简单输入变成隐私/资源负担 |
| Unknown capability 被显示成 permitted | 文档与权限概念混淆 |
| `--pipe` 或任何批量入口绕过 catalog 分类与策略 | 生产保护形同虚设 |
| 审批绑定可碰撞的摘要而不是精确字节 | 批准的与执行的可能不是同一条命令 |
| 粘贴多行后自动逐行执行 | 用户审阅前发生写入 |
| 用 `serde_json::Value` 承载服务器业务 JSON | 重复成员与数字词法静默丢失 |
| 远端 metadata 能把本地写命令分类改成只读 | 客户端策略被服务器内容操纵 |

**最终产品原则：简单操作足够直接，复杂操作足够强；结构化输出更清晰，原始数据更可信；工具退出后，不留下自己的后台负担。**

---

<a id="appendix-a"></a>
## 附录 A · 拟定配置示例

以下 TOML 展示完整配置的逻辑 schema；实施时必须提供 schema validation。它不是某个现有软件已经支持的文件格式。

```toml
schema_version = 2

[app]
command = "prc"
default_entry = "connection-selector"
required_background_service = false
telemetry = "off"

[display]
mode = "auto"
color = "auto"
theme = "penguin-dark"
plain = false

[display.table]
style = "full"
row_separators = true
header_separator = "double"
header_weight = "bold"
header_background = true
wrap = true
field_alignment = "left-top"
value_alignment = "left-top"
narrow_layout = "stacked"

[display.json]
auto_detect = "object-or-array"
placement = "inside-cell"
indent = 2
syntax_highlight = true
preserve_member_order = true
preserve_numeric_lexemes = true
preserve_duplicate_members = true
large_value_behavior = "ask"

[results]
cache_budget_mib = 64
single_retained_result_mib = 16
auto_json_index_mib = 2
spool = "explicit-consent"
persist_read_results = false

[history]
store = "embedded-sqlite"
scope = "connection-identity-db"
persist_responses = false
persist_write_payloads = false
redact_sensitive_commands = true
redact_key_arguments = "by-environment"   # development: none · staging: hashed · production: all positional
retention_days = 90
retention_entries = 10000
file_mode = "0600"

[assistance]
mode = "assisted"
language = "zh-CN"
shared_cli_tui_engine = true
auto_popup = true
popup_debounce_ms = 75
visible_candidates = 6
signature_help = true
accept_executes = false
plain_terminal_mode = "manual-list"
model_required = false

[assistance.history]
ghost_text = "scoped-read-only"
include_write_payloads = false
cross_profile_suggestions = false

[assistance.discovery]
default = "explicit"
allow_keys_star = false
old_hash_scan_may_read_values = false
prefix_mode = "literal-escaped"           # 字面前缀转义 glob 字符；--match 走 raw glob（§12.4）
budget_scope = "per-process"              # 无 daemon，不跨进程聚合（§24.7）

[assistance.discovery.auto]
max_in_flight = 1
requests_per_second = 2
max_requests_per_trigger = 2
time_budget_ms = 1000
response_budget_kib = 1024
permission_error_cooldown_seconds = 60

[assistance.discovery.explicit]
max_in_flight = 1
requests_per_second = 20
max_requests_per_trigger = 50
scan_count_hint = 1000
time_budget_ms = 10000
response_budget_kib = 4096
require_confirmation_in_production = true

[assistance.cache]
heap_increment_limit_mib = 12             # 子项合计 10 + 余量 2（§12.5）
readonly_catalog_reported_separately = true
key_count_limit = 5000
key_names_mib = 2
field_names_mib = 2
metadata_hot_mib = 2
scratch_and_indexes_mib = 2
parser_state_and_pending_mib = 1
discovery_response_mib = 1
persist_discovered_names = false
scope = "profile-service-db-identity-policy-topology-epoch"

[assistance.find]
engine = "local-lexical-and-reviewed-synonyms"
locales = ["zh-CN", "en"]
network_required = false

[assistance.guide]
insert_only = true
block_unfilled_slots = true
show_exact_command = true

[credentials]
store = "system"
allow_plaintext_fallback = false
bind_to = "tls-identity+server-identity"   # 不绑定 endpoint 地址（§21.3）
journal = true                             # 跨资源提交 journal 与启动 reconciliation（§4.2）
headless_fallback = "fail-closed"          # askpass / 会话内秘密 / 已评审 vault

[display.unicode]
ambiguous_width = "auto"                   # narrow | wide | auto(按 locale)
zwj_sequence_width = 2
allow_cursor_probe = true                  # 仅 TTY 且用户触发

[input]
bracketed_paste = "required"               # 不支持时进入 paste-staging（§14.1）
paste_staging_on_multiline = true
binary_arguments = "escape-or-explicit"    # §3.2 入口之外不接受原始字节

[profiles.dev]
id = "4c8700c4-451e-4ef4-a6ec-14f7f4ebccf1"
mode = "standalone"
environment = "development"
host = "redis-dev.example.internal"
port = 6379
database = 3

[profiles.dev.tls]
enabled = true
verify_certificate = true
server_name = "redis-dev.example.internal"

[profiles.dev.auth]
username = "developer"
secret_ref = "credential:8d45b06c-4c61-4c97-957f-154632de78ea"

[profiles.dev.policy]
reads = "allow"
writes = "allow"
destructive = "confirm-when-allowed"
remote_completion = "explicit-bounded"

[profiles.prod]
id = "a05e955c-cb70-4b0a-a755-7b5c357f1e79"
mode = "cluster"
environment = "production"
seeds = ["redis-prod.example.internal:6379"]
database = 0

[profiles.prod.tls]
enabled = true
verify_certificate = true

[profiles.prod.auth]
username = "readonly-developer"
secret_ref = "credential:f0daa351-496a-4908-9c75-fd6535635ba1"

[profiles.prod.policy]
reads = "bounded"
writes = "deny"
destructive = "deny"
unknown_commands = "deny"
remote_completion = "off"
agent_access = "deny-until-configured"

[clipboard]
mode = "explicit"
osc52 = "off"

[ai]
enabled = false
send_values = "explicit-consent"

[mcp]
enabled_by_default = false
transport = "stdio"
```

Profile 的主机与 UUID 都是示例；没有真实凭证。保存的 secret reference 指向本机系统 store，不是可用于认证的字符串。

### A.1 主题文件

```toml
schema_version = 2
name = "penguin-dark"

[colors]
surface = "#111827"
text = "#E5E7EB"
muted = "#9CA3AF"
border = "#64748B"
title = "#67E8F9"
header_background = "#164E63"
header_foreground = "#ECFEFF"
json_key = "#7DD3FC"
json_string = "#A7F3D0"
json_number = "#FDE68A"
json_boolean = "#C4B5FD"
json_null = "#CBD5E1"
warning = "#FBBF24"
error = "#FCA5A5"
production = "#FDA4AF"

[styles]
header_bold = true
field_bold = false
header_separator = "double"
row_separator = "single"
```

主题文件不能包含 command、provider 或自动执行片段。所有颜色都是显示层选择，不写回 Redis。

<a id="appendix-b"></a>
## 附录 B · 本地开发测试种子与预期

**只对一次性、本地测试 Redis 使用这些写入种子。** 它们使用唯一前缀，不要求 FLUSHDB，不接触生产数据；`redis-cli` 是现有工具，`prc` 是拟开发程序。

```bash
redis-cli -h 127.0.0.1 -p 6379 HSET \
  'penguin:fixture:player:10001' \
  playerId '10001' \
  username 'shieng' \
  platformId '70' \
  status 'ACTIVE' \
  balance '12890.50' \
  currency 'MYR' \
  config '{"notification":{"enabled":true,"channels":["FCM","HUAWEI"]},"theme":"dark"}'

redis-cli -h 127.0.0.1 -p 6379 SET \
  'penguin:fixture:json' \
  '{"id":"000123","large":9007199254740993,"amount":1.2300,"enabled":true,"value":null}'

redis-cli -h 127.0.0.1 -p 6379 SET \
  'penguin:fixture:duplicate-json' '{"a":1,"a":2}'

redis-cli -h 127.0.0.1 -p 6379 HSET \
  'penguin:fixture:edge-cases' \
  empty '' \
  zero '0' \
  numeric-looking '000123' \
  boolean-looking 'false' \
  nil-looking '(nil)'

redis-cli -h 127.0.0.1 -p 6379 ZADD \
  'penguin:fixture:leaderboard' \
  92881 player:10001 88291 player:92831 81203 player:82192
```

预期：第一条 Hash 恰好 7 fields；JSON cell 独立多行；`large` 不丢精度、`amount` 原词法保留；重复 `a` 不丢失；empty 与 nil 区分；leaderboard 在普通 ZRANGE 中升序，在 REV 中降序。

二进制、复杂控制字符、分块边界和巨大 payload 由 fixture generator 产生，不建议在说明文档里直接嵌入不可见控制字节。所有 fuzz/hostile fixture 都标为 untrusted，不在普通 shell 中执行其内容。

### B.1 最终演示脚本应该覆盖的检查

```bash
# 以下 prc 命令仅在项目实现后运行。
prc -h 127.0.0.1 -p 6379 HGETALL penguin:fixture:player:10001
prc -h 127.0.0.1 -p 6379 GET penguin:fixture:json
prc -h 127.0.0.1 -p 6379 GET penguin:fixture:duplicate-json
prc -h 127.0.0.1 -p 6379 HGETALL penguin:fixture:edge-cases
prc -h 127.0.0.1 -p 6379 ZRANGE penguin:fixture:leaderboard 0 2 REV WITHSCORES
prc -h 127.0.0.1 -p 6379 --json HGETALL penguin:fixture:player:10001
prc --learn hash
prc --demo --theme penguin-dark
prc --demo --no-color
prc --demo --plain
```

### B.2 差异测试记录模板

```json
{
  "case_id": "CMD-05",
  "baseline_cli": "PIN_EXACT_TAG_DURING_IMPLEMENTATION",
  "server_image_digest": "PIN_EXACT_DIGEST_DURING_IMPLEMENTATION",
  "protocol": "RESP2",
  "initial_state_fixture": "hash-existing-field",
  "request": ["HSET", "fixture:hash", "status", "ACTIVE"],
  "expected_reply": { "type": "integer", "value": 0 },
  "expected_render_annotation": "New fields: 0",
  "execution_status": "NOT_RUN_IN_THIS_PLAN"
}
```

`NOT_RUN_IN_THIS_PLAN` 是刻意保留的真实性标记：本文件规划测试，不伪造产品实现或通过记录。

---

<a id="references"></a>
## 附录 C · 一手资料与事实核对

编制与本次重点核对日期：**2026-09-16**。保留前版的一手资料索引，并补充/核对智能输入、语法、远端发现、状态与资源边界来源。下面的来源用于确认 Redis/JSON/terminal/依赖的现有行为；本文产品选择、架构、预算、配色和交付计划是为 Penguin Redis 提出的设计。`latest` 文档可能变化，正式实施需固定版本并保存对应资料 snapshot。

| 编号 | 一手来源 | 主要用于核对 |
|---|---|---|
| [R01] | Redis CLI 官方文档 | 命令参数、raw/JSON、历史、stdin、特殊模式 |
| [R02] | Redis RESP 协议规范 | 二进制安全、协议类型、RESP2/3 |
| [R03] | HGETALL | field/value 返回结构 |
| [R04] | HSET | 返回新增字段数而非修改数 |
| [R05] | ZRANGE | 升序、REV、索引与边界 |
| [R06] | XRANGE | Stream 读取与结果结构 |
| [R07] | XADD | Stream field/value 输入模型 |
| [R08] | SCAN | cursor、COUNT 提示、重复与变化期间保证 |
| [R09] | HPTTL | Hash field 级 TTL |
| [R10] | Redis Cluster specification | 路由、重定向、slot、DB 与多 key 约束 |
| [R11] | Redis Transactions | WATCH、MULTI、EXEC、连接状态与错误 |
| [R12] | DEL | 指定 key 删除，不是自动模式展开 |
| [R13] | UNLINK | 异步释放对象内存的语义 |
| [R14] | GET | string 读取、缺失和类型约束 |
| [R15] | Redis ACL | 服务器命令/key/身份权限边界 |
| [R16] | Redis Sentinel | 发现与高可用机制 |
| [R17] | Redis Pub/Sub | 消息交付和断线语义 |
| [R18] | RESTORE | 序列化恢复、TTL 与兼容约束 |
| [R19] | CLIENT UNBLOCK | 专门的阻塞解除操作 |
| [R20] | COMMAND DOCS | 命令元数据与帮助 |
| [R21] | IRedis 官方仓库 | 已有终端工具的交互与连接能力 |
| [R22] | Redis Insight 官方文档 | 现有图形数据工具基线 |
| [R23] | Apple Platform Security：Keychain | 系统秘密存储与保护 |
| [R24] | NO_COLOR 约定 | 颜色禁用与显式配置关系 |
| [R25] | RFC 8259 | JSON 语法、重复成员与数字互操作 |
| [R26] | serde_json RawValue 文档 | 保留 JSON 原始文本片段 |
| [R27] | redis-rs 官方 crate 文档 | 高/低层 API、iterator、连接与协议能力 |
| [R28] | Reedline 官方 crate 文档 | REPL 输入、提示和补全基础 |
| [R29] | MCP transports，固定 2025-11-25 规范基线 | stdio 与协议输出边界 |
| [R30] | MCP Security Best Practices | 信任边界与工具安全 |
| [R31] | Ratatui 官方文档 | Rust TUI 能力 |
| [R32] | Crossterm 官方 crate 文档 | 终端控制与事件基础 |
| [R33] | Keyring 官方 crate 文档 | keyring-core 与具体 store 的选择 |
| [R34] | TTL | 无过期和 key 不存在的区别 |
| [R35] | MONITOR | 监控命令与性能警告 |
| [R36] | DUMP | 序列化值与 TTL 边界 |
| [R37] | Tokio 官方 crate 文档 | 异步运行时与能力选择 |
| [R38] | cargo-nextest 官方文档 | Rust 测试运行与报告 |
| [R39] | Proptest 官方文档 | 属性测试与失败样例缩减 |
| [R40] | Redis command arguments | oneof/block/repeat、参数类型与元数据 |
| [R41] | Command key specifications | 不完整 key specs 与随选项变化的访问标志 |
| [R42] | SET | 条件/过期选项、版本历史与 TTL 行为 |
| [R43] | ZADD | 条件选项与 score/member 模型 |
| [R44] | HSCAN | NOVALUES、字段扫描与返回形态 |
| [R45] | ACL DRYRUN | 权限模拟与管理权限边界 |
| [R46] | CLIENT REPLY | ON/OFF/SKIP 与无回复状态 |
| [R47] | Reedline IdeMenu | IDE-style 菜单、候选布局与替换行为 |
| [R48] | Reedline Completer | 位置/span/候选与可轮询更新机制 |
| [R49] | Reedline builder API | 重绘、外部消息与 completion 配置 |
| [R50] | IRedis 官方产品说明 | 已有基于输入/响应历史的自动补全 |
| [R51] | XREADGROUP | 消费组读取及其状态影响 |
| [R52] | Redis Strings | 默认字符串大小边界与二进制安全 |
| [R53] | HELLO | 协议切换与认证参数 |
| [R54] | COMMAND GETKEYSANDFLAGS | 完整命令 key 提取及访问标志 |

[R01]: https://redis.io/docs/latest/develop/tools/cli/ "Redis CLI"
[R02]: https://redis.io/docs/latest/develop/reference/protocol-spec/ "Redis serialization protocol specification"
[R03]: https://redis.io/docs/latest/commands/hgetall/ "HGETALL"
[R04]: https://redis.io/docs/latest/commands/hset/ "HSET"
[R05]: https://redis.io/docs/latest/commands/zrange/ "ZRANGE"
[R06]: https://redis.io/docs/latest/commands/xrange/ "XRANGE"
[R07]: https://redis.io/docs/latest/commands/xadd/ "XADD"
[R08]: https://redis.io/docs/latest/commands/scan/ "SCAN"
[R09]: https://redis.io/docs/latest/commands/hpttl/ "HPTTL"
[R10]: https://redis.io/docs/latest/operate/oss_and_stack/reference/cluster-spec/ "Redis cluster specification"
[R11]: https://redis.io/docs/latest/develop/using-commands/transactions/ "Redis transactions"
[R12]: https://redis.io/docs/latest/commands/del/ "DEL"
[R13]: https://redis.io/docs/latest/commands/unlink/ "UNLINK"
[R14]: https://redis.io/docs/latest/commands/get/ "GET"
[R15]: https://redis.io/docs/latest/operate/oss_and_stack/management/security/acl/ "Redis ACL"
[R16]: https://redis.io/docs/latest/operate/oss_and_stack/management/sentinel/ "Redis Sentinel"
[R17]: https://redis.io/docs/latest/develop/pubsub/ "Redis Pub/Sub"
[R18]: https://redis.io/docs/latest/commands/restore/ "RESTORE"
[R19]: https://redis.io/docs/latest/commands/client-unblock/ "CLIENT UNBLOCK"
[R20]: https://redis.io/docs/latest/commands/command-docs/ "COMMAND DOCS"
[R21]: https://github.com/laixintao/iredis "IRedis official repository"
[R22]: https://redis.io/docs/latest/develop/tools/insight/ "Redis Insight"
[R23]: https://support.apple.com/guide/security/keychain-data-protection-secb0694df1a/web "Apple Keychain data protection"
[R24]: https://no-color.org/ "NO_COLOR"
[R25]: https://www.rfc-editor.org/rfc/rfc8259 "RFC 8259: JSON"
[R26]: https://docs.rs/serde_json/latest/serde_json/value/struct.RawValue.html "serde_json RawValue"
[R27]: https://docs.rs/redis/latest/redis/ "redis-rs crate documentation"
[R28]: https://docs.rs/reedline/latest/reedline/ "Reedline"
[R29]: https://modelcontextprotocol.io/specification/2025-11-25/basic/transports "MCP transports 2025-11-25"
[R30]: https://modelcontextprotocol.io/docs/2026-07-28/tutorials/security/security_best_practices "MCP security best practices"
[R31]: https://ratatui.rs/ "Ratatui"
[R32]: https://docs.rs/crossterm/latest/crossterm/ "Crossterm"
[R33]: https://docs.rs/keyring/latest/keyring/ "Keyring crate documentation"
[R34]: https://redis.io/docs/latest/commands/ttl/ "TTL"
[R35]: https://redis.io/docs/latest/commands/monitor/ "MONITOR"
[R36]: https://redis.io/docs/latest/commands/dump/ "DUMP"
[R37]: https://docs.rs/tokio/latest/tokio/ "Tokio"
[R38]: https://nexte.st/ "cargo-nextest"
[R39]: https://proptest-rs.github.io/proptest/intro.html "Proptest"
[R40]: https://redis.io/docs/latest/develop/reference/command-arguments/ "Redis command arguments"
[R41]: https://redis.io/docs/latest/develop/reference/key-specs/ "Command key specifications"
[R42]: https://redis.io/docs/latest/commands/set/ "SET"
[R43]: https://redis.io/docs/latest/commands/zadd/ "ZADD"
[R44]: https://redis.io/docs/latest/commands/hscan/ "HSCAN"
[R45]: https://redis.io/docs/latest/commands/acl-dryrun/ "ACL DRYRUN"
[R46]: https://redis.io/docs/latest/commands/client-reply/ "CLIENT REPLY"
[R47]: https://docs.rs/reedline/latest/reedline/struct.IdeMenu.html "Reedline IdeMenu"
[R48]: https://docs.rs/reedline/latest/reedline/trait.Completer.html "Reedline Completer"
[R49]: https://docs.rs/reedline/latest/reedline/struct.Reedline.html "Reedline builder API"
[R50]: https://iredis.xbin.io/ "IRedis 官方产品说明"
[R51]: https://redis.io/docs/latest/commands/xreadgroup/ "XREADGROUP"
[R52]: https://redis.io/docs/latest/develop/data-types/strings/ "Redis Strings"
[R53]: https://redis.io/docs/latest/commands/hello/ "HELLO"
[R54]: https://redis.io/docs/latest/commands/command-getkeysandflags/ "COMMAND GETKEYSANDFLAGS"

---

**交付说明：本文是整合版最终产品与工程实施基线，包含前版完整能力和 Command Intelligence 设计，可独立交给开发者使用。没有执行产品实现、真实 Redis 写入、发布安装包或性能 benchmark；文中的目标必须由后续开发与测试兑现。**
