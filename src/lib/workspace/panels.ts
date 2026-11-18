import { GitPullRequest, Network, SquareTerminal, FolderTree, Puzzle } from 'lucide-svelte';
import type { PanelId } from '#lib/stores/workspace.svelte.ts';
import { parseGraphPanelId } from '#lib/channels/panels.ts';
import { channels } from '#lib/stores/channels.svelte.ts';
import { extensionRegistry } from '#lib/extensions/registry.svelte.ts';

export interface PanelDefinition {
  id: PanelId;
  label: string;
  description: string;
  icon: typeof Network;
}

/** Built-in display order (also contributed via `BUILTIN_EXTENSIONS` in `src/lib/extensions/builtin.ts`). */
export const PANELS: readonly PanelDefinition[] = [
  { id: 'nodes', label: 'Node Graph', description: 'Orchestration DAG for this session', icon: Network },
  { id: 'terminal', label: 'Terminal', description: 'Shell in the session workspace', icon: SquareTerminal },
  { id: 'files', label: 'Files', description: 'Browse the session workspace', icon: FolderTree },
  { id: 'prs', label: 'Pull Requests', description: 'Review and merge GitHub PRs', icon: GitPullRequest }
];

export const PANEL_BY_ID: Record<string, PanelDefinition> = Object.fromEntries(
  PANELS.map((p) => [p.id, p])
);

export function getPanelDefinition(id: PanelId): PanelDefinition {
  const contributed = extensionRegistry.contributedPanelsById[id];
  if (contributed) {
    return {
      id: contributed.id,
      label: contributed.label,
      description: contributed.description,
      icon: contributed.iconComponent
    };
  }
  const channelGraph = parseGraphPanelId(id);
  if (channelGraph) {
    const state = Object.values(channels.states).find(
      (s) => s.ch === channelGraph.ch && s.topic === channelGraph.topic
    );
    return {
      id,
      label: (state?.data as { title?: string } | undefined)?.title ?? channelGraph.topic,
      description: `Graph from ${channelGraph.ch}`,
      icon: Network
    };
  }
  return (
    PANEL_BY_ID[id] ?? {
      id,
      label: id,
      description: 'Extension panel',
      icon: Puzzle
    }
  );
}
