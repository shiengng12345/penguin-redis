# 产出路径对照表（P0-META-01 / P0-META-02）

> 计划 §2.3 的 **产出** 列是复核者用来找证据的入口。七十条路径里有三十九条指向从未创建的目录：
> 计划写 `tests/pty/harness/`，harness 实际落在 `xtask/pty-harness/`，中间没有任何东西把两者对上。
> **每一条的工作都存在**（台账逐条引用了它），但顺着计划走的人看到的是一个空目录，
> 而空目录和「没做」在肉眼下没有区别。
>
> 所以有了这张表，以及 [`ci/check-plan-outputs.sh`](../ci/check-plan-outputs.sh)：
> 计划里的每条路径**要么真实存在且非空，要么在这里写明它去了哪**，而映射的目标同样会被检查——
> 一个指向空气的映射比没有映射更糟。

## 为什么路径会漂

三类，都不是疏忽：

1. **harness 属于 `xtask/` 而不是 `tests/`。** 它们是要被 `cargo run` 的程序，不是
   `cargo test` 会自动发现的集成测试。计划写这一列的时候还没有做出这个结构决定。
2. **单元测试跟着被测代码走。** 计划写 `tests/session-state/`，而会话状态机的测试就在
   `crates/pr-core/src/session.rs` 的 `mod tests` 里——Rust 的惯例是单元测试与实现同文件，
   把它们搬到一个顶层目录只会让它们看不见私有状态。
3. **一个目录变成了一份 README。** `tests/topology/ssh/` 这类目录里放的是「怎么跑、抓到了什么」，
   实测本身是 `crates/pr-transport/tests/ssh_live.rs` 加一个 CI job。

## 对照

| 计划里的路径 | 实际位置 | V 项 |
|---|---|---|
| `tests/pty/harness/` | `xtask/pty-harness/` | V-A01 |
| `fixtures/pty/baseline/` | `xtask/pty-harness/src/lib.rs` | V-A01 |
| `tests/fault/` | `xtask/src/fault.rs` | V-A04 |
| `xtask/measure/` | `xtask/src/measure.rs` | V-A06 |
| `tests/pipe/` | `crates/pr-application/src/pipe.rs` | V-B03 |
| `fixtures/hostile-input/pipe/` | `crates/pr-application/src/pipe.rs` | V-B03 |
| `fixtures/outcome/` | `crates/pr-protocol/tests/outcome_matrix.rs` | V-B04 |
| `tests/session-state/` | `crates/pr-core/src/session.rs` | V-B05 |
| `tests/output/push/` | `crates/pr-application/tests/push_output.rs` | V-B06 |
| `crates/prc/tests/args.rs` | `crates/prc/src/args.rs` | V-B07 |
| `tests/editor-state/` | `crates/pr-repl/src/buffer.rs` | V-C01 |
| `tests/pty/owner/` | `crates/pr-terminal/tests/pty_owner.rs` | V-C02 |
| `tests/pty/paste/` | `crates/pr-terminal/tests/paste_matrix.rs` | V-C03 |
| `tests/render/width/` | `crates/pr-terminal/tests/width_matrix.rs` | V-C04 |
| `tests/pty/keys/` | `crates/pr-terminal/tests/key_matrix.rs` | V-C05 |
| `fixtures/renderer/colour/` | `crates/pr-render/src/theme.rs` | V-C06 |
| `tests/credentials/` | `crates/pr-profiles/src/credentials.rs` | V-D01 |
| `tests/credentials/journal/` | `crates/pr-profiles/src/journal.rs` | V-D02 |
| `tests/topology/trust/` | `crates/pr-security/tests/trust_binding.rs` | V-D04 |
| `tests/security/terminal-injection/` | `crates/pr-render/tests/safetext_boundary.rs` | V-D05 |
| `tests/security/secret-path/` | `crates/prc/tests/secret_paths.rs` | V-D06 |
| `tests/catalog/precedence/` | `crates/pr-catalog/src/precedence.rs` | V-D07 |
| `fixtures/renderer/table/` | `crates/pr-render/src/table.rs` | V-E02 |
| `crates/pr-results/tests/` | `crates/pr-results/src/store.rs` | V-E03 |
| `tests/output/json-projection/` | `crates/pr-application/tests/json_projection.rs` | V-E04 |
| `fixtures/renderer/generic/` | `crates/pr-render/src/generic.rs` | V-E05 |
| `crates/pr-intelligence/tests/equivalence.rs` | `crates/pr-intelligence/src/analyser.rs` | V-F02 |
| `fixtures/catalog/quoting/` | `crates/pr-repl/src/lib.rs` | V-F03 |
| `tests/local-commands/` | `crates/pr-repl/src/lib.rs` | V-F03 |
| `fixtures/assistance/grammar/` | `crates/pr-catalog/tests/grammar_table.rs` | V-F04 |
| `tests/assistance-network/async/` | `crates/pr-intelligence/src/broker.rs` | V-F05 |
| `tests/assistance-network/scope/` | `crates/pr-intelligence/src/scope.rs` | V-F06 |
| `fixtures/assistance/find/{canonical,heldout}/` | `fixtures/assistance/find/canonical/` | V-F08 |
| `tests/assistance-network/zero-send/` | `crates/pr-intelligence/tests/zero_send.rs` | V-F10 |
| `tests/topology/cluster/` | `ci/topology/cluster-up.sh` | V-G01 |
| `tests/topology/sentinel/` | `ci/topology/sentinel-up.sh` | V-G02 |
| `tests/soak/tasks/` | `crates/pr-core/tests/task_soak.rs` | V-H04 |
| `tests/history/` | `crates/pr-repl/src/history.rs` | V-H06 |
| `crates/*/src/contract.rs` | `crates/pr-core/src/contract.rs` | V-J02 |

**`heldout` 那一行值得单独说一句**：计划写的是 `fixtures/assistance/find/{canonical,heldout}/`，
实际长出了五个目录——`canonical/`、`heldout-independent/`（活的，门槛判它）、
`heldout-independent-v1-void/` 与 `heldout-v{1,2,3}-void/`（作废的，留档可审计）。
这不是漂移，是 §16.4 的回退条款执行了四轮之后应有的形状。

## 规则

- 新写的东西**按实际结构放**，不要为了迁就这张表去造目录。
- 路径一旦与计划不一致，**同一个 PR 里补一行**。`ci/check-plan-outputs.sh` 在
  `phase-0-gate` job 里跑，漏了会红。
- 这张表**不改计划**。计划是当时的意图，台账是现在的事实，这张表是两者之间的桥。
