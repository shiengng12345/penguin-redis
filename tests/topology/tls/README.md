# TLS 矩阵（V-G04 / NET-01，v2.1 §21.1、§21.3）

实现在 `crates/pr-transport/src/tls.rs`，矩阵在 `crates/pr-transport/tests/tls_matrix.rs`（11 tests），命令行侧在 `crates/prc/src/args.rs`（6 tests）。决策见 [ADR-032](../../../docs/adr/ADR-032.md)。

## 通过标准

> NET-01 | TLS 错名/过期/未知 CA | **拒绝或显式配置；无静默 insecure**

## 证书不入库

每一张证书都在测试运行时由自签 CA 现场签发。把它们提交进仓库会更糟：一个 `notAfter` 固定的 fixture 会变成一个在没人挑的日子突然变红的测试，而「已过期」那一格最终会和「有效」那一格没有区别。

## 矩阵

| 情形 | 期望 | 实际 |
|---|---|---|
| 配置 CA 签发、名字正确、在有效期内 | 连上，且连接真能跑数据 | ✅ `+PONG` 往返 |
| 证书上的名字是 `other.example`，要验 `redis.internal` | 拒绝，且错误指向**名字** | ✅ `Verification(... name ...)` |
| 证书已过期 | 拒绝，且错误指向**有效期** | ✅ `Verification(... expired ...)` |
| 证书由未知 CA 签发（名字对、日期对） | 拒绝，且错误指向**签发者** | ✅ `Verification(... issuer/unknown ...)` |
| 服务器要求 client cert，我们提供 | 连上 | ✅ |
| 服务器要求 client cert，我们不提供 | 拒绝 | ✅ |
| 连 `127.0.0.1`，验 `redis.internal`（SNI 独立） | 连上 | ✅ |
| 同一张证书，改成用地址当验证名 | 拒绝 | ✅ |
| `--insecure` / `--no-verify` / `--tls-verify=none`（**无 profile**） | 拒绝 | ✅ `InsecureTlsRefused` |
| `--insecure`（**有 profile**） | 拒绝，且说出是谁的验证会被削弱 | ✅ `TlsWeakening(flag, profile)` |
| CA bundle 为空 / 不是 PEM | 报错，**不回退**到系统根或空集 | ✅ `EmptyCaBundle` |

## 「无静默 insecure」是两件事

**第一件：没有那个开关。** 不是「默认关闭」，是不存在。一个存在的开关，就是一个会在凌晨两点被打开的开关。`there_is_no_configuration_that_turns_verification_off` 在源码上断言 `dangerous`、`ServerCertVerifier`、`accept_invalid` 这些标识符不出现在 TLS 模块的**代码**里——先剥掉注释和字符串字面量，否则「这里没有 insecure 模式」这句说明本身就会触发检查，而人们修这种失败的方式是把说明删掉。

**第二件：拒绝必须是重定向，不是删功能。** `--tls-ca`、`--tls-name`、`--tls-cert`、`--tls-key` 就是那个去处，`--help` 里列全，拒绝的错误信息里点名。只说「不行」而不说「那该怎么办」，是变通方案被发明出来的方式。

## V-G04 在命令行侧发现的真实缺口

`--insecure` 之前只在**有 profile** 时被拒绝。没有 profile 时（`prc -h host --insecure`）它被记录下来然后**静默忽略**——这正是 §21.1 所禁止的静默 insecure，而且是更坏的那一种：操作者以为这个 flag 起了作用。现在无论有没有 profile 都拒绝，有 profile 时给更具体的那条消息（说出是谁的验证）。

## 验证名与地址分开，是配置不是推断

§21.3：

> 支持明确的 endpoint mapping，但 TCP 目标、TLS 验证名和凭证适用范围分开配置；**不能为了可达就关闭 TLS 校验**。

一个 cluster 节点宣告了一个你能连上的地址，不等于它可以决定自己的证书按什么名字被检查。`TlsConfig::server_name` 因此是独立字段，不是从连接的 host 取的。矩阵里两行一起证明这不是「不检查」：连 `127.0.0.1` 验 `redis.internal` 通过，同一张证书改用地址当验证名则被拒。

## `ca_set_hash` 把凭证绑到签发者

§21.3 的 `TrustIdentity::Ca { ca_set_hash, server_name }`。CA 集合的 DER 字节按出现顺序做 blake3。两个不同 CA 为同一个名字背书，是两个不同的 trust identity——为其中一个存的凭证不会自动发给另一个。往集合里加一个 CA 也会改变它：放宽「谁可以背书」是身份的改变，不是细节。

## 还没做的

- **Cluster / Sentinel over TLS**：需要真实拓扑，归 V-G03 与 Phase 3。
- **系统信任库**：目前只支持显式 CA 集合。企业环境要用系统根（§21.1 的 Corporate network 一行）时再加，且必须同样产出一个可绑定的 `ca_set_hash`。
- **TLS 握手的启动预算影响**：`benches/baseline/README.md` 记的 headroom 仍未含 TLS 链接进 `prc` 之后的数字，待 `passthrough` 真正走 TLS 时重测。
