import { GitPullRequest, Network, SquareTerminal, FolderTree } from 'lucide-svelte';
import type { PanelId } from '#lib/stores/workspace.svelte.ts';

export interface PanelDefinition {
  id: PanelId;
  label: string;
  description: string;
  icon: typeof Network;
}

/** Display order is also the header toolbar order. */
export const PANELS: readonly PanelDefinition[] = [
  { id: 'nodes', label: 'Node Graph', description: 'Orchestration DAG for this session', icon: Network },
  { id: 'terminal', label: 'Terminal', description: 'Shell in the session workspace', icon: SquareTerminal },
  { id: 'files', label: 'Files', description: 'Browse the session workspace', icon: FolderTree },
  { id: 'prs', label: 'Pull Requests', description: 'Review and merge GitHub PRs', icon: GitPullRequest }
];

export const PANEL_BY_ID: Record<PanelId, PanelDefinition> = Object.fromEntries(
  PANELS.map((p) => [p.id, p])
) as Record<PanelId, PanelDefinition>;
