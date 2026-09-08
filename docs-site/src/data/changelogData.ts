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
