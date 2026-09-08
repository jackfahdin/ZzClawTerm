import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docs: [
    'intro',
    'sponsor',
    {
      type: 'category',
      label: '快速开始',
      items: [
        'getting-started/installation',
        'getting-started/quick-start',
      ],
    },
    {
      type: 'category',
      label: '使用指南',
      items: [
        'guide/session-types',
        'guide/ssh-connection',
        'guide/rdp',
        'guide/vnc',
        'guide/layout-and-workspace',
        'guide/terminal',
        'guide/remote-monitoring',
        'guide/file-transfer',
        'guide/quick-commands',
        'guide/ai-assistant',
        'guide/tunnels-and-proxy',
        'guide/otp-and-auth',
        'guide/security',
        'guide/themes',
        'guide/translation',
        'guide/sync-and-backup',
        'guide/keyboard-shortcuts',
      ],
    },
    {
      type: 'category',
      label: '开发文档',
      items: [
        'development/architecture',
        'development/setup',
        'development/frontend',
        'development/backend',
        'development/contributing',
      ],
    },
    'faq',
  ],
};

export default sidebars;
