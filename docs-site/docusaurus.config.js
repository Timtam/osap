// @ts-check
import {themes as prismThemes} from 'prism-react-renderer';

// Docs content lives in the repo's top-level ../docs (single source of truth,
// also referenced by code + the design notes); this site renders + versions it.
// GitHub Pages settings for github.com/Timtam/osap. baseUrl has to match the repository
// name exactly — Pages serves a project site under /<repo>/, and every absolute reference
// in the built site is written from this value, so a mismatch produces a site whose every
// stylesheet and link 404s. docs-offline.ps1 reads it from here for the same reason.

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'OS Automation Platform',
  tagline: 'Accessible OS automation — Rust + Luau modules',
  favicon: 'img/favicon.ico',

  future: {v4: true},

  // Parse .md as CommonMark (so prose like `host.<ns>` and `{ ... }` is literal,
  // not MDX/JSX); .mdx still uses MDX.
  markdown: {format: 'detect', hooks: {onBrokenMarkdownLinks: 'warn'}},

  url: 'https://timtam.github.io',
  baseUrl: '/osap/',
  organizationName: 'Timtam',
  projectName: 'osap',

  onBrokenLinks: 'throw',

  i18n: {defaultLocale: 'en', locales: ['en']},

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          path: '../docs',
          routeBasePath: '/',
          sidebarPath: './sidebars.js',
          // Keep the live (current) docs as the default version; 0.1.0 etc. are
          // archived releases reachable via the version dropdown.
          lastVersion: 'current',
          versions: {current: {label: 'current'}},
        },
        blog: false,
        theme: {customCss: './src/css/custom.css'},
      }),
    ],
  ],

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      colorMode: {respectPrefersColorScheme: true},
      navbar: {
        title: 'OS Automation Platform',
        items: [
          {type: 'docsVersionDropdown', position: 'right'},
          {
            href: 'https://github.com/Timtam/osap',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        copyright: `Documentation for the OS Automation Platform. Built with Docusaurus.`,
      },
      prism: {
        theme: prismThemes.github,
        darkTheme: prismThemes.dracula,
        additionalLanguages: ['lua', 'toml', 'rust', 'bash'],
      },
    }),
};

export default config;
