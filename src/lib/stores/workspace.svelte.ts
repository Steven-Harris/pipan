// Workspace layout state: which auxiliary panels sit next to the chat, how the
// split is oriented, and how much room the panel area takes.
//
// Persisted to localStorage (user settings). PiPan's SQLite store arrives with
// persistent sessions; this key is the migration source for it.

import { githubStore } from '#lib/stores/github.svelte.ts';

export type BuiltinPanelId = 'nodes' | 'terminal' | 'files' | 'prs';
export type PanelId = BuiltinPanelId | (string & {});
export type SplitOrientation = 'vertical' | 'horizontal';
export type MainView = 'chat' | 'cost' | 'extensions' | (string & {});

export const PANEL_IDS: readonly BuiltinPanelId[] = ['nodes', 'terminal', 'files', 'prs'];

export const MIN_SPLIT_RATIO = 0.2;
export const MAX_SPLIT_RATIO = 0.8;
export const DEFAULT_SPLIT_RATIO = 0.5;

const STORAGE_KEY = 'pipan.workspace.v1';

interface PersistedWorkspace {
  orientation: SplitOrientation;
  openPanels: PanelId[];
  activePanel: PanelId | null;
  splitRatio: number;
  panelOpen?: boolean;
}

function isPanelId(value: unknown): value is PanelId {
  return typeof value === 'string' && value.trim().length > 0;
}

export function clampRatio(ratio: number): number {
  if (!Number.isFinite(ratio)) return DEFAULT_SPLIT_RATIO;
  return Math.min(MAX_SPLIT_RATIO, Math.max(MIN_SPLIT_RATIO, ratio));
}

function load(): PersistedWorkspace {
  const fallback: PersistedWorkspace = {
    orientation: 'vertical',
    openPanels: [],
    activePanel: null,
    splitRatio: DEFAULT_SPLIT_RATIO,
    panelOpen: false
  };
  try {
    const raw = typeof localStorage === 'undefined' ? null : localStorage.getItem(STORAGE_KEY);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Partial<PersistedWorkspace>;
    const openPanels = Array.isArray(parsed.openPanels)
      ? [...new Set(parsed.openPanels.filter(isPanelId))]
      : [];
    const activePanel =
      isPanelId(parsed.activePanel) && openPanels.includes(parsed.activePanel)
        ? parsed.activePanel
        : (openPanels[0] ?? null);
    const panelOpen =
      typeof parsed.panelOpen === 'boolean'
        ? parsed.panelOpen
        : openPanels.length > 0;
    return {
      orientation: parsed.orientation === 'horizontal' ? 'horizontal' : 'vertical',
      openPanels,
      activePanel,
      splitRatio: clampRatio(parsed.splitRatio ?? DEFAULT_SPLIT_RATIO),
      panelOpen
    };
  } catch {
    return fallback;
  }
}

class WorkspaceStore {
  orientation = $state<SplitOrientation>('vertical');
  openPanels = $state<PanelId[]>([]);
  activePanel = $state<PanelId | null>(null);
  splitRatio = $state<number>(DEFAULT_SPLIT_RATIO);
  currentView = $state<MainView>('chat');
  panelOpen = $state<boolean>(false);

  private loaded = false;

  /** Hydrate from user settings. Call once from onMount (localStorage is browser-only). */
  init(): void {
    if (this.loaded) return;
    this.loaded = true;
    const saved = load();
    this.orientation = saved.orientation;
    this.openPanels = saved.openPanels;
    this.activePanel = saved.activePanel;
    this.splitRatio = saved.splitRatio;
    this.panelOpen = saved.panelOpen ?? saved.openPanels.length > 0;
  }

  get isPanelOpen(): boolean {
    return this.panelOpen;
  }

  get hasPanels(): boolean {
    return this.panelOpen;
  }

  togglePanelArea(): void {
    this.panelOpen = !this.panelOpen;
    this.persist();
  }

  openPanelArea(): void {
    this.panelOpen = true;
    this.persist();
  }

  closePanelArea(): void {
    this.panelOpen = false;
    this.persist();
  }

  isOpen(id: PanelId): boolean {
    return this.openPanels.includes(id);
  }

  /** Header/slash command behaviour: closed -> open and focus; open -> close. */
  togglePanel(id: PanelId): void {
    if (this.isOpen(id)) {
      this.closePanel(id);
    } else {
      this.openPanel(id);
    }
  }

  openPanel(id: PanelId): void {
    if (!this.isOpen(id)) {
      this.openPanels = [...this.openPanels, id];
    }
    this.activePanel = id;
    this.panelOpen = true;
    this.persist();
  }

  closePanel(id: PanelId): void {
    const idx = this.openPanels.indexOf(id);
    if (idx === -1) return;
    const next = this.openPanels.filter((p) => p !== id);
    this.openPanels = next;
    if (this.activePanel === id) {
      // Prefer the neighbouring tab that slides into the closed tab's slot.
      this.activePanel = next[Math.min(idx, next.length - 1)] ?? null;
    }
    // Note: panel remains open even when empty so user can select + to load a panel
    this.persist();
  }

  setActivePanel(id: PanelId): void {
    if (!this.isOpen(id)) return;
    this.activePanel = id;
    this.persist();
  }

  setOrientation(orientation: SplitOrientation): void {
    if (this.orientation === orientation) return;
    this.orientation = orientation;
    this.persist();
  }

  toggleOrientation(): void {
    this.setOrientation(this.orientation === 'vertical' ? 'horizontal' : 'vertical');
  }

  /** Live update during a drag; call `commitSplitRatio` on release to persist. */
  setSplitRatio(ratio: number): void {
    this.splitRatio = clampRatio(ratio);
  }

  commitSplitRatio(): void {
    this.persist();
  }

  openCostView(): void {
    void githubStore.selectPr(null);
    this.currentView = 'cost';
  }

  openChatView(): void {
    this.currentView = 'chat';
  }

  toggleCostView(): void {
    void githubStore.selectPr(null);
    this.currentView = this.currentView === 'cost' ? 'chat' : 'cost';
  }

  openExtensionsView(): void {
    void githubStore.selectPr(null);
    this.currentView = 'extensions';
  }

  toggleExtensionsView(): void {
    void githubStore.selectPr(null);
    this.currentView = this.currentView === 'extensions' ? 'chat' : 'extensions';
  }

  openMainView(viewId: MainView): void {
    if (viewId !== 'chat') {
      void githubStore.selectPr(null);
    }
    this.currentView = viewId;
  }

  private persist(): void {
    if (!this.loaded) return;
    try {
      const data: PersistedWorkspace = {
        orientation: this.orientation,
        openPanels: $state.snapshot(this.openPanels),
        activePanel: this.activePanel,
        splitRatio: this.splitRatio,
        panelOpen: this.panelOpen
      };
      localStorage.setItem(STORAGE_KEY, JSON.stringify(data));
    } catch {
      // Storage unavailable or full: layout simply won't persist this time.
    }
  }
}

export const workspaceStore = new WorkspaceStore();
