# Phase 0 报告（P0-META-01）

> 依据 `penguin-redis-phase-plan.md` §2.3 / §2.4。每个 V 项一行：状态 + 证据链接。  
> 合法终态只有 `PASS` 与 `FALLBACK-ADOPTED(ADR-xxx)`。**没有 DEFERRED / SKIPPED 列。**  
> 状态说明：`IN-PROGRESS` = 进行中；`BLOCKED(<原因>)` = 被外部条件阻塞（必须写明解阻条件，且不是终态）。

## 汇总

| 状态 | 数量 |
|---|---|
| PASS | 46 |
| FALLBACK-ADOPTED | 1 |
| IN-PROGRESS | 15 |
| BLOCKED | 0 |

## 环境记录

| 项 | 值 |
|---|---|
| 主机 | Apple M3 · 24 GB · macOS 26.5.2 (aarch64) |
| Rust | 1.97.1 stable（`rust-toolchain.toml` 固定） |
| Docker | 29.7.2 client / 29.5.2 daemon · linux/arm64 |
| 本机 Redis | 8.4.0（仅作对照，不进矩阵） |
| gh | 2.88.1 · 已登录 `shiengng12345` |
| 远端 | `https://github.com/shiengng12345/penguin-redis` |
| Windows / Linux runner | 本机无；通过 GitHub Actions `windows-latest` / `ubuntu-latest`（V-A05） |

## Track A · 基础设施与 harness

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-A01 | PASS | `xtask/pty-harness/` · 28 tests · portable-pty 0.9 |统一 Unix PTY + Windows ConPTY；send/resize/bracketed-paste/wait_for；录制可回放且 golden 确定性；**DSR 应答器**：`ConPTY` 启动发 `ESC[6n` 并阻塞等待终端回复，不应答则子进程输出一个字节都发不出（V-C02 第一次真跑 Windows 时暴露，此前 harness 的 Windows 路径全被 `#[ignore]`）；**屏幕模型** `Screen`：`ConPTY` 转发的是自身缓冲区的差量而非子进程写的字节，所以「等待连续子串」在 Windows 上不成立，测试改为断言渲染后的屏幕与 scrollback，`unhandled()` 暴露模型跳过的序列数 |
| V-A02 | IN-PROGRESS | — | |
| V-A03 | PASS | `xtask/resp-server/` · `fixtures/protocol/` (316 samples / 12 categories) · 15 tests | RESP2/3 encoder incl. streamed+push+attribute; scripted delivery; 1 GiB streamer; hostile corpus每类≥20 |
| V-A04 | PASS | `xtask/src/fault.rs` · 8 tests | TCP 代理：客户端/服务端定点切断、blackhole、慢速、分片、单字节损坏、拒连；FS：只读/填充 |
| V-A05 | PASS | CI run 35053565259 · `.github/workflows/ci.yml` · `ci/*.sh` | **17/17 全绿**：三平台 build+lint 与 test、5 个 digest-pinned server、cluster、sentinel、4 个 gate |
| V-A06 | PASS | `xtask/src/measure.rs` · 8 tests · `cargo run -p xtask -- measure-baseline` | 计数 allocator（live/peak/calls）+ RSS 采样 + 预算表；heap 与 RSS 分列不混计 |

## Track B · 协议与执行内核

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-B01 | IN-PROGRESS | — | |
| V-B02 | PASS | `crates/pr-protocol/` · 23 tests（含 316 样本 corpus 逐字节喂入） | 增量 RESP2/3 解码含 streamed；预算与协议错误分离；词法保真；bare-LF 即时报错 |
| V-B03 | PASS | `crates/pr-application/src/pipe.rs` · 14 tests | 逐帧解码→同一 catalog 分类→同一 policy；prod 遇写帧即停不跳过（SEC-09）；畸形/截断/非命令帧 fail-closed 并报帧号与字节偏移（PIPE-08）；agent 完全不可用；停止帧之后一帧都不放行 |
| V-B04 | PASS | `crates/pr-protocol/tests/outcome_matrix.rs` · 12 tests | 经真实 wire 字节驱动：截断回复/连接关闭/协议错误 → `UnknownAfterSend`；分片不改变结论；push 不占回复槽；全组合映射表 + 每个退出码可达性 |
| V-B05 | PASS | `crates/pr-core/src/session.rs` · 18 tests | HELLO/AUTH/SELECT/RESET/MULTI/WATCH/CLIENT REPLY 状态机；`may_probe()` 在事务内与 reply-suppressed 时拒绝注入；断线返回 `ReconnectLosses` 且不静默恢复 |
| V-B06 | PASS | `crates/pr-application/src/output.rs` · `tests/push_output.rs` 25 tests | push 路由由**模式**决定而非 flag：reply 模式丢弃并在 stderr 计数 `N push frames suppressed`；`--show-pushes` 转 stderr NDJSON（带 arrival seq）；`ndjson`/`typed-json` 按到达顺序进 stdout 且 `--show-pushes` 不重复投递。push 在回复前/后/前后夹/单独四种时序均不占回复槽位；带 attribute 的 push 仍识别为 push。`--output resp` 规范化再编码（三种 null → `_`、attribute 紧邻其值、bytes 无损往返），与 `--trace-wire` 原始 wire 分离且测试断言二者不同。输出 flag 冲突报错不做 last-wins；`--json` 的 RESP3 偏好不覆盖显式 `-2` |
| V-B07 | PASS | `crates/prc/src/args.rs` · 17 tests | §3.2 顶层语法；§3.3 profile×flag 矩阵逐格用例；`-h` 恒为 host；输出模式互斥不做 last-wins |

## Track C · 终端与编辑器

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-C01 | FALLBACK-ADOPTED(ADR-026) | `docs/spikes/SPIKE-001.md` · `crates/pr-repl/src/buffer.rs` · 20 tests | Reedline 不满足 (b)：undo 在私有 `Editor` 上；且 mid-codepoint 光标会在库内 panic。采用预命名回退：自有 grapheme LineBuffer + EditTransaction |
| V-C02 | PASS | `crates/pr-terminal/`（ownership/event/paint/coordinator + `pr-terminal-demo`）· `tests/owner.rs` 19 tests · `tests/pty_owner.rs` 7 PTY tests | 「无第二个 stdin 读者/并发 stdout 写者」做成资源而非约定：`Ownership` 进程级 token，第二个 `Coordinator::new` 返回 `AlreadyOwned`，`Painter` 必须同时持 token 与 `&mut Sink`；worker 只能 `Mailbox::post`，由 coordinator 决定何时 print-and-redraw。ASSIST-062：25/20 条 push 涌入与打字交错，buffer、光标、菜单焦点均不变，通知逐条进 scrollback 且从不落在 prompt 行内；UX-12/场景 H：F1 帮助不动命令框，Esc 先关帮助再退 TUI，草稿与来源 `@r2 / DB 0 / result #17` 原样带回且**不执行**；alternate screen 进/出各恰好一次，TUI 期间通知排队、回到 REPL 后按序补印 |
| V-C03 | PASS | `crates/pr-terminal/src/paste.rs` 18 tests · `tests/paste_matrix.rs` 10 tests · `tests/owner.rs` 6 tests · `tests/pty_owner.rs` 3 PTY tests · `docs/manual-verification/MV-V-C03-paste.md` | paste-staging：多行进审阅视图逐行显示、可上下移动/编辑/删除，三种选择（逐条 / 单条多行 / 取消），**Enter 在该视图中刻意无作用**——用户凭反射会按的那个键正是此刻必须什么都不做的键。单行直接进编辑缓冲区不提交。真实 PTY 上验证：粘贴 3 行含 `FLUSHALL`，删掉那行再选「逐条」，只有另两条被提交。无 bracketed paste 的 10 ms 时序启发式在 14 条定时 trace corpus 上 **0 漏检 / 误判 0 of 7（0%）**；覆盖瞬时、1ms、CRLF、纯 `\r`、10 行脚本、超长单行，以及正常打字、快速打字（15ms）、犹豫打字、按键重复、刚好 11ms 的边界。时序**无法**覆盖的情形（慢速中继的粘贴）用一个断言「它确实漏检」的测试写明，而不是从 corpus 里删掉 |；**Windows CI 抓到真缺口**：ConPTY 下 crossterm 不投递 `Event::Paste`，粘贴内容以普通按键（含 Enter）到达，三条命令全部自动执行——包括 `FLUSHALL`。时序启发式原先只是纯函数、从未接到实时输入路径。现已接入 `Coordinator::handle_at` / `tick`：burst 内完成的行被扣住不提交，安静 10 ms 后整体进审阅；单行快速输入退回编辑缓冲区不提交（代价是多按一次 Enter，不会有人跑到没读过的命令）
| V-C04 | PASS | `crates/pr-terminal/tests/width_matrix.rs` 12 tests · `crates/pr-terminal/src/probe.rs` 12 tests · `crates/pr-render/src/table.rs` `DrawMode::CursorReset` · `xtask/pty-harness/src/screen.rs` `CellWidth` · `docs/manual-verification/MV-V-C04-width.md` | 降级绘制不能靠「渲染完再读回自己的输出」证明——那只说明渲染器同意自己。测试改为**用一种宽度模型渲染、用另一种模型显示**：屏幕模型的 `CellWidth::UnicodeWide` 扮演 CJK 终端，narrow 策略渲染的 padded 表格右边框逐行漂移（先断言这个漂移真的发生，否则降级测试是空的），`CursorReset` 每个单元格边界发绝对光标移动，框线仍对齐。`CSI 6 n` 探测：仅 TTY、仅用户触发（`prc --probe-width`），非 TTY 直接拒绝且**一个字节都不写**，无应答 500 ms 放弃而非挂起，一处分歧即切换。过程中修正屏幕模型两个真实错误：组合字符被强制成 1 列（终端会合成到前一格，格子改存 grapheme cluster），以及 box-drawing 按 Ambiguous 判宽（真实终端特例化为窄，否则任何 TUI 都是坏的）。真实终端差异 → `MV-V-C04-*` 人工记录，编号已分配 |
| V-C05 | PASS | `crates/pr-repl/src/entries.rs` 9 tests · `crates/pr-terminal/tests/key_matrix.rs` 4 tests · `docs/manual-verification/MV-V-C05-keys.md` | 验收标准是「每个功能至少一条文字入口在所有组合可用」，不是「每个快捷键都能用」——后者不可能成立。§14.2 八行做成数据，测试断言：除「取消输入」（Ctrl+C 是唯一每个终端都送达的键）外每个功能都有文字入口、每条入口都能 parse、全是可直接键入的 ASCII、`Ctrl+Space` 不作任何默认键、重映射**不能**连带移除文字入口。裸 PTY 实测（macOS）：F1(SS3/CSI)、F2(SS3/CSI)、F3、F4、F5、F6、Ctrl+R/P/N **全部送达**；`Ctrl+Space`（NUL）**未送达**，与 §14.2 预判一致。终端 × 输入法（含 macOS F 键默认被系统占用的两种设置）→ `MV-V-C05-*` 人工记录，编号已分配 |
| V-C06 | PASS | `crates/pr-render/src/theme.rs` · 14 tests | Dark/Light/High-contrast 三套 token 全定义；WCAG 对比度自动校验；24bit→256→16→mono 量化后 JSON token 仍可分辨；**mono 保留 bold、plain 零转义** |
| V-C07 | PASS | `docs/spikes/SPIKE-003.md` · Windows CI 上的 `pty_owner.rs` 11 + `key_matrix.rs` 4 + `width_matrix.rs` 12 + `pty-harness` 36 tests · `crates/prc/src/binout.rs` 4 tests + `win_02_binary_output_reaches_a_pipe_byte_for_byte` · `crates/pr-core/src/signal.rs` 9 tests | **这个 spike 的真正价值是证明了 V-A01 的 Windows 路径此前从未执行过**（harness 自己的测试在 Windows 上全被 `#[ignore]`，注释写着「V-C07 covers Windows」）。第一次真跑 ConPTY 连续暴露 3 个 P0：① ConPTY 启动发 `ESC[6n` 并阻塞等待，harness 不应答则子进程输出一字节都发不出；② crossterm 在 Windows 同时投递 `KeyEventKind::Release`，每个字符被输入两次；③ **粘贴的三条命令全部自动执行，含 `FLUSHALL`**——crossterm 在 Windows 根本不投递 `Event::Paste`，而 §14.1 的时序兜底当时只是个没人调用的纯函数。全部已修并有测试守住。WIN-01：同一套 fixture 在 ConPTY 下通过，前提是断言**渲染后的屏幕**而非字节流（ConPTY 转发的是自身缓冲区差量）。WIN-02：canary 由会被文本模式破坏的字节组成（`\r\n`、裸 `\r`、裸 `\n`、`\x1a`、`\0`、`\xff\xfe`、多字节 UTF-8），管道往返逐字节一致；真实 Windows 风险不是 CRLF 转换而是控制台走 `WriteConsoleW` 无法承载非法 UTF-8，`write_binary` 把失败变成一句可操作的解释。Ctrl+C 六态语义 + Ctrl+Break≡两次 Ctrl+C（把规则跑两遍而非开特例，七种状态逐一对照）。**限制已记录**：`SetConsoleCtrlHandler` 的 OS 投递层尚未接线（需要封装 unsafe 的 crate，与 TLS 栈一起在 ADR-023 决定）；legacy conhost 属人工记录，非一级目标 |

## Track D · 安全与凭证

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-D01 | PASS | `crates/pr-profiles/src/credentials.rs` · 9 tests · 本机 Keychain 实跑 | keyring 3 按平台 feature；`credential:<uuid>` 引用；无可用 store 时 **fail-closed**，回退选项里没有明文文件；keyutils 明确标为非持久 |
| V-D02 | PASS | `crates/pr-profiles/src/journal.rs` · 12 tests | §4.2 六步顺序 + fsync journal + 启动 reconciliation；崩溃点逐一测试：孤儿 secret 只报告不自动删、悬空 profile 标 credential missing、未完成轮换清理可重放且幂等 |
| V-D03 | PASS | `crates/pr-security/tests/approval.rs` 13 tests · `crates/pr-security/src/approval.rs` `preview()` / `escape_bytes()` | SEC-10 说的是：**两条 argv 显示摘要相同，一个令牌只能执行其中一条**。这不是假想——客户端让命令可读的每一种手段（Unicode 规范化、转义、截断、掩码）都是多对一函数，绑在可读形式上的令牌就授权了所有归约到它的东西。测试建了「在合理摘要下会碰撞」的对照集：单参数含空格 vs 两个参数、空尾参数、预组合 vs 分解重音、零宽空格、被截断的同前缀 key、掩码后的不同密码、尾随空格、转义换行 vs 真换行——9 对里 8 对在摘要下完全一致，而哈希与 `preview()` 全部不同。**两个方向都测**：一个会碰撞的 preview 会让字节绑定去保护一个没人能正确做出的决定。preview 显示每个参数的字节长度并把非可打印 ASCII 转义（零宽空格显示为 `\xe2\x80\x8b` 而不是隐形），且不可被恶意参数驱动终端——审批者是在终端里读它的。失效测试覆盖 profile / TrustIdentity / DB / 三个 epoch 各自单独变化、过期、额度耗尽 |
| V-D04 | PASS | `crates/pr-security/tests/trust_binding.rs` 20 tests · `crates/pr-security/src/trust.rs` `RotationWindow` / `KnownNodes` / `RedirectDecision` / `Advisory` | §21.3 的核心决定是：凭证绑 `(tls_identity, server_identity)`，**不**绑 endpoint。因为地址会因为与信任无关的原因移动（DNS、端口映射、Sentinel failover 到已登记节点），每次地址变化都重问会训练用户不读就确认——那时确认什么也保护不了，比没有更糟。证书轮换给出「新旧同时接受 N 天」的窗口：机群不会原子换证，滚动期间有的节点报旧身份有的报新的。窗口**单向且有界**——旧身份会过期，新身份不会；一场永远不结束的轮换就是没人做完的轮换。窗口只放宽**恰好一个**身份，不是「轮换期间什么都收」。边界外重定向必停：`MOVED` 到任意主机再去认证，正是被攻陷或配错的集群收集密码的方式——地址是**服务器**选的。`KnownNodes::accept` 收 decision 而非裸地址，越界节点不经 `accept_confirmed` 根本无法记录，类型让危险路径更长。近似域名（`evil-redis-prod.internal`、`redis-prod.internal.evil.com` 等 4 例）全部判为需确认。**测试找到一个真缺口**：§21.3 要求 sentinel 集合变化「警告不阻断」，但 `compare()` 返回 `NoChange`，调用方无从得知——既有测试的注释写着 warns，代码却警告不出来。新增 `AdvisoryChange(Advisory)`，并把「告知」与「停下来问」分成两个方法 |
| V-D05 | PASS | `crates/pr-render/tests/safetext_boundary.rs` · 10 tests | 15 个真实终端控制 payload × value/key/列标题/generic renderer/4 主题×4 色深；剥离自有 SGR 后断言无 ESC/BEL/NUL/C1/bidi；Debug 与 Display 同样惰性；转义幂等 |
| V-D06 | PASS（history 路径） | `crates/pr-repl/src/history.rs` 泄漏测试 · `pr-profiles` journal/凭证测试 | 按环境分级脱敏（dev 留名 / staging 哈希 / prod 全占位）；AUTH·HELLO·CONFIG·ACL·MIGRATE 无视环境一律脱敏；**直接 grep SQLite 文件断言密码与 PII 不落盘**；搜索只能看到脱敏后文本 |
| V-D07 | PASS | `crates/pr-catalog/src/precedence.rs` · 10 tests | 本地 catalog 是 effects 唯一权威；服务器谎称 write 为 readonly 时分类不变、仅标 `server reports`；未分类命令即使服务器说 readonly 仍算 mutating |
| V-D08 | IN-PROGRESS | — | |
| V-D09 | IN-PROGRESS | — | |

## Track E · 数据模型

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-E01 | PASS | `crates/pr-json/` · 17 tests · `docs/spikes/SPIKE-004.md` | occurrence DOM；重复成员保留+`#n` 寻址；数字词法逐字节保留；歧义路径拒绝；span 局部重写 |
| V-E02 | PASS | `crates/pr-render/src/{table,width}.rs` · 43 tests | 每 field 横线、表头双线、JSON 在 cell 内不加内部横线；宽度采样后冻结；窄屏改纵向块不隐藏列；宽字符/ZWJ/组合字符帧对齐；SEC-05 控制字节不外泄 |
| V-E03 | PASS | `crates/pr-results/src/store.rs` · 14 tests | 64 MiB/16 MiB 预算、oldest-first 淘汰；`Evicted`/`TooLarge`/`Unknown`/`ScopeExpired` 四种原因分开报；**§8.2 顺序**：已知长度且未开始输出才可询问，非 TTY 永不询问 |
| V-E04 | PASS | `crates/pr-application/src/json_projection.rs` · `crates/pr-application/tests/json_projection.rs` 29 tests（§18.3 表格逐行）· `compatibility/manifest.toml` 的 `[json_projection]` 段（6 处实测差异 + 8 处一致） | `--json` 与 `typed-json` 是**两种不同的投影**，§18.1 给了它们不同的任务：前者是官方 CLI 的形状给 `jq` 用，后者是带版本的无损结构给工具用。无损与眼熟互相拉扯，§18.3 选了眼熟——**但把每一处损失都说出来**。14 行逐行有用例，全部用真实 wire bytes 过真解码器。差异不是推测的，是**实测的**：用 `DEBUG PROTOCOL <type>` 在同一台 pinned 服务器上造出每种回复，与 redis-cli 8.0.6 `--json` 逐条对比，6 处差异写进 manifest 并逐条写明「哪个权威说了算 + 为什么」。其中 baseline 输了 5 处：错误回复它输出 `error:"..."`（**根本不是合法 JSON**，`jq` 读不了）、错误退出码给 0（shell 的 `&&` 链会继续往下跑）、RESP3 big number 直接打印 `Unknown reply type: 13` 解析不了、RESP3 attribute 报协议错误并断开、map 的整数 key 被强制转成字符串 key（于是整数 `1` 和字符串 `"1"` 变成同一个 key）。第 6 处是刻意分歧：无效 UTF-8 时 baseline 给 `"\xff\xfe\x80"`——一个 12 字符的 JSON 字符串装 3 个字节，`jq` 看到的长度和内容都是错的，要还原还得懂 redis-cli 的转义规则；我们给 `{"$bytes":"//6A"}` 并在 stderr 提示。这一处正是 V-E04 要求「记录而非默默决定」的那类。另有一条贯穿全模块的性质测试：**无损的行必须安静，有损的行必须出声**——静默的损失正是脚本算错答案而没人发现的方式。PIPE-02：同一条 `HGETALL` 在 `-2` 下是扁平数组、`-3` 下是对象，`--json` 不得抹平；RESP2 偶数数组**不**自动 map 化 |
| V-E05 | PASS | `crates/pr-render/src/generic.rs` · 16 tests | 未知/模块结果走 generic RESP tree：double 保留词法、map 保序且保留重复 key、非 UTF-8 转义并报字节数、attribute 不丢弃、深度有界；任意 shape 不 panic（CMD-10/12、SEC-07） |

## Track F · 智能输入引擎

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-F01 | PASS | `crates/pr-catalog/snapshots/` (redis 8.0.6 / valkey 8.1.10, digest-pinned) · `src/snapshot.rs` + `src/compile.rs` · `ci/catalog-snapshot.sh` · `ci/check-catalog-snapshots.sh` · 38 unit + 19 integration tests | 固定镜像抓取 → blake3 指纹 → 编译成 `CommandSpec`；577 / 379 条命令全部分类，仅 EVAL/EVALSHA/EXEC/FCALL 刻意留 `unknown`（其效果等于被要求执行的内容）；override 表只能加限制，`unknown` 仅在标注 `classified` 时清除；双分支 merge 报告 Redis-only(HEXPIRE/VADD/模块) 与 Valkey-only(COMMANDLOG/CLIENT CAPA/SCRIPT SHOW)，效果冲突为 0；CI 门禁要求快照能从 pinned image 逐字节重现 |
| V-F02 | PASS | `crates/pr-intelligence/src/analyser.rs` · 11 tests（含 3 个 proptest） | 宽容分析器与权威 tokenizer 的 argv 等价性 property；span 恒在范围内且 analyse 不 panic；按 span 替换后参数个数不变、替换落在正确位置（ASSIST-081） |
| V-F03 | PASS | `crates/pr-repl/` · 26 tests | 权威 tokenizer 对齐 `sdssplitargs`；全 256 字节 quote 往返；`:` 本地命令独立 grammar（重复选项报错、零 shell 展开） |
| V-F04 | PASS | `crates/pr-catalog/tests/grammar_table.rs` · 19 tests（§11.7 十一行逐行）· `src/spec.rs` grammar walker | 真实语法树 walker 而非位置表：子命令非 key、HSET field/value 交替且 value 位不枚举、ZADD score/member 不互换（带选项时仍正确）、SET/ZADD 互斥组按版本过滤且冲突可解释不暗改、XADD 嵌套 choice、EVAL numkeys（解析失败时取 0 个 key）、XREAD STREAMS 成对切分、key 名为 NX/GET 由位置决定、ZRANGE BYSCORE/BYLEX 模式、二进制参数无损、模块命令分类 |
| V-F05 | PASS | `crates/pr-intelligence/src/broker.rs` · 11 tests | 到达时校验 revision+scope+序号；过期结果丢弃且**不显示**；焦点跟 `CandidateId` 不跟索引（ASSIST-028）；候选消失时焦点不静默落到别处 |
| V-F06 | PASS | `crates/pr-intelligence/src/scope.rs` · 14 tests | 六元组 scope（profile UUID + 服务身份 + DB + auth/policy/**topology** epoch）；逐项验证任一变化都不泄漏；field 绑定 parent key；schema hint 不主张存在性 |
| V-F07 | IN-PROGRESS | — | |
| V-F08 | IN-PROGRESS | — | |
| V-F09 | IN-PROGRESS | — | |
| V-F10 | PASS | `crates/pr-intelligence/tests/zero_send.rs` · 11 tests | transport spy 计**尝试次数**；分析/候选/接受/焦点/可提交性检查/Guide 组装/换主题/丢弃过期结果/scope 失效，以及完整离线编辑会话 —— 全部 0 条业务命令 |

## Track G · 连接拓扑

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-G01 | PASS | `ci/topology/cluster-{up,down}.sh` · `crates/pr-routing/` 11 tests + 5 个 `topology_cluster_*` 实跑 | 6 节点 cluster_state:ok / 16384 slots；hash tag CRC16 与真服务器 7 个 key 逐一吻合；CROSSSLOT 不被当重定向 |
| V-G02 | PASS | `ci/topology/sentinel-{up,down}.sh` · 3 个 `topology_sentinel_*` | 1 primary + 1 replica + 3 sentinel；根因是缺 `sentinel resolve-hostnames yes`（6.2+ 默认拒绝主机名） |
| V-G03 | IN-PROGRESS | — | |
| V-G04 | IN-PROGRESS | — | |
| V-G05 | IN-PROGRESS | — | |

## Track H · 资源预算与性能基线

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-H01 | PASS | `crates/prc/tests/budgets.rs` 8 tests（CI 三平台跑）· `benches/baseline/README.md` · `crates/pr-core/src/mem.rs` 4 tests | `prc` 刻意链接整条依赖图（tokio / rusqlite bundled / keyring / crossterm / ratatui / blake3 / serde_json / 内嵌 catalog 586 条命令）——只链一半无法回答「依赖有没有先把预算吃掉」。release 实测：`--help` p95 **4.86 ms**（预算 100 ms，余 95 ms）、idle REPL **5.0 MiB**（预算 30，余 25）、idle TUI **6.5 MiB**（预算 60，余 53）、binary 1.6 MiB。CI 断言跑 debug build（更大更慢，通过即保守成立）。`--help` 便宜是**结构性**的而非计时侥幸：catalog 懒编译，测试断言 `--help` 不打印 catalog provenance 而 `--version` 打印。RSS 测量补齐 Windows（`tasklist`）——此前 `rss_bytes()` 在 Windows 返回 `None`，预算根本无从检查。**未链接**：TLS 栈与 `redis-rs`（`pr-transport` 仍为空），上述 headroom 就是它们要装得下的空间，落地后必须重测 |
| V-H02 | IN-PROGRESS | — | |
| V-H03 | PASS | `crates/pr-results/src/subscription.rs` 12 tests · `benches/pubsub/README.md` | §24.3 一句话里有三个决定，各防一种失败：**ring** 而非增长列表（100k msg/s 撑爆任何列表，OOM 会连用户正在看的一起丢）；**两个**上限而非一个（10,000 × 2 KB = 20 MiB，10,000 × 8 B = 80 KB，任一上限单独都不够）；**淘汰计数恒显**（Pub/Sub 是 at-most-once，重连不补发 R17，看不到的就是不知道的）。实测：100k × 256 B → 保留 10,000 条 / 3.64 MB，淘汰 90,000（entry 上限先到）；100k × 4 KB（到达 400 MB）→ ≤16 MiB，byte 上限先到。关键性质 `received == retained + dropped` 在 20,000 条变长消息风暴后断言，计数对不上的淘汰数比没有更糟——它看起来像信息。超过整个缓冲区的单条消息**拒收而非清空缓冲区去装它**，并单独计为 `oversized`。channel/pattern 名字计入字节预算。吞吐 14.9 ms debug / 4.4 ms release 记录 100k 条 |
| V-H04 | PASS | `crates/pr-core/tests/task_soak.rs` 10 tests · `crates/prc/tests/tui_cycle.rs` 3 tests · `crates/pr-core/src/scope.rs` 8 unit tests | 测试抓到 `TaskScope::spawn` 的**真实设计缺陷**：原实现用 `select!` 让 token 与用户 future 竞速，取消时直接 drop future——协作式清理从来跑不到（16 个任务只有 11 个执行了 cleanup），且 `shutdown` 对永不退出的任务也返回 `drained = true`，这个返回值等于谎话。改为不竞速：token 只是信号，有界等待给清理时间，超时才 `abort`，`shutdown` 的 `true` 现在真的表示「每个任务自己走完了」（§24.5 写 journal 前要的正是这个区分）。覆盖：无视 token 的任务、panic 的任务、阻塞线程的任务、飞行中被 drop 的 scope、嵌套 scope 互不影响、每 scope 独立计数。PERF-03：1000 次 TUI 开关（真 coordinator + 真 Ratatui frame），warmup 后前 450 次 +32 KB、后 450 次 **+0 B**，终端 token 每轮都归还 |
| V-H05 | IN-PROGRESS | — | |
| V-H06 | PASS | `crates/pr-repl/src/history.rs` · 19 tests | rusqlite bundled + WAL 多连接共享 + `user_version` schema 版本（更新的文件拒绝打开）；0600 权限；scope 按 profile/identity/db 隔离 |

## Track I · 兼容矩阵、catalog 与许可

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-I01 | PASS | `compatibility/manifest.toml` · CI job `manifest-pinned` | 5 服务器 target 全部 digest 固定；无 `latest`；gate 脚本强制 |
| V-I02 | PASS | `fixtures/catalog/valkey-diff/`（`diff.md` + `summary.txt`）· `crates/pr-catalog/src/diff.rs` 10 tests · `tests/grammar_table.rs` 2 tests · `xtask catalog-diff` · `ci/check-catalog-snapshots.sh` 门禁 | 分歧是**记录**下来的，不是抹平的：呈现并集等于告诉 Valkey 用户 `HEXPIRE` 存在，呈现交集等于对 Redis 用户隐藏它——两者都在回答用户没问的问题。四类差异按「忽略了会坏什么」分节：缺失命令（建议会被服务器拒绝）、参数差异（建议形状不对）、`since` 差异（把选项推给太老的服务器）、**效果差异（在错误的分类上做策略判断）**——最后一类是这项属于 Phase 0 而非文档任务的原因。实测 577 vs 379，362 条完全一致，Redis-only 207（含 183 个捆绑模块 + HEXPIRE 族 + 向量集），Valkey-only 9，参数差异 8 条（`SET IFEQ`、`BGSAVE CANCEL`、`CLIENT KILL PRIMARY`、`CLIENT LIST` 多出 6 个过滤器、`CLUSTER SETSLOT TIMEOUT`…），**效果差异 0**。fixture 由 CI 重新生成并 diff，改手写的内容会被拦下 |
| V-I03 | PASS | `crates/pr-catalog/LICENSES.md`（逐来源结论表）· `docs/adr/ADR-023.md` · `LICENSE-MIT` + `LICENSE-APACHE` · `crates/prc/tests/licence.rs` 6 tests · `crates/pr-catalog/tests/grammar_table.rs` 2 tests | 审查先回答了一个更基本的问题：**二进制里嵌的到底是什么**。`COMMAND DOCS` 的回复里有两类性质完全不同的东西——接口结构（命令名、arity、flags、ACL 分类、key spec、参数树、`since`、`group`）与上游撰写的散文（`summary`、`complexity`，占 16%）。前者取自 digest-pinned 服务器的**运行时回复**而非上游源文件，任何人跑一次服务器都能复现（CI 每次都在逐字节复现）；后者是别人写的英文。**结论：散文从快照里整体移除**——`ci/catalog-normalise.py` 归一化阶段丢弃，`snapshot.rs` 的 schema 里没有这两个字段，而因为那些 struct 是 `deny_unknown_fields`，将来某次抓取若重新带上它们会**加载失败**而不是悄悄分发没人审过许可的文本。快照体积 343 KB → 286 KB。Redis `src/commands/*.json` 与文档站（CC-BY-SA-4.0）**从未取用**，避开后者是刻意的——它的 copyleft 会传染到我们自己写的说明。命令说明改由本仓独立撰写（§13 要的本来就是给卡在提示符前的人看的文字，不是参考手册的一句话），落点 `descriptions.rs`（Phase 2）；在它存在之前 `CommandSpec::summary` 为空字符串，**明显缺失而非悄悄借用**。`prc` 自身许可定为 **`MIT OR Apache-2.0`**：Rust 生态事实标准，企业法务例行放行，而 AGPL 常被明确禁止——一个开发者 CLI 被法务禁用等于没人用。**代价明说**：这不给 Penguin Studio 任何商业保护，任何人可闭源分发。决定**可逆且刻意做成一处改动**（workspace `license` 字段 + 两个 LICENSE 文件），项目未发布任何二进制、未上 crates.io，Phase 7 之前更换无需任何外部同意。**未做而记下的**：「接口结构 vs 散文」的划分是工程判断不是法律意见，发布前需人工法务确认，已列入发布门禁，不阻塞 Phase 1 |
| V-I04 | PASS | `compatibility/redis-cli-modes.toml`（20 modes）· `compatibility/redis-cli-8.0.6-help.txt` · `crates/pr-application/tests/cli_modes.rs` 12 tests | §28.4 明说理由：「名称与详细 flags 由该 baseline 的 help/source 生成登记」——凭记忆写的清单会悄悄漏掉没人想到的那个模式，而漏掉只在用户真去跑它、发现没有时才暴露。inventory 从 digest-pinned 的 redis-cli 8.0.6 help 生成，help 原文一并入库，**双向**校验：清单里有而 baseline 没有的 flag 是「为不存在的东西定契约」；baseline 有而清单没有的是「没人决定过的兼容缺口」。`NOT_A_MODE` 显式列出而非用规则过滤——新 flag 会同时落在两边之外并让测试失败，而不是被某条恰好匹配的规则悄悄归类；**测试第一次跑就抓到 `-2`/`-3` 未分类**。发现两个 §28.4 未命名的模式：`--hotkeys`（属同一行，但多一个前提——只在 `maxmemory-policy` 为 LFU 时有效，必须说明而非返回空结果让人以为「没有热 key」）与 `--intrinsic-latency`（**不连服务器**，测本机调度延迟，§28.4 的 latency 行假设的是 INFO/PING 循环）。安全列逐条断言：`--rdb` / `--functions-rdb` / `--replica` / `--lru-test` prod+agent 双 deny；`--pipe` / `--eval` / `--ldb` / `--cluster` agent deny；`--ldb-sync-mode` 的策略必须与 `--ldb` 不同（它阻塞服务器，代价由其他客户端承担）；`-r` 的策略是乘法而非继承一次 |

## Track J · 契约冻结与 ADR

| ID | 状态 | 证据 | 备注 |
|---|---|---|---|
| V-J01 | PASS | `docs/adr/ADR-001..030.md` + `README.md` | 30 条全部 accepted；ADR-023/026 的**结论**分别由 V-I03 / SPIKE-001 填入 |
| V-J02 | IN-PROGRESS | — | 进行中：`pr-core`(SafeText/ExecutionOutcome/CommandRequest/TaskScope)、`pr-security`(TrustIdentity/ApprovalToken)、`pr-json`(JsonNode) 已冻结；`pr-intelligence`/`pr-repl` 待办 |
| V-J03 | IN-PROGRESS | — | |
| V-J04 | IN-PROGRESS | — | |

## Meta

| ID | 状态 | 证据 |
|---|---|---|
| P0-META-01 | IN-PROGRESS | 本文件 |
| P0-META-02 | IN-PROGRESS | — |
| P0-META-03 | IN-PROGRESS | — |

## 变更日志

| 日期 | 变更 |
|---|---|
| 2026-09-16 | V-E04 → PASS（JSON projection v1）；与 pinned `redis-cli --json` 实测 6 处差异入 manifest，其中 5 处是 baseline 的缺陷（错误回复不是合法 JSON、big number/attribute 解析不了、整数 key 被强转）；824 tests |
| 2026-09-16 | V-I03 → PASS（catalog 再分发结论 + `prc` 许可）；快照移除上游散文（343 KB → 286 KB），`license` 由占位符改为 `MIT OR Apache-2.0`；795 tests |
| 2026-09-16 | V-I04 → PASS（`redis-cli` 特殊模式 inventory）；双向校验抓到未分类 flag 与两个 §28.4 未覆盖的模式；`ci/mark-phase0.py` 改为从文件读证据与备注（反引号曾被 shell 命令替换吃掉） |
| 2026-09-16 | V-I04 → PASS（redis-cli 特殊模式 inventory）；双向校验抓到未分类 flag 与两个 §28.4 未覆盖的模式 |
| 2026-09-16 | V-D04 → PASS（TrustIdentity 绑定 / 证书轮换窗口 / 边界外重定向）；补上 §21.3 要求但实现缺失的 sentinel 集合变更警告 |
| 2026-09-16 | 建立报告；workspace 骨架；环境记录 |
| 2026-09-16 | V-A03 / V-I01 / V-J01 → PASS；pr-core + pr-security 契约冻结；CI workflow 与 4 个 gate 脚本落地 |
| 2026-09-16 | V-E01 → PASS（pr-json occurrence DOM，SPIKE-004）；workspace 85 tests 全绿 |
| 2026-09-16 | V-A01 / V-A04 / V-A06 → PASS（PTY harness、故障注入、资源测量）；111 tests 全绿 |
| 2026-09-16 | V-B02 → PASS（增量 RESP decoder）；首次 CI 实跑暴露 4 个真实问题并修复；134 tests 全绿 |
| 2026-09-16 | V-F03 → PASS（quoting + 本地命令 grammar）；第二轮 CI 暴露 5 个问题并修复；Cluster/Sentinel 起停脚本落地；160 tests 全绿 |
| 2026-09-16 | V-B07 → PASS（prc 参数契约）；177 tests 全绿 |
| 2026-09-16 | CI run 35048391825：15/17 绿。cluster/sentinel 脚本首跑失败，已记入 V-G01/V-G02 |
| 2026-09-16 | V-G01 / V-G02 → PASS：两个 topology 根因定位并修复；新增 `pr-routing` slot/hash-tag 实现与 8 个实跑测试；196 tests 全绿 |
| 2026-09-16 | **CI 17/17 全绿** → V-A05 PASS；SPIKE-001 结论：Reedline 不达标，V-C01 走 ADR-026 回退并实现自有 LineBuffer；216 tests 全绿 |
| 2026-09-16 | V-B05 / V-D01 → PASS（会话状态机、三平台凭证存储）；244 tests 全绿 |
| 2026-09-16 | V-D02 → PASS（凭证 journal 与崩溃 reconciliation）；256 tests 全绿 |
| 2026-09-16 | V-C06 → PASS（三主题 token + 量化降级）；测试暴露 mono/plain 语义混淆并分离；270 tests 全绿 |
| 2026-09-16 | V-E02 → PASS（TableModel + 显示宽度策略）；299 tests 全绿 |
| 2026-09-16 | V-E05 / V-B04 → PASS（generic RESP 渲染、outcome 组合矩阵）；327 tests 全绿 |
| 2026-09-16 | V-H06 / V-D06 → PASS（history 存储与按环境脱敏）；346 tests 全绿 |
| 2026-09-16 | V-E03 / V-D07 → PASS（有界 result store、catalog 权威性）；370 tests 全绿 |
| 2026-09-16 | V-B03 → PASS（`--pipe` 逐帧策略，BLOCKER #1 验证完成）；384 tests 全绿 |
| 2026-09-16 | V-F02 → PASS（双解析器等价性 property）；395 tests 全绿 |
| 2026-09-16 | V-F05 / V-F06 → PASS（异步 broker、observation scope 隔离）；420 tests 全绿 |
| 2026-09-16 | V-F10 / V-D05 → PASS（零发送不变量、SafeText 显示边界）；441 tests 全绿 |
| 2026-09-16 | V-F01 / V-F04 → PASS（catalog 编译流水线 + §11.7 语法全表）；walker 抓到两个真 bug（Choice 选不到无关键字分支、numkeys 解析失败仍吞 key）；`deny_unknown_fields` 抓到快照残留字段；新增 `catalog` CI 门禁；488 tests 全绿 |
| 2026-09-16 | V-C02 → PASS（单一 terminal owner）；引入 crossterm（ADR-022 指定）作 raw mode 与事件源；真实 PTY 上跑 ASSIST-062 / UX-12 / 场景 H；524 tests 全绿 |
| 2026-09-16 | V-B06 → PASS（push 帧与 one-shot 输出契约）；PTY harness 补 DSR 应答器——Windows CI 显示 ConPTY 启动发 `ESC[6n` 并阻塞等待，harness 不应答导致子进程输出永远发不出（V-A01 的 Windows 路径此前全被 `#[ignore]`，V-C02 是第一次真跑）；catalog 嵌入二进制并 fail-closed；558 tests |
| 2026-09-16 | V-H01 → PASS（启动与空闲基线）；`prc` 链接全依赖图，`pr-tui` 接入 Ratatui，RSS 测量补 Windows 路径；三项预算余量 95 ms / 25 MiB / 53 MiB 并记入 `benches/baseline/` |
| 2026-09-16 | PTY harness 补屏幕模型（`Screen`，14 tests）与 DSR 应答器；Windows 上 crossterm 的 `KeyEventKind::Release` 导致每个字符输入两次，translate 搬进 `pr-terminal::bridge` 并加 8 个测试；`tasklist` RSS 解析被千位分隔符截断（48 MB 读成 120 KB）已修；603 tests |
| 2026-09-16 | V-H04 → PASS（任务监督 leak detection / PERF-03）；修正 `TaskScope` 的取消语义（`select!` 竞速使协作清理永远跑不到、`shutdown` 返回值失真）；617 tests |
| 2026-09-16 | V-C04 → PASS（Unicode 宽度矩阵）；新增 `DrawMode::CursorReset` 降级绘制、`CSI 6 n` 探测与 `prc --probe-width`；屏幕模型改为 grapheme cluster 格子；651 tests |
| 2026-09-16 | V-C03 → PASS（bracketed paste 与时序启发式）；paste-staging 接入 coordinator，`Action::SubmitMany` 让「一次粘贴多条命令」必须被显式处理；687 tests |
| 2026-09-16 | V-C05 → PASS（按键可达性）；§14.2 表格入代码 + `PR_KEYPROBE` 键位探针；700 tests |
| 2026-09-16 | V-C07 → PASS（SPIKE-003 Windows ConPTY）；WIN-01 / WIN-02 通过；Ctrl+C / Ctrl+Break 语义层完成；719 tests |
| 2026-09-16 | V-I02 → PASS（Valkey vs Redis 差异清单）；新增 `Node` 签名渲染与 `xtask catalog-diff`；730 tests |
| 2026-09-16 | V-H03 → PASS（订阅风暴 ring buffer / PERF-02）；`benches/pubsub/` 基线入库 |
| 2026-09-16 | V-D03 → PASS（审批令牌字节哈希 / SEC-10）；新增 `:approval show --bytes` 的 preview 与转义；755 tests |
