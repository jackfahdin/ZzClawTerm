# SSH 端口转发设计（本地/远程/动态）

日期：2026-08-21
状态：已与用户逐节确认（范围 / 生命周期 / UI 边界 / 架构方案）
关联：`docs/superpowers/specs/2026-08-17-zzclawterm-v0.1-design.md` §十二（v0.2 概要）

## 一、目标

- 三种转发全做：本地 -L、远程 -R、动态 -D（SOCKS5），对标 MobaXterm
- 转发规则绑定会话 profile：连接成功后自动启动，断线随会话销毁，重连自动重建
- 单条规则失败不影响会话与其余规则
- 泵改造不得拖慢 shell 终端（延迟回归 ≤5%）

## 二、已确认的关键决策

| 决策点 | 结论 |
|---|---|
| 范围 | 三种全做（-L / -R / -D） |
| 生命周期 | 随会话自动重建（断线销毁、重连恢复） |
| UI 边界 | 规则编辑（会话编辑对话框加 QTableWidget）+ 活动状态（状态栏/会话菜单），不动 ZzPureToolsPro |
| 泵架构 | 方案 A：worker 线程统一调度多 channel，入口 socket 在 GUI 线程，数据走 queued 管道 |
| SOCKS5 | RFC1928 无认证子集，解析器独立小类纯函数可单测 |
| 配置存储 | sessions.json 加 `portForwards` 字段，version 仍为 1（缺失字段默认空列表，旧文件兼容） |

## 三、架构与组件

**ZzSshCore 侧（仓库 /home/zz/Jackfahdin/github/ZzSshCore）**

```text
ZzSshConnection          [改] 新增转发入口（createTunnel 类 API，queued 到 worker）
ZzSshConnectionWorker    [改] 读取泵按 block directions 调度多 channel
                           （替换 worker.h:24 "v0.1 每连接只开一个 shell channel" 的假设）
ZzSshChannel             [改] 新增 openDirectTcpip(targetHost, targetPort, originator)
ZzSshForwardListener     [新] 远程转发：channel_forward_listen/accept/cancel 封装
ZzSshTunnel              [新] GUI 线程对象：一条转发规则的运行时实体（与 shell channel 平级）
```

**ZzClawTerm 侧（主仓库）**

```text
ZzForwardRule            [新] 值类型：类型(Local/Remote/Dynamic) + 监听地址端口 + 目标地址端口
ZzSessionProfile         [改] 加 portForwards 字段
ZzTunnelManager          [新] 每个活动会话的隧道集合：连接后建隧道、断线销毁、重连重建
ZzTransportEndpoint      [改] 加 portForwards 字段，ZzTabManager::endpointFor() 负责映射
ZzSshTransportAdapter    [改] 连接成功后驱动 ZzTunnelManager
ZzSessionEditDialog      [改] 加"端口转发"规则表（QTableWidget + 增删按钮 + 即时校验）
状态栏/会话菜单          [改] 活动隧道数与失败规则提示
```

线程模型不变：每 SSH 连接一个 worker QThread；本地监听 QTcpServer 在 GUI 线程，接受连接后数据经 queued 信号槽进出 worker。

## 四、三种转发数据流

**本地（-L）**：本地 App → QTcpServer(GUI 线程) → [queued] → worker → direct-tcpip channel → SSH 服务端 → 目标。每个已接受连接一个 channel，连接关闭即 channel 关闭。

**远程（-R）**：远程 App → SSH 服务端监听端口 → forwarded-tcpip channel 推入 → worker 在泵周期内 forward_accept → [queued] → GUI 线程 QTcpSocket → 本地目标。会话结束时 forward_cancel。

**动态（-D）**：本地 SOCKS5 服务（入口同 -L），RFC1928 无认证子集：握手 → 解析目标 → direct-tcpip → 与 -L 相同的数据泵。`ZzSocks5Handshake` 解析器独立纯函数式小类。

**统一组件 `ZzSocketChannelPump`**：一个 QTcpSocket 与一个 channel 之间的双向搬运工（readyRead→写 channel 节流；channel 数据→写 socket 带背压）。三种转发复用同一泵，只有入口与 channel 打开方式不同。

**背压与上限**：单连接 socket 写缓冲水位 1MB（超限暂停对端读取）；单隧道最大并发连接 256；监听绑定失败只影响该规则。

## 五、配置格式

```json
{
  "portForwards": [
    { "type": "local",   "listenHost": "127.0.0.1", "listenPort": 13306, "targetHost": "db.internal", "targetPort": 3306 },
    { "type": "remote",  "listenHost": "0.0.0.0",   "listenPort": 8080,  "targetHost": "127.0.0.1",   "targetPort": 3000 },
    { "type": "dynamic", "listenHost": "127.0.0.1", "listenPort": 1080 }
  ]
}
```

校验：端口 1-65535；local/remote 必须有目标地址；dynamic 无目标；同 (type, listenHost, listenPort) 不允许重复；编辑对话框即时校验，非法规则禁止保存。

## 六、错误处理矩阵

| 场景 | 行为 |
|---|---|
| 监听端口被占用 | 该规则标记失败（状态栏提示），其余规则与会话不受影响 |
| 服务端拒绝转发（AllowTcpForwarding off） | remote 规则失败提示，会话保留 |
| 会话断线 | 全部隧道销毁、监听释放；重连成功后自动重建 |
| 单个转发连接出错（目标不可达等） | 只关该连接，隧道继续监听 |
| SOCKS5 协议错误 | 按 RFC1928 回错误码后关闭该连接 |

## 七、测试

- ZzSshCore：单元测试复用 ZzMockTransport 基建；集成测试复用 Docker openssh-server（按需调整 Dockerfile 的 AllowTcpForwarding/GatewayPorts 配置），三种转发各起真实 TCP 回环验证双向贯通
- ZzClawTerm：ZzForwardRule 序列化往返与校验、ZzTunnelManager 生命周期（连接→建隧道→断线→销毁→重连→重建，mock transport）、SOCKS5 解析器单元测试（合法/畸形/截断）
- 沿用既有约定：QTest、每测试类一个 ctest 用例、性能测试打 perf 标签

## 八、性能门控（Release，落 tests/perf/records/，不达标不验收）

| 测试 | 阈值 | 说明 |
|---|---|---|
| 本地转发吞吐 | ≥ 50MB/s | Docker 容器回环，单连接 |
| 转发建立延迟 | ≤ 200ms | 连接成功后隧道就绪时间 |
| 256 并发连接转发 | 无泄漏、无错数据 | 资源上限路径 |
| shell+转发并行回归 | shell 侧不回退超 5% | 泵改造不得拖慢终端；口径 = ZzSshCore 既有 perf 基线的连接耗时与吞吐两项 |

## 九、范围边界（明确不做）

- 跳板机/ProxyJump 链式转发（后续版本）
- X11 转发（独立大功能，v0.3）
- 转发规则的导入导出、全局（非会话绑定）独立隧道
- 跨会话搜索式隧道管理面板（等 ZzPureToolsPro）

## 十、项目硬约束（两仓库通用）

- C++20 / Qt 6.8+ / CMake 3.25+；类名 Zz 前缀；文件名与类名严格一致；Doxygen 简体中文注释
- commit message：Conventional Commits 前缀 + 首行中文简述 + 空行 + 中文详细说明
- ZzSshCore 改动在其独立仓库执行并推送前先经用户确认；主仓库 gitlink 随后更新
