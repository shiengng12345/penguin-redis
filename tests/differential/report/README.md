# Differential 比对报告（V-A02，v2.1 §32.2、附录 B.2）

由 `cargo run -p differential` 生成，逐 case 一个 JSON。**不要手改**——`ci/check-differential.sh` 会重跑并逐字节比对。

## 为什么是两台服务器

§32.2 写得很直白：同一个写命令，官方与 Penguin 必须用两个独立但相同初态的实例。在同一实例上先后跑两次 `INCR` 再比结果，看起来像差异测试，其实测的是执行顺序。

## 四层

| 层 | 比什么 | 怎么取到的 |
|---|---|---|
| 1 | argv 字节 | 录制代理记下客户端→服务器的原始字节，再解成命令序列 |
| 2 | 响应字节 | 同一个代理记下服务器→客户端的原始字节，逐字节比 |
| 3 | 最终状态 | 事后用**同一个** reader 从两台服务器读回来——这一层测的是服务器，不是客户端 |
| 4 | 输出与 exit code | 两个进程的 stdout / stderr / 退出码 |

1～3 层必须完全一致。第 4 层允许不同，但**差异必须在 case 里事先写明**，否则明天冒出来的新差异只会被耸肩放过。

## 结果

| case | 场景 | 通过标准 | 层1 | 层2 | 层3 | 层4 |
|---|---|---|---|---|---|---|
| CMD-01 | `SET diff:cmd01 --raw` | `--raw` 原样作为 value | 一致 | 一致 | 一致 | 一致 |
| CMD-02 | `set diff:CMD02:MixedKey VaLuE` | 只识别命令，不修改 key/value | 一致 | 一致 | 一致 | 一致 |
| CMD-03 | `MGET diff:cmd03 diff:cmd03 diff:cmd03:absent` | 保留重复、顺序、nil | 一致 | 一致 | 一致 | 一致 |
| CMD-04 | `HMGET diff:cmd04 empty absent` | nil 与 empty string 明确区分 | 一致 | 一致 | 一致 | 一致 |
| CMD-05 | `HSET diff:cmd05 status SUSPENDED` | 不显示失败，不捏造修改数 | 一致 | 一致 | 一致 | 一致 |

服务器镜像：`redis@sha256:71da9275c5f3fcb97d0fa0c8c5b36cc995327265420f17a04bfd544f458059f7`；baseline：`redis-cli 7.4.11`（同一镜像内的 `redis-cli`，不是开发机 PATH 上的那个）。
