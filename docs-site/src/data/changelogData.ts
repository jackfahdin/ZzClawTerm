export type ChangelogSection = {
  title: string;
  items: string[];
};

export type ChangelogRelease = {
  version: string;
  sections: ChangelogSection[];
};

const changelogReleasesEn: ChangelogRelease[] = [
  {
    version: '[0.1.6] - 2026-10-25',
    sections: [
      {
        title: 'Added',
        items: [
          '**sync:** Sync upstream NyaTerm (29 commits): connection editor save flow, WSL command construction and working directory handling, bulk admission and queuing for transfer jobs, clipboard input handling, terminal selection text management, AI system-prompt context, cloud sync snapshot heads, SFTP directory downloads, terminal execution recording, ping sequence counters and numeric-pattern semantic highlighting, minimum contrast ratio, Windows application version resources, macOS app icon and title-bar double-click resize, and more.',
          '**settings:** X11 gains clipboard / PRIMARY selection, native OpenGL, and DPI launch options; Windows IPv4 listen addresses are queried in the background, and copyable current / next-launch arguments are shown.',
        ],
      },
      {
        title: 'Fixed',
        items: [
          '**connections:** Selecting a shell preset in the local terminal editor now fills the path input immediately instead of leaving it blank until save.',
          '**connections:** New local terminal connections are named after the selected shell (localized preset names; custom paths use the file name without `.exe`) instead of always "Local Terminal".',
          '**desktop:** Dismiss the command suggestion popup on left-click outside it or when the terminal loses focus.',
          '**ci:** Handle empty Gitee Release query results that broke release syncing.',
        ],
      },
    ],
  },
  {
    version: '[0.1.5] - 2026-10-21',
    sections: [
      {
        title: 'Added',
        items: [
          '**sync:** Sync upstream NyaTerm (40 commits): dual-pane file manager, logical line numbers for terminal snapshots, WindTerm Dark/Light themes, settings panel visibility management with a remote monitoring tab, per-connection username source handling, session retry with tab placement, tab close buttons, built-in editor font size, document language recognition, and more.',
          '**settings:** New Settings → Terminal → X11 tab: a centered X11 icon shows the server state (accent while running, gray otherwise; hover for status, display, and path details); local DISPLAY and X server path settings moved here.',
          '**settings:** X server path becomes a dropdown ("ZzXsrv (default)" auto-discovery / "Custom" with a native browse button), validated to require an existing vcxsrv.exe before settings can be applied.',
          '**settings:** New "Show X server tray icon" toggle (on by default; launches new instances with `-notrayicon` when off); a red "restart the X server to take effect" hint appears while a running managed instance disagrees with the toggle.',
        ],
      },
      {
        title: 'Changed',
        items: [
          '**terminal:** X server management collapses to a single master toggle "Enable X server" (MobaXterm semantics, on by default): started with the app, stopped on exit, and revived automatically by the next X11-forwarded connection if it dies; the separate "start on demand" toggle is removed and is now always-on internal behavior.',
          '**terminal:** The "Enable X server" toggle takes effect immediately: enabling starts the server in the background, disabling stops instances started by the app (external servers are untouched).',
          '**settings:** The "Local X11 DISPLAY" description now states its meaning plainly (the local display SSH X11 forwarding connects back to; port = 6000 + display number; the default localhost:0 fits the managed ZzXsrv).',
          '**ui:** Remove the selection indicator bar from sidebar buttons.',
        ],
      },
      {
        title: 'Fixed',
        items: [
          '**desktop:** Dismiss the command suggestion popup on right-click outside it or when the terminal loses focus.',
          '**build:** Bundled ZzXsrv updated to 21.1.16.2, fixing the console window flash caused by xkbcomp running through cmd.exe at X server startup.',
        ],
      },
    ],
  },
  {
    version: '[0.1.4] - 2026-10-19',
    sections: [
      {
        title: 'Added',
        items: [
          '**sync:** Sync upstream NyaTerm (82 commits): custom theme management with CSS color parsing, theme editor contrast handling, tab drag reordering and overflow scrolling, terminal wheel zoom, Telnet keep-alive configuration, remote desktop key bindings and upload resource management, cooperative cloud sync cancellation with unreadable-snapshot recovery, transfer progress reporting, editor save permission preservation, Windows Terminal settings compatibility (jsonc-parser), Alt-as-meta handling, scroll history state management, and more.',
          '**build:** All forked dependencies (including portable-pty) now resolve to GitCode mirrors; no direct nyakang repository references remain.',
        ],
      },
      {
        title: 'Fixed',
        items: [
          '**ui:** Fix connection icon popover backdrop bleed-through and list scrolling, plus assorted tab and menu interaction issues (via upstream sync).',
        ],
      },
    ],
  },
  {
    version: '[0.1.3] - 2026-10-09',
    sections: [
      {
        title: 'Added',
        items: [
          '**update:** New Gitee update source; automatic mode checks and downloads in Gitee → GitCode → GitHub order.',
          '**sync:** Sync upstream NyaTerm (15 commits): quick command category drag-and-drop with the "all" category as drop target, terminal rendering performance (glyph support, styled-span rendering rework), held `*`/`**` flush after 50ms output idle, session text scrollback limit, whitespace preservation for connection passwords, cloud sync snapshot secret decryption rework, restored v1 icon palette and server glyph, non-ASCII font family name rendering fixes, and more.',
        ],
      },
      {
        title: 'Changed',
        items: [
          '**ci:** GitCode/Gitee release attachments are no longer uploaded by CI (unstable cross-border path); only code and tags are mirrored. Attachments are mirrored from a domestic network with `scripts/release/mirror_gitee.py`.',
        ],
      },
      {
        title: 'Fixed',
        items: [
          '**app:** Close single-instance activation connections gracefully after answering so the peer (notably on macOS) reliably receives the ACK.',
        ],
      },
    ],
  },
  {
    version: '[0.1.2] - 2026-10-08',
    sections: [
      {
        title: 'Added',
        items: [
          '**sync:** Sync upstream NyaTerm through v2.0.0-preview.5 (82 commits): V1 plugin system, Hex editor, terminal tab drag, transfer drag-and-drop import/export with batch deletion, character encoding support, custom serial baud rates, terminal command navigation and semantic keyword highlighting, quick command categories, collapsible connection folders, and more.',
          '**terminal:** Connection password autofill with confirmation, and input line resynchronization for Tab completion.',
          '**ai:** AI agent JSON protocol, execution policy and history management enhancements, and global proxy settings.',
          '**remote-desktop:** RA2_256 server trust verification for RDP.',
          '**recording:** Session recording rekeying and disconnection handling.',
        ],
      },
      {
        title: 'Changed',
        items: [
          '**release:** Release notes now automatically include the curated Chinese changelog section.',
        ],
      },
    ],
  },
  {
    version: '[0.0.1] - 2026-09-08',
    sections: [
      {
        title: 'Added',
        items: [
          '**release:** First ZzClawTerm release. The project is forked from [NyaTerm](https://github.com/nyakang/nyaterm) and ships with SSH, local shell, Telnet, Serial, RDP, VNC, SFTP, tunnels, OTP, AI assistance, and encrypted sync and backup in one workspace.',
        ],
      },
    ],
  },
];

const changelogReleasesZhCN: ChangelogRelease[] = [
  {
    version: '[0.1.6] - 2026-10-25',
    sections: [
      {
        title: '新增',
        items: [
          '**sync:** 同步上游 NyaTerm（29 个提交）：连接编辑器保存功能完善、WSL 命令构造与工作目录处理、传输任务批量准入与排队、剪贴板输入处理、终端选择文本管理、AI 请求系统提示上下文、云同步快照 heads 管理、SFTP 目录下载增强、终端执行录制增强、ping 序列计数与数值模式语义高亮、对比度下限调整、Windows 应用版本资源、macOS 应用图标与标题栏双击缩放等。',
          '**settings:** X11 增加剪贴板 / PRIMARY 选择、原生 OpenGL、DPI 等启动参数设置；后台查询 Windows IPv4 监听地址，展示可复制的当前及下次启动参数。',
        ],
      },
      {
        title: '修复',
        items: [
          '**connections:** 本地终端编辑器从下拉选择 shell 预设（PowerShell / 命令提示符 / Bash / WSL / Windows Terminal）后，路径输入框同步显示对应程序，不再停留空白。',
          '**connections:** 新建本地终端连接的默认名按所选 shell 派生（预设用本地化名称，自定义路径取文件名去 `.exe`），不再一律叫 "Local Terminal"。',
          '**desktop:** 左键点击建议框外部或终端失焦时，正确关闭命令建议弹窗。',
          '**ci:** 修复 Gitee Release 查询返回空值导致的同步失败。',
        ],
      },
    ],
  },
  {
    version: '[0.1.5] - 2026-10-21',
    sections: [
      {
        title: '新增',
        items: [
          '**sync:** 同步上游 NyaTerm（40 个提交）：双栏文件管理器、终端快照逻辑行号管理、WindTerm Dark/Light 主题、设置面板可见性管理与远程监控子页、连接用户名来源处理、会话重试与标签页放置、标签页关闭按钮、内置编辑器字体大小配置、文档语言识别等。',
          '**settings:** 新增「设置 → 终端 → X11」子页：状态区以居中 X11 大图标着色显示运行状态（运行中高亮、未运行灰色，悬浮图标查看运行状态、Display、程序路径等详情）；本地 DISPLAY 地址与 X Server 路径配置迁入该页。',
          '**settings:** X Server 路径改为下拉选择（「ZzXsrv（默认）」自动探测 /「自定义」），自定义模式提供原生文件浏览按钮，并对路径做有效性校验（须为存在的 vcxsrv.exe，否则阻止应用设置）。',
          '**settings:** 新增「显示 X Server 托盘图标」开关（默认开启，关闭时以 `-notrayicon` 启动新实例）；当运行中的托管实例与该开关不一致时，开关下方显示红色「重启 X Server 后生效」警示。',
        ],
      },
      {
        title: '变更',
        items: [
          '**terminal:** X Server 管理简化为单一总开关「启用 X Server」（MobaXterm 同款语义，默认开启）：随应用启动拉起、退出时联动关闭，运行中若服务器意外退出，下一个勾选 X11 转发的连接会自动兜底拉起；原「连接时按需启动」独立开关移除，降级为内部恒生效行为。',
          '**terminal:** 「启用 X Server」总开关立即生效：开启即后台启动 X Server，关闭即停止由本应用拉起的实例（外部服务器不受影响），不再等下次启动。',
          '**settings:** 「本地 X11 DISPLAY」说明文案改为明确其语义（SSH X11 转发回连的本地 display，端口 = 6000 + display 号，默认 localhost:0 适用于托管 ZzXsrv）。',
          '**ui:** 移除侧边栏按钮的选中指示条。',
        ],
      },
      {
        title: '修复',
        items: [
          '**desktop:** 右键点击建议框外部或终端失焦时，正确关闭命令建议弹窗。',
          '**build:** 随包分发的 ZzXsrv 升级至 21.1.16.2，修复启动 X Server 时 xkbcomp 经 cmd.exe 执行导致的控制台窗口闪烁。',
        ],
      },
    ],
  },
  {
    version: '[0.1.4] - 2026-10-19',
    sections: [
      {
        title: '新增',
        items: [
          '**sync:** 同步上游 NyaTerm（82 个提交）：自定义主题管理与 CSS 颜色解析、主题编辑器对比度处理、标签页拖动排序与溢出滚动、终端滚轮缩放、Telnet 保活间隔配置、远程桌面按键绑定与上传资源管理、云同步协作取消与损坏快照诊断恢复、传输进度上报、编辑器保存权限保留、Windows Terminal 设置兼容（jsonc-parser）、Alt-as-meta 键处理、滚动历史状态管理等。',
          '**build:** 全部 fork 依赖（含新增的 portable-pty）统一指向 GitCode 镜像，不再直接依赖 nyakang 的仓库。',
        ],
      },
      {
        title: '修复',
        items: [
          '**ui:** 修复连接图标弹层背景透出与列表滚动、标签页与菜单的若干交互问题（随上游同步）。',
        ],
      },
    ],
  },
  {
    version: '[0.1.3] - 2026-10-09',
    sections: [
      {
        title: '新增',
        items: [
          '**update:** 新增 Gitee 更新来源，自动模式按 Gitee → GitCode → GitHub 顺序检查更新与下载安装包。',
          '**sync:** 同步上游 NyaTerm（15 个提交）：快捷命令分类拖拽与「全部」分类落点、终端渲染性能优化（glyph 支持、样式 span 渲染重构）、终端输出 `*`/`**` 空闲 50ms 自动补写、会话文本滚动缓冲限制、连接密码保存/读取保留空白字符、云同步快照密钥解密重构、v1 图标调色板与服务器图标恢复、非 ASCII 字体名渲染修复等。',
        ],
      },
      {
        title: '变更',
        items: [
          '**ci:** 取消 GitCode/Gitee 的 CI 大附件上传（跨境链路不稳定），仅保留代码与 tag 同步；Release 附件改由本机脚本 `scripts/release/mirror_gitee.py` 在国内网络镜像。',
        ],
      },
      {
        title: '修复',
        items: [
          '**app:** 单实例激活连接在应答后优雅关闭，避免对端（尤其是 macOS）偶发收不到 ACK。',
        ],
      },
    ],
  },
  {
    version: '[0.1.2] - 2026-10-08',
    sections: [
      {
        title: '新增',
        items: [
          '**sync:** 同步上游 NyaTerm 至 v2.0.0-preview.5（82 个提交）：V1 插件系统、Hex 编辑器、终端标签页拖拽、传输文件拖拽导入/导出与批量删除、字符编码支持、串口自定义波特率、终端命令导航与关键词语义高亮、快捷命令分类管理、连接资产文件夹折叠等。',
          '**terminal:** 连接密码凭据自动填充（填充前需确认），终端输入行 Tab 补全重同步。',
          '**ai:** AI 代理 JSON 协议、执行策略与历史管理增强，全局代理设置。',
          '**remote-desktop:** RDP 支持 RA2_256 服务器证书校验。',
          '**recording:** 会话录制支持 rekeying 与断连处理。',
        ],
      },
      {
        title: '变更',
        items: [
          '**release:** Release 描述自动附带人工维护的中文更新日志。',
        ],
      },
    ],
  },
  {
    version: '[0.0.1] - 2026-09-08',
    sections: [
      {
        title: '新增',
        items: [
          '**release:** ZzClawTerm 首个版本。项目 fork 自 [NyaTerm](https://github.com/nyakang/nyaterm)，在一个工作区中提供 SSH、本地终端、Telnet、串口、RDP、VNC、SFTP、隧道、OTP、AI 辅助以及加密同步与备份。',
        ],
      },
    ],
  },
];

const changelogReleasesByLocale: Record<string, ChangelogRelease[]> = {
  en: changelogReleasesEn,
  'zh-CN': changelogReleasesZhCN,
};

export function getChangelogReleases(locale: string): ChangelogRelease[] {
  return changelogReleasesByLocale[locale] ?? changelogReleasesByLocale['zh-CN'];
}
