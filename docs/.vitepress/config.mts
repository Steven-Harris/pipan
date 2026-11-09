import { defineConfig } from 'vitepress';

export default defineConfig({
  title: 'PiPan Documentation',
  description: 'Desktop GUI and orchestration workspace for the pi AI coding agent',
  base: process.env.BASE_PATH || '/pipan/',
  themeConfig: {
    nav: [
      { text: 'Guide', link: '/guide/getting-started' },
      { text: 'Architecture', link: '/guide/architecture' },
      { text: 'Configuration', link: '/guide/configuration' },
      { text: 'Reference', link: '/guide/extensions#manifest-schema' },
      { text: 'GitHub', link: 'https://github.com/Steven-Harris/pipan' }
    ],
    sidebar: [
      {
        text: 'Getting Started',
        items: [
          { text: 'Introduction', link: '/guide/getting-started#introduction' },
          { text: 'Installation', link: '/guide/getting-started#installation' },
          { text: 'Quickstart', link: '/guide/getting-started#quickstart' }
        ]
      },
      {
        text: 'Architecture',
        items: [
          { text: 'Overview', link: '/guide/architecture#overview' },
          { text: 'Tauri & Rust Core', link: '/guide/architecture#tauri-and-rust-core' },
          { text: 'Pi RPC Sidecar', link: '/guide/architecture#pi-rpc-sidecar' },
          { text: 'SQLite Persistence', link: '/guide/architecture#sqlite-persistence' }
        ]
      },
      {
        text: 'Features',
        items: [
          { text: 'Split Workspace', link: '/guide/features#split-workspace' },
          { text: 'DAG Canvas', link: '/guide/features#dag-canvas' },
          { text: 'GitHub Integration', link: '/guide/features#github-integration' },
          { text: 'Terminal & Explorer', link: '/guide/features#terminal-and-explorer' },
          { text: 'Doctor', link: '/guide/features#doctor' }
        ]
      },
      {
        text: 'Extensions',
        items: [
          { text: 'Plugin Guide', link: '/guide/extensions#plugin-guide' },
          { text: 'Manifest Schema', link: '/guide/extensions#manifest-schema' }
        ]
      }
    ],
    socialLinks: [
      { icon: 'github', link: 'https://github.com/Steven-Harris/pipan' }
    ],
    search: {
      provider: 'local'
    }
  }
});
