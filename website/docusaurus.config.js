// @ts-check

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'coralX',
  tagline: 'Modern coral reef benthic point count analysis — cross-platform, open-source, AI-powered.',
  favicon: 'img/favicon.ico',

  url: 'https://padreon.github.io',
  baseUrl: '/coralX/',

  organizationName: 'padreon',
  projectName: 'coralX',

  onBrokenLinks: 'warn',
  onBrokenMarkdownLinks: 'warn',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: './sidebars.js',
          editUrl: 'https://github.com/padreon/coralX/edit/main/website/',
          routeBasePath: '/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      image: 'img/coralx-social.png',
      colorMode: {
        defaultMode: 'dark',
        disableSwitch: false,
        respectPrefersColorScheme: true,
      },
      navbar: {
        title: 'coralX',
        logo: {
          alt: 'coralX Logo',
          src: 'img/logo.svg',
        },
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'docsSidebar',
            position: 'left',
            label: 'Docs',
          },
          {
            href: 'https://github.com/padreon/coralX',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Docs',
            items: [
              { label: 'User Guide', to: '/user-guide' },
              { label: 'Training Guide', to: '/training-guide' },
              { label: 'Contributing', to: '/contributing' },
            ],
          },
          {
            title: 'Links',
            items: [
              {
                label: 'GitHub',
                href: 'https://github.com/padreon/coralX',
              },
              {
                label: 'Report an Issue',
                href: 'https://github.com/padreon/coralX/issues',
              },
            ],
          },
        ],
        copyright: `Copyright © ${new Date().getFullYear()} coralX. Built with Docusaurus.`,
      },
      prism: {
        theme: require('prism-react-renderer').themes.github,
        darkTheme: require('prism-react-renderer').themes.dracula,
        additionalLanguages: ['python', 'bash'],
      },
    }),
};

module.exports = config;
