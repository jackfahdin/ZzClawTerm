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
