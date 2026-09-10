import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'ZzClawTerm',
  tagline: 'A desktop client for SSH-centric operations and mixed terminal workflows.',
  favicon: 'img/logo.svg',

  // Docusaurus 的 url 只能是裸域名，子路径归 baseUrl。按 GitHub Pages
  // 项目站惯例配置（https://jackfahdin.github.io/ZzClawTerm/）；将来换
  // 独立域名时把 url 换成域名、baseUrl 改回 '/'。
  url: 'https://jackfahdin.github.io',
  baseUrl: '/ZzClawTerm/',

  organizationName: 'Jackfahdin',
  projectName: 'ZzClawTerm',

  onBrokenLinks: 'throw',
  onBrokenAnchors: 'ignore',

  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
  },

  i18n: {
    defaultLocale: 'zh-CN',
    locales: ['zh-CN', 'en'],
    localeConfigs: {
      'zh-CN': {
        label: '简体中文',
        direction: 'ltr',
      },
      en: {
        label: 'English',
        direction: 'ltr',
      },
    },
  },

  presets: [
    [
      'classic',
      {
        docs: {
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/jackfahdin/ZzClawTerm/edit/master/docs-site/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  // 站点统计（umami）暂未启用：上游配置指向上游作者的实例
  // （umami.coderkang.top）。将来有自己的 umami 实例时，恢复本段并替换
  // websiteID 与 dataHostURL。
  // plugins: [
  //   [
  //     "./src/plugins/umami/index.ts",
  //     {
  //       websiteID: "<your-website-id>",
  //       dataHostURL: "<your-umami-host>",
  //     },
  //   ],
  // ],

  themes: [
    [
      "@easyops-cn/docusaurus-search-local",
      {
        hashed: true,
        language: ['en', 'zh'],
        indexBlog: false,
        docsRouteBasePath: '/docs',
        highlightSearchTermsOnTargetPage: true,
        explicitSearchResultPath: true,
      },
    ],
  ],

  themeConfig: {
    colorMode: {
      defaultMode: 'dark',
      disableSwitch: false,
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'ZzClawTerm',
      logo: {
        alt: 'ZzClawTerm Logo',
        src: 'img/logo.svg',
      },
      items: [
        {
          to: '/#features',
          position: 'left',
          label: '功能',
          className: 'navbar__center-link navbar__center-link--features',
          activeBaseRegex: 'a^',
        },
        {
          to: '/docs/',
          position: 'left',
          label: '文档',
          className: 'navbar__center-link navbar__center-link--docs',
        },
        {
          to: '/changelog',
          position: 'left',
          label: '日志',
          className: 'navbar__center-link navbar__center-link--changelog',
        },
        {
          type: 'localeDropdown',
          position: 'right',
        },
        {
          href: 'https://github.com/jackfahdin/ZzClawTerm',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: '文档',
          items: [
            {
              label: '快速开始',
              to: '/docs/getting-started/installation',
            },
            {
              label: '使用指南',
              to: '/docs/guide/ssh-connection',
            },
          ],
        },
        {
          title: '开发',
          items: [
            {
              label: '架构说明',
              to: '/docs/development/architecture',
            },
            {
              label: '贡献指南',
              to: '/docs/development/contributing',
            },
          ],
        },
        {
          title: '更多',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/jackfahdin/ZzClawTerm',
            },
            {
              label: '问题反馈',
              href: 'https://github.com/jackfahdin/ZzClawTerm/issues',
            },
          ],
        },
      ],
      copyright: `Copyright &copy; ${new Date().getFullYear()} Jackfahdin. Built with Docusaurus.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'toml', 'bash', 'json'],
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
