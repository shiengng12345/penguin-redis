# 共享配置文件的并发（V-G05 / WIN-03 / LIFE-03，v2.1 §30.2、§12.10、R20、R39）

实现在 `crates/pr-profiles/src/{shared,perms}.rs`；测试在 `crates/pr-profiles/tests/shared_files.rs`（7 tests，三平台常驻）与 `crates/pr-profiles/tests/credential_store.rs`（4 tests，其中 3 个碰真实系统凭证库，CI 以 `--ignored` 在三平台跑）。

## §12.10 的协议，逐条为什么

> 写入前取 advisory lock（POSIX `flock` / Windows `LockFileEx`），读取版本号 → 修改 → 临时文件 → fsync → 原子 rename → 版本号 +1；版本号不匹配时不覆盖，写入 `conflicts/` 目录并提示用户合并。

| 环节 | 它挡住什么 |
|---|---|
| **锁** | 两个进程的 read-modify-write 交错 |
| **版本检查** | 锁挡不住的那一种：一个进程读过、放开了锁（或根本没拿）、回来写一个基于过期输入算出来的值 |
| **临时文件 + fsync + rename** | 崩溃后留下半个文件。**没有 fsync，rename 可能先于内容落盘**，断电后活下来的是一个看起来合法的空文件 |
| **`conflicts/`** | 拒绝写入之后把用户的编辑丢掉——那仍然是丢了 |

锁来自 `std::fs::File::lock`（Rust 1.89 稳定）：Unix 上是 `flock`，Windows 上是 `LockFileEx`，不需要依赖，也不需要我们写 unsafe。它是 advisory 的，如 §12.10 所说——它协调愿意使用它的进程，不阻止 `cat > file`。

**锁在单独的锁文件上，不在数据文件上。** 数据文件是被 rename 替换的；加在它身上的锁，是加在一个即将不再是「大家看的那个文件」的 inode 上。

## LIFE-03：无丢更新，用真实进程测

8 个**进程** × 20 轮 = 160 次更新，一次不丢，且版本号恰好走到 160。

用进程而不是线程，因为 advisory lock 是进程之间的承诺——同一进程里的线程在多数平台上共享这把锁，一个多线程版本的测试会在一个根本不工作的实现上通过。

worker 遇到冲突会重试：本项要证明的是没有更新被**丢掉**，不是没有更新被**拒绝**。一次保住了编辑的拒绝，正是协议在工作。

## 过期写入者被拒绝，且编辑被保留

测试构造了锁挡不住的那一种：读到 v1，别人写成了 v2，然后基于 v1 写回。结果是 `Conflict { expected: 1, found: 2, conflict: ... }`，磁盘上仍是别人的 v2，而被拒的编辑躺在 `conflicts/` 里。错误信息说的是「该编辑没有被丢弃：它在 ...」，而不是只说出了错。

## 没有版本头的文件不会被盲写

一个没有版本行的文件不是我们写的。给它猜一个版本号，就是版本协议不再保护任何东西的那一刻。这种文件读出来是 `NoVersion`，`update` 直接失败，手写的内容一个字节都不动。

## 目的地永不原地改写

任何时刻打开这个文件的读者，看到的都是完整的某一版——旧的或新的。这成立是因为目的地只通过 rename 被替换；如果它被以写模式打开并截断，中间就有一个它是空的窗口。测试比对替换前后的 inode（Windows 上比对创建时间与大小），并检查没有 `.tmp` 残留。

## WIN-03：ACL 仅当前用户

| 平台 | 机制 |
|---|---|
| Unix | `chmod 0600` |
| Windows | 显式的、**protected** 的 DACL，只授权当前用户 SID |

Windows 这一处是整个 workspace 里唯一的 `unsafe`，值得说清楚为什么：Rust 的 `std` 无法给文件挂 security descriptor——`OpenOptionsExt` 提供的是 flags 与 QoS，不是 DACL。而「继承 `%LOCALAPPDATA%` 恰好授予的权限」不是同一个承诺，那是对别人配置的那个目录的一个猜测。WIN-03 要求 ACL 是**被设置**的，所以它被设置了：一个独立模块，四次 Win32 调用，每次返回值都检查，每处分配在成功与失败路径上都释放，并且**把结果读回来验证**——一个静默的 no-op 不能冒充成功。

SDDL 用的是 `D:P(A;;FA;;;<当前用户 SID>)`：`D:P` 表示 protected，不继承父目录的任何授权。

**三类文件都受限**：数据文件、锁文件，以及 `conflicts/` 下的文件——最后这类最容易忘，因为它写在失败路径上。检查函数必须能说「不」，否则它什么也没说：有一条 Unix 测试先把文件设成 0644、断言被判为不安全，再修复、再断言。

## 凭证存取碰真实系统库

macOS Keychain / Windows Credential Manager / Linux Secret Service。用 mock 只能证明 mock 能工作，而要验证的是**平台接受我们递过去的东西**。密码里塞了非 ASCII、引号、换行、制表符——如果平台存不住这些，在这里发现比在用户的密码里发现好。

这三个测试是 `#[ignore]` 的，理由只有一个：一台没有 D-Bus 会话的无头 Linux 机器上没有库可谈，那里的失败报告的是环境而不是代码。CI 在三个平台上显式跑它们，Linux 上先起一个会话 keyring。

写进去的东西按进程号命名，并且在**每一条路径上**（含断言失败）删除——一个会在开发者 Keychain 里留下条目的测试，是一个会被他们关掉的测试。

**两个引用不会撞车**：这是 LIFE-03 的「凭证误绑定」那一半——如果两个 profile 共用一个条目，改其中一个的密码会悄悄改掉另一个。

## 还没做的

- **staged update + compensation**（§30.2：credential store 与配置文件之间没有天然原子事务）。协议的骨架在这里，补偿逻辑归 Phase 2。
- **迁移**（先验证 schema、再建可恢复备份）：归 Phase 7。
- **删除共享 credential 前列出所有引用**：需要 profile 索引，归 Phase 2。
