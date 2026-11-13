import { invoke } from '@tauri-apps/api/core';
import { CHANNELS_EXTENSION_ID, graphPanelId, parseGraphPanelId } from '#lib/channels/panels.ts';
import { channels } from '#lib/stores/channels.svelte.ts';
import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
import type { PiSkillItem } from '#lib/types/pi.ts';
import {
  ALIASES_BY_BUILTIN_ID,
  BUILTIN_EXTENSIONS,
  resolveExtensionIcon,
  resolveExtensionId
} from './builtin.ts';
import type {
  ExternalExtensionBundle,
  PiPanExtensionManifest,
  PiPanRuntimeExtensionRegistration,
  PiPanRuntimeStatusBarItemState,
  RegisteredExtension,
  ResolvedMainViewContribution,
  ResolvedPanelContribution,
  ResolvedSidebarTabContribution,
  ResolvedSlashCommandContribution,
  ResolvedStatusBarItemContribution
} from './types.ts';

const STORAGE_KEY = 'pipan.extensions.v1';

interface PersistedExtensionPref {
  enabled: boolean;
  installed: boolean;
}

type PersistedExtensionsMap = Record<string, PersistedExtensionPref>;

function loadSavedPreferences(): PersistedExtensionsMap {
  try {
    const raw = typeof localStorage === 'undefined' ? null : localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as unknown;
    if (!parsed || typeof parsed !== 'object') return {};
    const out: PersistedExtensionsMap = {};
    for (const [key, val] of Object.entries(parsed as Record<string, unknown>)) {
      if (val && typeof val === 'object') {
        const obj = val as Partial<PersistedExtensionPref>;
        out[key] = {
          enabled: typeof obj.enabled === 'boolean' ? obj.enabled : true,
          installed: typeof obj.installed === 'boolean' ? obj.installed : true
        };
      }
    }
    return out;
  } catch {
    return {};
  }
}

function createInitialBuiltinList(prefs: PersistedExtensionsMap = {}): RegisteredExtension[] {
  return BUILTIN_EXTENSIONS.map((manifest) => {
    const saved = prefs[manifest.id];
    const defaultEnabled = manifest.enabledByDefault ?? true;
    const installed = saved ? saved.installed : true;
    const enabled = installed ? (saved ? saved.enabled : defaultEnabled) : false;
    return {
      manifest,
      builtin: true,
      installed,
      enabled,
      aliases: ALIASES_BY_BUILTIN_ID[manifest.id] ?? [],
      error: null
    };
  });
}

class ExtensionRegistry {
  extensions = $state<RegisteredExtension[]>(createInitialBuiltinList());
  runtimeRegistrations = $state<Record<string, PiPanRuntimeExtensionRegistration>>({});
  loading = $state<boolean>(false);
  error = $state<string | null>(null);

  private initialized = false;
  private prefs: PersistedExtensionsMap = {};

  readonly installedExtensions = $derived(this.extensions.filter((ext) => ext.installed));
  readonly enabledExtensions = $derived(
    this.extensions.filter((ext) => ext.installed && ext.enabled)
  );
  readonly builtinExtensions = $derived(this.extensions.filter((ext) => ext.builtin));
  readonly externalExtensions = $derived(this.extensions.filter((ext) => !ext.builtin));

  readonly contributedPanels = $derived.by((): ResolvedPanelContribution[] => {
    const panels: ResolvedPanelContribution[] = [];
    for (const ext of this.enabledExtensions) {
      for (const p of ext.manifest.contributes.panels ?? []) {
        panels.push({
          ...p,
          extensionId: ext.manifest.id,
          extensionName: ext.manifest.name,
          builtin: ext.builtin,
          label: p.title,
          description: p.description ?? `${p.title} (${ext.manifest.name})`,
          iconComponent: resolveExtensionIcon(p.icon)
        });
      }
    }
    // One panel per published graph topic. Offered in the picker, never opened automatically.
    if (this.enabledExtensions.some((ext) => ext.manifest.id === CHANNELS_EXTENSION_ID)) {
      const seen = new Set<string>();
      for (const state of Object.values(channels.states)) {
        if (state.primitive !== 'graph') continue;
        const id = graphPanelId(state.ch, state.topic);
        if (seen.has(id)) continue;
        seen.add(id);
        const title = (state.data as { title?: string }).title ?? state.topic;
        panels.push({
          id,
          title,
          icon: 'network',
          defaultPosition: 'split',
          extensionId: CHANNELS_EXTENSION_ID,
          extensionName: 'Extension Channels',
          builtin: true,
          label: title,
          description: `Graph from ${state.ch}`,
          iconComponent: resolveExtensionIcon('network')
        });
      }
    }
    return panels;
  });

  readonly contributedPanelsById = $derived.by(
    (): Record<string, ResolvedPanelContribution> => {
      const map: Record<string, ResolvedPanelContribution> = {};
      for (const panel of this.contributedPanels) {
        map[panel.id] = panel;
      }
      return map;
    }
  );

  readonly contributedSidebarTabs = $derived.by((): ResolvedSidebarTabContribution[] => {
    const tabs: ResolvedSidebarTabContribution[] = [];
    for (const ext of this.enabledExtensions) {
      for (const tab of ext.manifest.contributes.sidebarTabs ?? []) {
        tabs.push({
          ...tab,
          extensionId: ext.manifest.id,
          extensionName: ext.manifest.name,
          builtin: ext.builtin,
          iconComponent: resolveExtensionIcon(tab.icon)
        });
      }
    }
    return tabs;
  });

  readonly contributedMainViews = $derived.by((): ResolvedMainViewContribution[] => {
    const views: ResolvedMainViewContribution[] = [];
    for (const ext of this.enabledExtensions) {
      for (const view of ext.manifest.contributes.mainViews ?? []) {
        views.push({
          ...view,
          extensionId: ext.manifest.id,
          extensionName: ext.manifest.name,
          builtin: ext.builtin,
          iconComponent: resolveExtensionIcon(view.icon)
        });
      }
    }
    return views;
  });

  readonly contributedStatusBarItems = $derived.by((): ResolvedStatusBarItemContribution[] => {
    const items: ResolvedStatusBarItemContribution[] = [];
    for (const ext of this.enabledExtensions) {
      for (const item of ext.manifest.contributes.statusBarItems ?? []) {
        items.push({
          ...item,
          extensionId: ext.manifest.id,
          extensionName: ext.manifest.name,
          builtin: ext.builtin
        });
      }
    }
    return items.sort((a, b) => (b.priority ?? 0) - (a.priority ?? 0));
  });

  readonly contributedSlashCommands = $derived.by((): ResolvedSlashCommandContribution[] => {
    const commands: ResolvedSlashCommandContribution[] = [];
    for (const ext of this.enabledExtensions) {
      for (const cmd of ext.manifest.contributes.slashCommands ?? []) {
        commands.push({
          ...cmd,
          extensionId: ext.manifest.id,
          extensionName: ext.manifest.name,
          builtin: ext.builtin
        });
      }
    }
    return commands;
  });

  readonly slashCommandsAsSkillItems = $derived.by((): PiSkillItem[] => {
    return this.contributedSlashCommands.map((cmd) => ({
      name: cmd.name,
      description: cmd.description,
      source: 'extension',
      scope: cmd.builtin ? 'builtin' : cmd.extensionId
    }));
  });

  resolveId(idOrAlias: string): string {
    return resolveExtensionId(idOrAlias);
  }

  getExtension(idOrAlias: string): RegisteredExtension | undefined {
    const targetId = this.resolveId(idOrAlias).toLowerCase();
    return this.extensions.find(
      (ext) =>
        ext.manifest.id.toLowerCase() === targetId ||
        ext.aliases.some((alias) => alias.toLowerCase() === targetId)
    );
  }

  isExtensionEnabled(idOrAlias: string): boolean {
    const ext = this.getExtension(idOrAlias);
    return Boolean(ext && ext.installed && ext.enabled);
  }

  isPanelEnabled(panelId: string): boolean {
    if (parseGraphPanelId(panelId)) return this.isExtensionEnabled(CHANNELS_EXTENSION_ID);
    return Boolean(this.contributedPanelsById[panelId]);
  }

  isSidebarTabEnabled(tabId: string): boolean {
    return this.contributedSidebarTabs.some((t) => t.id === tabId);
  }

  isMainViewEnabled(viewId: string): boolean {
    return this.contributedMainViews.some((v) => v.id === viewId);
  }

  isStatusBarItemEnabled(itemId: string): boolean {
    return this.contributedStatusBarItems.some((i) => i.id === itemId);
  }

  getRuntimeStatusBarItemState(
    extensionId: string,
    itemId: string
  ): PiPanRuntimeStatusBarItemState | null {
    const reg = this.runtimeRegistrations[extensionId];
    return reg?.statusBarItems?.[itemId] ?? null;
  }

  async init(): Promise<void> {
    if (this.initialized) return;
    this.initialized = true;

    this.prefs = loadSavedPreferences();
    this.extensions = createInitialBuiltinList(this.prefs);

    this.installWindowHostApi();
    await this.refreshExternalExtensions();
    this.reconcileWorkspaceWithEnabledExtensions();
  }

  private installWindowHostApi(): void {
    if (typeof window === 'undefined') return;
    window.pipan = {
      version: '0.2.0',
      registerExtension: (registration: PiPanRuntimeExtensionRegistration) => {
        this.registerRuntimeExtension(registration);
      },
      openPanel: (panelId: string) => {
        if (this.isPanelEnabled(panelId)) {
          workspaceStore.openPanel(panelId);
        }
      },
      closePanel: (panelId: string) => {
        workspaceStore.closePanel(panelId);
      },
      openMainView: (viewId: string) => {
        if (viewId === 'chat' || viewId === 'extensions' || this.isMainViewEnabled(viewId)) {
          workspaceStore.openMainView(viewId);
        }
      }
    };
  }

  async refreshExternalExtensions(): Promise<void> {
    this.loading = true;
    this.error = null;
    try {
      const bundles = await invoke<ExternalExtensionBundle[]>('list_external_extensions');
      if (Array.isArray(bundles)) {
        this.mergeExternalBundles(bundles);
      }
    } catch {
      // Non-fatal in browser-only preview mode without Tauri backend
    } finally {
      this.loading = false;
    }
  }

  private mergeExternalBundles(bundles: ExternalExtensionBundle[]): void {
    const builtins = this.extensions.filter((e) => e.builtin);
    const externalList: RegisteredExtension[] = [];

    for (const bundle of bundles) {
      const id = bundle.manifest.id;
      const saved = this.prefs[id];
      const defaultEnabled = bundle.manifest.enabledByDefault ?? true;
      const enabled = saved ? saved.enabled : defaultEnabled;

      const regExt: RegisteredExtension = {
        manifest: {
          ...bundle.manifest,
          builtin: false
        },
        builtin: false,
        installed: true,
        enabled,
        aliases: [],
        installPath: bundle.installPath,
        sourceUrl: bundle.sourceUrl,
        entryCode: bundle.entryCode,
        error: null
      };

      externalList.push(regExt);

      if (enabled && bundle.entryCode) {
        this.evaluateExternalEntryScript(regExt, bundle.entryCode);
      }
    }

    this.extensions = [...builtins, ...externalList];
    this.reconcileWorkspaceWithEnabledExtensions();
  }

  private evaluateExternalEntryScript(ext: RegisteredExtension, code: string): void {
    if (typeof window === 'undefined') return;
    try {
      const runner = new Function('window', 'pipan', code);
      runner(window, window.pipan);
    } catch (err) {
      ext.error = err instanceof Error ? err.message : String(err);
    }
  }

  registerRuntimeExtension(registration: PiPanRuntimeExtensionRegistration): void {
    if (!registration || typeof registration.id !== 'string' || !registration.id.trim()) {
      return;
    }
    const resolvedId = this.resolveId(registration.id.trim());
    this.runtimeRegistrations = {
      ...this.runtimeRegistrations,
      [resolvedId]: registration
    };

    const existingIdx = this.extensions.findIndex(
      (e) => e.manifest.id.toLowerCase() === resolvedId.toLowerCase()
    );

    if (existingIdx === -1 && registration.manifest) {
      const m = registration.manifest;
      const manifest: PiPanExtensionManifest = {
        id: resolvedId,
        name: m.name?.trim() || resolvedId,
        version: m.version?.trim() || '0.1.0',
        description: m.description?.trim() || 'Runtime registered PiPan plugin',
        author: m.author?.trim() || 'External Plugin',
        builtin: false,
        enabledByDefault: m.enabledByDefault ?? true,
        contributes: m.contributes ?? {}
      };
      const saved = this.prefs[resolvedId];
      const enabled = saved ? saved.enabled : (manifest.enabledByDefault ?? true);
      this.extensions = [
        ...this.extensions,
        {
          manifest,
          builtin: false,
          installed: true,
          enabled,
          aliases: [],
          error: null
        }
      ];
    }

    try {
      registration.activate?.();
    } catch (err) {
      const target = this.getExtension(resolvedId);
      if (target) {
        target.error = err instanceof Error ? err.message : String(err);
      }
    }
  }

  setExtensionEnabled(idOrAlias: string, enabled: boolean): void {
    const ext = this.getExtension(idOrAlias);
    if (!ext) return;
    if (enabled && !ext.installed) {
      ext.installed = true;
    }
    ext.enabled = enabled;
    this.prefs[ext.manifest.id] = {
      enabled: ext.enabled,
      installed: ext.installed
    };
    this.persistPreferences();

    if (enabled && !ext.builtin && ext.entryCode && !this.runtimeRegistrations[ext.manifest.id]) {
      this.evaluateExternalEntryScript(ext, ext.entryCode);
    } else if (!enabled) {
      try {
        this.runtimeRegistrations[ext.manifest.id]?.deactivate?.();
      } catch {
        // Ignore plugin deactivate errors
      }
    }

    this.reconcileWorkspaceWithEnabledExtensions();
  }

  toggleExtension(idOrAlias: string): void {
    const ext = this.getExtension(idOrAlias);
    if (!ext) return;
    this.setExtensionEnabled(ext.manifest.id, !ext.enabled);
  }

  installBuiltinExtension(idOrAlias: string): void {
    const ext = this.getExtension(idOrAlias);
    if (!ext || !ext.builtin) return;
    ext.installed = true;
    ext.enabled = true;
    this.prefs[ext.manifest.id] = {
      enabled: true,
      installed: true
    };
    this.persistPreferences();
  }

  async uninstallExtension(idOrAlias: string): Promise<boolean> {
    const ext = this.getExtension(idOrAlias);
    if (!ext) return false;
    this.error = null;

    if (ext.builtin) {
      ext.enabled = false;
      ext.installed = false;
      this.prefs[ext.manifest.id] = {
        enabled: false,
        installed: false
      };
      this.persistPreferences();
      this.reconcileWorkspaceWithEnabledExtensions();
      return true;
    }

    try {
      if (ext.installPath) {
        await invoke<boolean>('uninstall_extension', {
          extensionId: ext.manifest.id
        });
      }
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return false;
    }

    try {
      this.runtimeRegistrations[ext.manifest.id]?.deactivate?.();
    } catch {
      // Ignore deactivate error
    }

    const nextRuntime = { ...this.runtimeRegistrations };
    delete nextRuntime[ext.manifest.id];
    this.runtimeRegistrations = nextRuntime;

    delete this.prefs[ext.manifest.id];
    this.persistPreferences();

    this.extensions = this.extensions.filter((e) => e.manifest.id !== ext.manifest.id);
    this.reconcileWorkspaceWithEnabledExtensions();
    return true;
  }

  async installFromFolder(sourcePath: string): Promise<RegisteredExtension | null> {
    const trimmed = sourcePath.trim();
    if (!trimmed) {
      this.error = 'Please provide a local extension folder path.';
      return null;
    }
    this.loading = true;
    this.error = null;
    try {
      const bundle = await invoke<ExternalExtensionBundle>('install_extension_from_folder', {
        sourcePath: trimmed
      });
      this.prefs[bundle.manifest.id] = {
        enabled: true,
        installed: true
      };
      this.persistPreferences();
      await this.refreshExternalExtensions();
      return this.getExtension(bundle.manifest.id) ?? null;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return null;
    } finally {
      this.loading = false;
    }
  }

  async installFromGit(repoUrl: string): Promise<RegisteredExtension | null> {
    const trimmed = repoUrl.trim();
    if (!trimmed) {
      this.error = 'Please provide a Git repository URL.';
      return null;
    }
    this.loading = true;
    this.error = null;
    try {
      const bundle = await invoke<ExternalExtensionBundle>('install_extension_from_git', {
        repoUrl: trimmed
      });
      this.prefs[bundle.manifest.id] = {
        enabled: true,
        installed: true
      };
      this.persistPreferences();
      await this.refreshExternalExtensions();
      return this.getExtension(bundle.manifest.id) ?? null;
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
      return null;
    } finally {
      this.loading = false;
    }
  }

  resetAllBuiltins(): void {
    for (const ext of this.extensions) {
      if (ext.builtin) {
        ext.installed = true;
        ext.enabled = true;
        this.prefs[ext.manifest.id] = {
          enabled: true,
          installed: true
        };
      }
    }
    this.persistPreferences();
    this.reconcileWorkspaceWithEnabledExtensions();
  }

  executeSlashCommand(commandName: string, args = ''): boolean {
    const cleanName = commandName.trim().replace(/^\/+/, '').toLowerCase();
    for (const ext of this.enabledExtensions) {
      const runtimeHandler = this.runtimeRegistrations[ext.manifest.id]?.slashCommands?.[cleanName];
      if (runtimeHandler) {
        void runtimeHandler(args);
        return true;
      }
    }

    // Built-in convenience slash command triggers when invoked directly
    if (cleanName === 'nodes' && this.isPanelEnabled('nodes')) {
      workspaceStore.togglePanel('nodes');
      return true;
    }
    if (cleanName === 'terminal' && this.isPanelEnabled('terminal')) {
      workspaceStore.togglePanel('terminal');
      return true;
    }
    if (cleanName === 'files' && this.isPanelEnabled('files')) {
      workspaceStore.togglePanel('files');
      return true;
    }
    if (cleanName === 'prs' && this.isPanelEnabled('prs')) {
      workspaceStore.togglePanel('prs');
      return true;
    }
    if (cleanName === 'cost' && this.isMainViewEnabled('cost')) {
      workspaceStore.openCostView();
      return true;
    }
    return false;
  }

  reconcileWorkspaceWithEnabledExtensions(): void {
    // Close any open session panels whose contributing extension has been disabled or uninstalled
    for (const openId of [...workspaceStore.openPanels]) {
      if (!this.isPanelEnabled(openId)) {
        workspaceStore.closePanel(openId);
      }
    }

    // If current main view is a contributed view (e.g., 'cost') and its extension is disabled, return to 'chat'
    const currentView = workspaceStore.currentView;
    if (
      currentView !== 'chat' &&
      currentView !== 'extensions' &&
      !this.isMainViewEnabled(currentView)
    ) {
      workspaceStore.openChatView();
    }
  }

  private persistPreferences(): void {
    try {
      if (typeof localStorage === 'undefined') return;
      localStorage.setItem(STORAGE_KEY, JSON.stringify(this.prefs));
    } catch {
      // Storage unavailable
    }
  }
}

export const extensionRegistry = new ExtensionRegistry();
