// Draws the user to extension graphs: opens a graph's panel the first time it is published,
// remembers which panel has a node that started waiting for the user, and tells the panel which
// node to select.

import { MAX_AUTO_OPENED_PANELS, newlyWaiting, waitingWithActions } from '#lib/channels/attention.ts';
import { graphPanelId, parseGraphPanelId } from '#lib/channels/panels.ts';
import type { GraphState } from '#lib/channels/types.ts';
import { channels, type GraphCommit } from '#lib/stores/channels.svelte.ts';
import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
import { extensionRegistry } from '#lib/extensions/registry.svelte.ts';

interface SessionRecord {
  /** Panels already handled: opened by PiPan, or found open. A panel the user closes stays closed. */
  handled: Set<string>;
  /** Panels PiPan opened itself, for the cap. */
  opened: number;
}

class ChannelAttention {
  /** Panels with a waiting node the user has not looked at yet. */
  unseen = $state.raw<Record<string, true>>({});
  /** The node a panel should select the next time it can. */
  focus = $state.raw<Record<string, string>>({});

  // Kept across session switches so a panel the user closed is not reopened on return. An app
  // restart starts fresh.
  private sessions = new Map<string, SessionRecord>();
  private started = false;

  /** Subscribes to the channel store. Call once from the app shell. */
  init(): void {
    if (this.started) return;
    this.started = true;
    channels.onGraph((commit) => this.handleGraph(commit));
    channels.onReset(() => {
      this.unseen = {};
      this.focus = {};
    });
    $effect.root(() => {
      // Looking at a panel clears its dot.
      $effect(() => {
        const active = workspaceStore.activePanel;
        if (workspaceStore.panelOpen && active && this.unseen[active]) {
          const { [active]: _seen, ...rest } = this.unseen;
          this.unseen = rest;
        }
      });
    });
  }

  /**
   * True while the panel has a node waiting for the user that they have not looked at. The mark
   * goes away when the node stops waiting or the graph is cleared, as well as when the panel is shown.
   */
  needsAttention(panelId: string): boolean {
    if (!this.unseen[panelId]) return false;
    const parsed = parseGraphPanelId(panelId);
    if (!parsed) return false;
    const state = Object.values(channels.states).find((s) => s.ch === parsed.ch && s.topic === parsed.topic);
    return state !== undefined && waitingWithActions(state.data as GraphState).length > 0;
  }

  private isVisible(panelId: string): boolean {
    return workspaceStore.panelOpen && workspaceStore.activePanel === panelId;
  }

  handleGraph({ state, previous }: GraphCommit): void {
    const panelId = graphPanelId(state.ch, state.topic);
    this.maybeOpen(state.sessionId, panelId);

    const graph = state.data as GraphState;
    // The run moved on, so whatever the mark pointed at no longer needs the user.
    if (waitingWithActions(graph).length === 0 && this.unseen[panelId]) {
      const { [panelId]: _stale, ...rest } = this.unseen;
      this.unseen = rest;
    }
    const waiting = newlyWaiting(previous?.data as GraphState | undefined, graph);
    if (waiting.length === 0) return;
    this.focus = { ...this.focus, [panelId]: waiting[0] };
    if (!this.isVisible(panelId)) this.unseen = { ...this.unseen, [panelId]: true };
  }

  private maybeOpen(sessionId: string, panelId: string): void {
    let record = this.sessions.get(sessionId);
    if (!record) this.sessions.set(sessionId, (record = { handled: new Set(), opened: 0 }));
    if (record.handled.has(panelId)) return;
    if (workspaceStore.isOpen(panelId)) {
      record.handled.add(panelId);
      return;
    }
    // Not offered yet (extensions still loading, or the channels extension is off): try again
    // on the next publish.
    if (!extensionRegistry.isPanelEnabled(panelId)) return;
    if (record.opened >= MAX_AUTO_OPENED_PANELS) return;
    record.opened += 1;
    record.handled.add(panelId);
    workspaceStore.openPanel(panelId);
  }

  /** Called by the panel once it has selected the node. */
  consumeFocus(panelId: string): void {
    if (!(panelId in this.focus)) return;
    const { [panelId]: _done, ...rest } = this.focus;
    this.focus = rest;
  }

  /** Test helper: forget what was opened. */
  resetForTests(): void {
    this.sessions.clear();
    this.unseen = {};
    this.focus = {};
  }
}

export const channelAttention = new ChannelAttention();
