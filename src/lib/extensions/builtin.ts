import {
  Activity,
  Blocks,
  Bot,
  Coins,
  FileCode,
  FolderTree,
  GitBranch,
  GitPullRequest,
  HeartPulse,
  Network,
  Puzzle,
  Radio,
  Sparkles,
  SquareTerminal,
  Terminal,
  Wand2
} from 'lucide-svelte';
import type { ExtensionIconComponent, PiPanExtensionManifest } from './types.ts';

/**
 * 1. `pipan.node-graph` (`pipan.nodes`) — Orchestration Node Graph (DAG)
 * Contributes the Nodes split panel & graph inspector.
 */
export const NODE_GRAPH_EXTENSION: PiPanExtensionManifest = {
  id: 'pipan.node-graph',
  name: 'Orchestration Node Graph (DAG)',
  version: '0.2.0',
  description:
    'Contributes the Nodes DAG split panel, interactive node inspector, and per-session TB/LR coordinate persistence.',
  author: 'PiPan Core',
  builtin: true,
  enabledByDefault: true,
  contributes: {
    panels: [
      {
        id: 'nodes',
        title: 'Node Graph',
        icon: 'network',
        defaultPosition: 'split',
        description: 'Orchestration DAG for this session'
      }
    ],
    slashCommands: [
      {
        name: 'nodes',
        description: 'Toggle the Orchestration Node Graph (DAG) split panel'
      }
    ]
  }
};

/**
 * 2. `pipan.github-prs` (`pipan.github`) — GitHub Pull Requests & Reviews
 * Contributes the PR sidebar tab, PR session panel, and webhook status bar item.
 */
export const GITHUB_PRS_EXTENSION: PiPanExtensionManifest = {
  id: 'pipan.github-prs',
  name: 'GitHub Pull Requests & Reviews',
  version: '0.2.0',
  description:
    'Contributes the PR sidebar tab, Pull Requests review/diff/merge session panel, and live gh webhook forward status bar item.',
  author: 'PiPan Core',
  builtin: true,
  enabledByDefault: true,
  contributes: {
    panels: [
      {
        id: 'prs',
        title: 'Pull Requests',
        icon: 'git-pull-request',
        defaultPosition: 'split',
        description: 'Review and merge GitHub PRs'
      }
    ],
    sidebarTabs: [
      {
        id: 'prs',
        title: 'PRs',
        icon: 'git-pull-request'
      }
    ],
    statusBarItems: [
      {
        id: 'github-webhook',
        alignment: 'left',
        priority: 50
      }
    ],
    slashCommands: [
      {
        name: 'prs',
        description: 'Toggle the GitHub Pull Requests & Reviews split panel'
      }
    ]
  }
};

/**
 * 3. `pipan.terminal` — Integrated Workspace Terminal
 * Contributes the Terminal session panel.
 */
export const TERMINAL_EXTENSION: PiPanExtensionManifest = {
  id: 'pipan.terminal',
  name: 'Integrated Workspace Terminal',
  version: '0.2.0',
  description:
    'Contributes the integrated shell session panel bound to the active workspace directory with Pi RPC bash bridge.',
  author: 'PiPan Core',
  builtin: true,
  enabledByDefault: true,
  contributes: {
    panels: [
      {
        id: 'terminal',
        title: 'Terminal',
        icon: 'square-terminal',
        defaultPosition: 'bottom',
        description: 'Shell in the session workspace'
      }
    ],
    slashCommands: [
      {
        name: 'terminal',
        description: 'Toggle the Integrated Workspace Terminal session panel'
      }
    ]
  }
};

/**
 * 4. `pipan.files` — Workspace File Explorer
 * Contributes the Files session panel and prompt file tree picker.
 */
export const FILES_EXTENSION: PiPanExtensionManifest = {
  id: 'pipan.files',
  name: 'Workspace File Explorer',
  version: '0.2.0',
  description:
    'Contributes the Files session panel with Git status badges, syntax preview, and prompt file tree attachment picker.',
  author: 'PiPan Core',
  builtin: true,
  enabledByDefault: true,
  contributes: {
    panels: [
      {
        id: 'files',
        title: 'Files',
        icon: 'folder-tree',
        defaultPosition: 'split',
        description: 'Browse the session workspace'
      }
    ],
    slashCommands: [
      {
        name: 'files',
        description: 'Toggle the Workspace File Explorer session panel'
      }
    ]
  }
};

/**
 * 5. `pipan.cost-analytics` (`pipan.analytics`) — Cost & Usage Analytics
 * Contributes the Cost main view and status bar spend widget.
 */
export const COST_ANALYTICS_EXTENSION: PiPanExtensionManifest = {
  id: 'pipan.cost-analytics',
  name: 'Cost & Usage Analytics',
  version: '0.2.0',
  description:
    'Contributes the Cost & Usage Analytics main view (Day/Week/Month/Year) and the status bar session/cumulative spend widget.',
  author: 'PiPan Core',
  builtin: true,
  enabledByDefault: true,
  contributes: {
    mainViews: [
      {
        id: 'cost',
        title: 'Cost',
        icon: 'coins'
      }
    ],
    statusBarItems: [
      {
        id: 'cost-spend',
        alignment: 'right',
        priority: 80
      }
    ],
    slashCommands: [
      {
        name: 'cost',
        description: 'Open the Cost & Usage Analytics dashboard'
      }
    ]
  }
};

// Named exports for both canonical and short module identifiers
export const PIPAN_NODES_EXTENSION = NODE_GRAPH_EXTENSION;
export const PIPAN_GITHUB_EXTENSION = GITHUB_PRS_EXTENSION;
export const PIPAN_TERMINAL_EXTENSION = TERMINAL_EXTENSION;
export const PIPAN_FILES_EXTENSION = FILES_EXTENSION;
export const PIPAN_ANALYTICS_EXTENSION = COST_ANALYTICS_EXTENSION;

/**
 * Ordered so contributed session panels default to `nodes`, `terminal`, `files`, `prs`
 * in the session header toolbar.
 */
export const BUILTIN_EXTENSIONS: readonly PiPanExtensionManifest[] = [
  NODE_GRAPH_EXTENSION,
  TERMINAL_EXTENSION,
  FILES_EXTENSION,
  GITHUB_PRS_EXTENSION,
  COST_ANALYTICS_EXTENSION
];

export const BUILTIN_EXTENSION_ALIASES: Readonly<Record<string, string>> = {
  'pipan.node-graph': 'pipan.node-graph',
  'pipan.nodes': 'pipan.node-graph',
  'node-graph': 'pipan.node-graph',
  nodes: 'pipan.node-graph',
  'pipan.github-prs': 'pipan.github-prs',
  'pipan.github': 'pipan.github-prs',
  'github-prs': 'pipan.github-prs',
  github: 'pipan.github-prs',
  prs: 'pipan.github-prs',
  'pipan.terminal': 'pipan.terminal',
  terminal: 'pipan.terminal',
  'pipan.files': 'pipan.files',
  files: 'pipan.files',
  'pipan.cost-analytics': 'pipan.cost-analytics',
  'pipan.analytics': 'pipan.cost-analytics',
  'cost-analytics': 'pipan.cost-analytics',
  analytics: 'pipan.cost-analytics',
  cost: 'pipan.cost-analytics'
};

export const ALIASES_BY_BUILTIN_ID: Readonly<Record<string, string[]>> = {
  'pipan.node-graph': ['pipan.nodes', 'node-graph'],
  'pipan.github-prs': ['pipan.github', 'github-prs'],
  'pipan.terminal': ['terminal'],
  'pipan.files': ['files'],
  'pipan.cost-analytics': ['pipan.analytics', 'cost-analytics']
};

export function resolveExtensionId(idOrAlias: string): string {
  const normalized = idOrAlias.trim().toLowerCase();
  return BUILTIN_EXTENSION_ALIASES[normalized] ?? idOrAlias.trim();
}

const ICON_MAP: Readonly<Record<string, ExtensionIconComponent>> = {
  network: Network,
  'git-pull-request': GitPullRequest,
  'square-terminal': SquareTerminal,
  terminal: Terminal,
  'folder-tree': FolderTree,
  coins: Coins,
  blocks: Blocks,
  puzzle: Puzzle,
  sparkles: Sparkles,
  'file-code': FileCode,
  'git-branch': GitBranch,
  radio: Radio,
  activity: Activity,
  'heart-pulse': HeartPulse,
  bot: Bot,
  wand: Wand2
};

export function resolveExtensionIcon(iconName?: string): ExtensionIconComponent {
  if (!iconName) return Puzzle;
  const key = iconName.trim().toLowerCase();
  return ICON_MAP[key] ?? Puzzle;
}
