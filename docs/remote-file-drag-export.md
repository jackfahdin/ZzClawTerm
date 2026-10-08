# 文件／目录拖拽下载

远程拖出支持 Windows 和 macOS，要求来源连接使用 SFTP。Linux 的列表、树及
resolver 均关闭远程拖出；请使用普通“下载”。本地文件和目录仍通过真实路径／
URI 拖出。应用已移除远程暂存状态、准备作业和二次拖拽流程，不改 GPUI Linux 原生实现。

| 平台 | 流程 | 队列及目标语义 |
| --- | --- | --- |
| Windows | OLE worker 枚举描述符，Drop 后按需读取 SFTP 内容 | 每个选中的顶层文件／目录一行；Explorer 处理目标和重名 |
| macOS | Finder promise 给出最终完整 URL，后台递归下载 | 每个原生请求一个下载任务；失败保留已提交部分，允许安全恢复 |
| Linux | 远程使用普通下载，本地保留 URI 拖出 | 不产生远程暂存下载 |

## 目标保护和重试

目标解析冻结 `CreateNew`／`ReplaceKnownTarget` 写入授权。新文件独占创建；
覆盖已知文件时通过身份校验后的同一打开句柄写回，保留权限与 inode 语义。
目标后来出现或被替换不会自动获得覆盖权限，最终组件不跟随链接或 reparse point。

promised download 固定来源类型和冲突契约，只复用本任务创建且身份匹配的目录。
已提交文件记录本地身份、大小、修改时间及远端 raw path、大小、mtime：本地
外部修改／替换返回冲突；远端元数据未变时保留完整文件；缺少远端稳定元数据
或远端已变化则重新下载任务自有文件。失败文件不提交或拼接来源不明的断点前缀。

队列重试注册表保存原来源 Weak、raw path、完整路径选项和目标所有权记录。
切换标签、更改全局策略或重连不能把恢复操作转到另一来源。原来源关闭时提示
重新拖拽。成功或清除任务释放记录。队列手动重试是独立恢复操作，不调用旧
Finder completion；每个原生请求的 completion 对成功、失败及 panic 恰好调用一次。
普通下载保留 Overwrite／Skip／Rename／Ask 及已有断点策略。

目录不是整棵树的原子事务：macOS 中途失败后已提交文件保留。所选下载根目录
及祖先须由用户控制；路径身份复查不能为任意并发目录重命名提供全路径原子事务。
远端元数据检查也不是远端内容快照。

## 受控传输和聚合

Windows 枚举复用一次受控 SFTP 会话，根条目重新 lstat，子项使用本次原始目录
列表的轻量属性；不做 UID/GID、getent、链接目标查询或内容预读。保留 raw path、
顶层标识、父目录优先顺序和空目录，限制 1024 个根、128 层及 65536 个条目。
实际打开内容继续校验类型和大小。

每个来源同时最多三个拖拽 SFTP 操作，兼容模式为一个。permit、兼容 gate、属性
和目录请求均可取消；控制轮询 25ms，单次请求超时 60 秒，资源清理有界。
空闲原生流完成有界预读后释放 permit／gate，再次读取时重新获取，防止保留原流
并读取 Clone 或其他文件时互相等待；消费预读失败保留错误类型，超时／关闭会话
在释放兼容 gate 前清除缓存。顶层队列控制同时作用于枚举，各项取消互不改写来源。
普通下载调度不使用拖拽预算，文件内部并发沿用传输设置。每次手势共用一个异步
来源生命周期监测任务，来源消失取消所属控制；macOS 全应用共用一个最多三项
并发的 NSOperationQueue，避免为多选创建大量阻塞队列。

Windows 顶层控制暂停／取消已打开及尚未打开的子项，单个流有独立子控制。
释放一个流不取消目录。按远端路径身份合并字节区间，Seek、Clone 和重复打开不
重复累计字节或完成文件数；读取错误保留。原生生命周期覆盖开始、放下及终态，
EndOperation 或同步最终释放只报告一次。聚合等待已注册读取清理后结束；目标
未请求的子文件不算读取失败，空目录也能完成。

完成文案为“已向目标提供内容”，不声称 Explorer 最终落盘成功，不提供虚构目标
路径或 Windows 原生重试。空闲超时不计入读取、枚举和用户暂停；窗口关闭、主动
取消及网络请求超时仍有效。Windows 聚合及 macOS 进度每任务按 50ms 节流，
最终进度强制一次；UI 在既有 16ms 合并窗口按任务保留最新进度，生命周期立即处理。

## 依赖和验证

Zed 补丁位于 `nyakang/zed` 的 `zzclawterm` 分支，契约、Windows 生命周期和 macOS
共享队列分别提交；最终 pin 为 `400d43cd62b6499c77a1400d6f83116d9845e187`。
gpui-kit 所有 Zed revision 同步，pin 检查通过；Kit pin 为
`c4f044e6a140a8320ed90192fd0508e91da20855`。补丁原因和验证记录在各自 `ZZCLAWTERM.md`。
不编辑 `temp/vendor` 或 Cargo 缓存源码，不更新无关 registry 版本。
配置、数据库、凭据、备份及同步格式均未变。

本地 Windows 验证：transport 432 项通过、2 项单元测试忽略；真实 SFTP 环境集成
测试 1 项因未配置服务忽略。新增覆盖目标竞态、
任务所有权、目录自动／手动恢复、阻塞请求取消、gate／permit 取消与释放、并发上限，
以及万级文件枚举无内容读取／无逐子项 stat。Windows GPUI 原生拖拽套件 13 项通过。
主仓库 desktop 聚合、来源固定、节流回归包含在 workspace 串行测试中。
`cargo test --workspace --locked -- --test-threads=1` 共 3498 项通过、0 项失败、
15 项忽略（包含需真实服务／交互环境的测试及手动性能测量）。
最终源码的 `cargo check --workspace --locked`、
`cargo clippy --workspace --all-targets --locked`、`cargo fmt --all -- --check`
及 `git diff --check` 均通过；workspace 检查、测试和 Clippy 无编译警告。

最终 revision 的 Windows、macOS、Linux 原生编译及 Windows 拖拽、macOS promise
ABI 测试已通过：[CI 37622541491](https://github.com/nyakang/zed/actions/runs/37622541491)。

仍需真实 Windows Explorer／macOS Finder 实测多选、重名、取消、长期暂停、
目录恢复与大文件哈希；Linux 列表／树远程入口及本地 URI 拖出仍需 Linux 实机回归。
原生编译、COM／Objective-C dispatch 和模拟 SFTP 测试不能替代这些交互验收。
