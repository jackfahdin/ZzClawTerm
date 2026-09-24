# 更新来源选择设计

正式版更新对话框提供自动、GitCode、GitHub 三种来源。默认自动模式先从 GitCode 读取更新清单，并限时比较 GitHub 版本；GitCode 固定入口不可用或落后时改用 GitHub。显式选择某一来源只使用该来源，切换后重新检查。预览版保持 GitHub 更新流程，直到每日构建也镜像到 GitCode。

GitCode 没有可直接读取为 JSON 的 `releases/latest/download/latest.json`：该 URL 返回网页。GitHub Release 成功、GitCode 对应发行版的更新附件与版本清单全部同步后，GitCode 同步工作流维护一个固定的稳定通道 Release，并替换其中的 `latest.json`。客户端从此固定地址检查更新，再从版本 tag 的 GitCode Release 读取版本固定的清单和安装包。

清单仍由 GitHub 构建、签名，内含 GitHub URL。客户端先验证清单中的版本、平台和规范 GitHub URL，再仅按已验证的版本与文件名构造 GitCode URL。安装前继续使用现有 minisign 公钥验证下载的完整字节。自动模式下载失败时只尝试同版本、同文件名的 GitHub 安装包；显式 GitCode 模式不切换来源。

来源选择在当前应用进程内生效；启动时恢复为自动。下载进行中或等待安装时不可切换来源，以免更新状态与下载包不一致。更新对话框只保留一个安装命令，并显示当前实际使用的来源。
