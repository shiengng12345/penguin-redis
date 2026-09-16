# 冻结的接口骨架（V-J02，v2.1 §11.11、§19.2、§21.3、§23.2、§3.5、§8.3、§12.9、§31.4）

Phase 1 开始之前冻结十二个类型。理由不是「改起来麻烦」——改一处的代价从来不是那次编辑，而是**几个月后发现有两个 crate 对同一个字段的含义理解不同**。

冻结不等于不可改，等于**带版本**：每个拥有契约的 crate 有一个 `SCHEMA_VERSION`，`crates/prc/tests/contracts.rs` 强制十二个类型全部存在、各自带 doc test、且各 crate 的 `OWNED` 列表合起来恰好是这十二个（多一个少一个都红）。

## 为什么是 doc test 而不是说明文字

一个例子编译不过的类型，它的文档已经和它漂移了——而这种漂移在有人真的去用它、发现例子只是「愿景」之前，是看不见的。测试还要求例子里**出现该类型本身**：一个只打印常量的例子可以在类型底下变了之后继续编译很久。

`ignore` 与 `no_run` 的 doc test 不算数，因为它们不会被运行，也就显示不出漂移。

## 十二个

| 类型 | crate | 定义在 | 它保证什么 |
|---|---|---|---|
| `CommandRequest` | pr-core | `src/request.rs` | 到达 kernel 的请求，argv 是**精确字节**；审批哈希算的就是它 |
| `ExecutionOutcome` | pr-core | `src/outcome.rs` | 四个正交维度：delivery / reply / effects / render（ADR-021） |
| `ResultRecord` | pr-core | `src/contract.rs` | 一次执行与**还留着什么**；`Retention` 永远不会把「我们丢掉了」说成「它不存在」 |
| `SafeText` | pr-core | `src/safetext.rs` | 服务器来的字符串到达 renderer 之前的唯一通道（§23.5） |
| `TaskScope` | pr-core | `src/scope.rs` | `JoinSet` + `CancellationToken` + drop 时 abort 的显式监督 |
| `CompletionRequest` | pr-intelligence | `src/contract.rs` | 带 **buffer revision** 与 **observation scope**，迟到或跨目标的结果因此可判（§12.8、ADR-017） |
| `CompletionCandidate` | pr-intelligence | `src/contract.rs` | `insert` 与 `display` 是**两个字段**——二进制名字插入原字节、显示转义（ASSIST-019/083） |
| `AssistanceSnapshot` | pr-intelligence | `src/contract.rs` | 某一瞬间的完整状态；`focused: None` 是 Enter 可以提交的唯一状态（ADR-016） |
| `TrustIdentity` | pr-security | `src/trust.rs` | endpoint / tls_identity / server_identity / auth_identity 四元组（§21.3） |
| `ApprovalToken` | pr-security | `src/approval.rs` | 绑定 argv 的**字节**哈希 + 完整身份 + epoch + 有效期（§23.2、ADR-024） |
| `LocalCommandSpec` | pr-catalog | `src/contract.rs` | effects / 危险性 / 审批等级 / key 位置的**唯一权威**；远端只能加风险（ADR-030） |
| `JsonNode` | pr-json | `src/dom.rs` | 服务器业务 JSON 的无损模型，重复成员按 occurrence 索引寻址（ADR-028） |

## 几条在类型里、而不是在约定里的规则

**`Retention` 不会把本地淘汰说成服务器上没有。** `EvictedForBudget` 的文案是「未保留：为不超出结果存储预算而丢弃，重跑可再看」，不是「不存在」。客户端的预算说明不了服务器上有什么，而一个被告知「不存在」的用户会停止寻找（§12.4）。`reply_bytes` 在淘汰之后仍然保留——「那条回复 400 MiB，没有留」有用，「这里什么都没有」没用。

**`CompletionCandidate` 的 `insert` 和 `display` 不能合成一个。** 合成之后无论选哪个都是 bug：要么往输入缓冲里插了一段转义文本，要么往终端上写了裸控制字节，而 §23.6 禁止后者。

**`Provenance::asserts_existence()` 只对真正观察到的名字为真。** catalog 里的名字说明「这条命令是这么拼的」，不说明「这个 key 在那里」（§10.3）。

**`LocalCommandSpec::merge_remote` 只会 OR 上风险，永远不会摘掉。** 一个被污染或过时的 `COMMAND` 回复把写命令标成只读，在这里改变不了任何事——而且 `unknown` 标志不会被远端清掉：本地 catalog 不认识某条命令是关于**本地 catalog** 的事实，服务器无权裁决，而这恰恰是一台被攻破的服务器最想声称的那件事（ADR-030）。

**`Approval::policy_may_satisfy()` 只对最轻一档为真。** 策略型批准不能代替人（§29.4、R12）。

## 版本怎么用

存过盘的契约——history、spool 文件、诊断包——带着写入时的 `SCHEMA_VERSION`。读不懂的版本要**拒绝**而不是猜。对 `TrustIdentity` 来说尤其如此：猜错的后果是把密码发给了错的服务器。

## 还没冻结的

`ResultStore`、`Coordinator`、`Decoder` 这些是实现，不是契约——它们会随 Phase 1 变化，而变化不应该波及依赖方。§31.4 的依赖箭头指向这十二个，不指向它们。
