# SSH 隧道：per-node 与 SOCKS5（V-G03 / NET-07，v2.1 §21.5、R15）

实现在 `crates/pr-transport/src/ssh.rs`；参数与协议测试在 `crates/pr-transport/tests/ssh_tunnels.rs`（15 tests，无需 Docker）；实测在 `crates/pr-transport/tests/ssh_live.rs`（7 tests，真 sshd + 真 cluster）；fixture 在 `ci/topology/ssh/`。

## 安全性质活在 argv 里，所以逐字断言 argv

§21.5：「实现首选受控、可审计的系统 OpenSSH 适配，**不拼接 shell 字符串**」。

一个悄悄消失的选项不会改变任何行为测试看得见的东西——它只是把 NET-07 从保证变成巧合。所以这些是被直接断言的：

| 选项 | 没有它会怎样 |
|---|---|
| `ControlMaster=no` + `ControlPath=none` | `ssh` 可能接上用户已经开着的复用会话，而我们退出时会把**他们的**会话一起关掉。NET-07 就是这一条 |
| `StrictHostKeyChecking=yes` + profile 指定的 `UserKnownHostsFile` | 一个携带凭证的客户端做 trust-on-first-use，等于一次中间人就被永久信任 |
| `BatchMode=yes` | 在管道里问「are you sure?」的隧道会永远挂住 |
| `ProxyCommand=none` | 用户配置里的 `ProxyCommand` 是一个 profile 从未批准的可执行文件 |
| `-L 127.0.0.1:...` 显式绑定 | 不写地址时 `ssh` 按 `GatewayPorts` 绑定，一个网络可达的 forward 是一个直通内网地址的洞 |
| `IdentitiesOnly=yes`（有 `-i` 时） | 否则 `ssh` 会先把 agent 里所有 key 递出去——在一台记录 offered key 的跳板机上，那是用户全部的 key |
| `ExitOnForwardFailure=yes` | 否则进程活着而 forward 没起来，连接会去到一个不存在的地方 |

还有一条：把带 `;` 的主机名、用户名、内网地址喂进去，断言危险文本**完整地留在一个 argv 元素里**——这才证明它从未被任何东西解析过。

## 两种模式，实测都跟得上 MOVED

单个 loopback forward 只能到 seed：`MOVED` 报的是**内网地址**，而那个 loopback 端口不通向那里。

| 模式 | 跟随 MOVED 的代价 | 实测 |
|---|---|---|
| `per-node` | 为新节点再开一条 `-L` | ✅ 两条隧道，端口互不相同，reshard 后回收离开的节点 |
| `socks5` | 同一条 `-D` 上再发一次 CONNECT | ✅ 一条 forward 到达全部三个节点 |

`max_tunnels` 默认 16（§21.5）。超预算是**按节点**的失败而不是集群的失败：错误信息里有该 profile 的上限，没有「cluster」二字。

## SOCKS5 客户端自己写，不用 crate

四十行，而且它在凭证要走的那条路上。一个在代理拒绝时**静默回退直连**的客户端，会把连接——以及随后的密码——送到 profile 从未授权的地方。这个实现在每一种拒绝码上都返回错误，并且把 RFC 1928 的码翻译成人话（打印 `0x02` 的诊断是把读者送去查 RFC）。

CONNECT 用的是**域名**而不是本地解析出的地址：内网名字可能只在隧道通向的那个网络里能解析，本地解析会让这个模式失去意义。测试里的 SOCKS5 服务器断言地址类型确实是 domain。

它不提供任何认证：代理是我们自己在 loopback 上起的 `ssh -D`，向它出示凭证等于向任何抢先占了那个端口的东西出示凭证。

## fixture：跳板机 + 它背后的 cluster

三个 Redis 节点在一个网络上，一台 `sshd` 在前面，只有 bastion 被 publish。bastion 镜像的**底座就是 compatibility manifest 已经 pin 的那个 redis 7.4 digest**——少一个没人审过的第三方镜像坐在凭证路径中间，而且这个 digest 仓库本来就在反复校验。

sshd 配置成 forward-only：`ForceCommand /usr/sbin/nologin`、不许 agent forwarding、不许 X11。一台能执行命令的跳板机，被攻破时的后果比它承载的隧道更糟。

密钥对由 `up.sh` 每次现场生成，不提交、不复用。known-hosts 在 setup 时 `ssh-keyscan` 一次——**测试本身绝不 scan**，否则测的就是一个 trust-on-first-use 的客户端了。

## 一个被自己的测试推翻的前提

第一版用「宿主机连不到 cluster」来证明隧道是必要的。它在这台机器上失败了：Docker 把网桥地址路由给了宿主机，换成 `--internal` 网络也一样。**一个在这台机器成立、在那台不成立的前提，不是前提。**

改成正向证明：建立隧道、确认能 PING 通、**停掉 bastion**、断言经隧道的连接随之失败。除了「流量确实经过它」之外没有别的解释。直连可达性仍然被打印出来（这次跑的环境是「可以直连」），但不再被断言——它是环境信息，不是被测性质。

## NET-07：退出只清理自己的

进程那一半是可观察的：drop 隧道后 `ssh` 进程被杀、被回收，loopback 端口放回来——测试等着把它重新 bind 上。

「不破坏用户其他连接」那一半是 `ControlPath=none` 保证的，断言在 argv 上——我们自己进程的测试观察不到别人的会话。

## 回退条款

「某模式在某平台不可用 → 该平台该模式标 unsupported，另一模式必须可用」。`mode_available()` 做的是真实检查（跑 `ssh -V`）而不是返回常量；今天两种模式依赖同一个 `ssh`，所以答案相同，但形状已经在那里，将来某个平台丢掉其中一种时可以直说而不是在连接时才失败。

## 还没做的

- **Sentinel over SSH**：§21.5 提到 Sentinel 返回的也是内网地址，与 Cluster 同理。归 Phase 3。
- **TLS 经隧道校验内网名**：结构上已经成立——`TlsConfig` 的验证名与 TCP 目标是两个独立字段（V-G04，并有一条测试证明用地址当验证名会被拒）。缺的是一台带 TLS 的隧道后集群，归 Phase 3。
- **`ProxyCommand` 的额外信任批准流程**：目前是一律 `ProxyCommand=none`。导入配置时的批准 UI 归 Phase 2。
