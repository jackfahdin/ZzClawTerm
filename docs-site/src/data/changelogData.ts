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
