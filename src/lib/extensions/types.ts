import type { Network } from 'lucide-svelte';

export type ExtensionIconComponent = typeof Network;
export type ExtensionPanelDefaultPosition = 'split' | 'bottom';
export type ExtensionStatusBarAlignment = 'left' | 'right';

export interface ExtensionPanelContribution {
  id: string;
  title: string;
  icon: string;
  defaultPosition?: ExtensionPanelDefaultPosition;
  description?: string;
}

export interface ExtensionSidebarTabContribution {
  id: string;
  title: string;
  icon: string;
}

export interface ExtensionMainViewContribution {
  id: string;
  title: string;
  icon: string;
}

export interface ExtensionStatusBarItemContribution {
  id: string;
  alignment: ExtensionStatusBarAlignment;
  priority?: number;
}

export interface ExtensionSlashCommandContribution {
  name: string;
  description: string;
}

export interface PipanExtensionContributes {
  panels?: ExtensionPanelContribution[];
  sidebarTabs?: ExtensionSidebarTabContribution[];
  mainViews?: ExtensionMainViewContribution[];
  statusBarItems?: ExtensionStatusBarItemContribution[];
  slashCommands?: ExtensionSlashCommandContribution[];
}

export interface PipanExtensionManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  builtin?: boolean;
  enabledByDefault?: boolean;
  contributes: PipanExtensionContributes;
  entry?: string; // Relative JS module path for external plugins
}

export interface ExternalExtensionBundle {
  manifest: PipanExtensionManifest;
  installPath: string;
  entryCode?: string;
  sourceUrl?: string;
}

export interface ResolvedPanelContribution extends ExtensionPanelContribution {
  extensionId: string;
  extensionName: string;
  builtin: boolean;
  label: string;
  description: string;
  iconComponent: ExtensionIconComponent;
}

export interface ResolvedSidebarTabContribution extends ExtensionSidebarTabContribution {
  extensionId: string;
  extensionName: string;
  builtin: boolean;
  iconComponent: ExtensionIconComponent;
}

export interface ResolvedMainViewContribution extends ExtensionMainViewContribution {
  extensionId: string;
  extensionName: string;
  builtin: boolean;
  iconComponent: ExtensionIconComponent;
}

export interface ResolvedStatusBarItemContribution extends ExtensionStatusBarItemContribution {
  extensionId: string;
  extensionName: string;
  builtin: boolean;
}

export interface ResolvedSlashCommandContribution extends ExtensionSlashCommandContribution {
  extensionId: string;
  extensionName: string;
  builtin: boolean;
}

export interface PipanExtensionRenderContext {
  extensionId: string;
  contributionId: string;
  kind: 'panel' | 'sidebarTab' | 'mainView';
  manifest: PipanExtensionManifest;
}

export type PipanContributionRenderFn = (
  container: HTMLElement,
  context: PipanExtensionRenderContext
) => void | (() => void);

export interface PipanRuntimeStatusBarItemState {
  text: string;
  tooltip?: string;
  badge?: string;
  onClick?: () => void;
}

export interface PipanRuntimeExtensionRegistration {
  id: string;
  manifest?: Partial<PipanExtensionManifest>;
  panels?: Record<string, PipanContributionRenderFn | { html?: string; text?: string }>;
  sidebarTabs?: Record<string, PipanContributionRenderFn | { html?: string; text?: string }>;
  mainViews?: Record<string, PipanContributionRenderFn | { html?: string; text?: string }>;
  statusBarItems?: Record<string, PipanRuntimeStatusBarItemState>;
  slashCommands?: Record<string, (args: string) => void | Promise<void> | string | Promise<string>>;
  activate?: () => void | (() => void);
  deactivate?: () => void;
}

export interface RegisteredExtension {
  manifest: PipanExtensionManifest;
  builtin: boolean;
  installed: boolean;
  enabled: boolean;
  aliases: string[];
  installPath?: string;
  sourceUrl?: string;
  entryCode?: string;
  error?: string | null;
}

export interface PipanHostGlobalApi {
  readonly version: string;
  registerExtension: (registration: PipanRuntimeExtensionRegistration) => void;
  openPanel: (panelId: string) => void;
  closePanel: (panelId: string) => void;
  openMainView: (viewId: string) => void;
}

declare global {
  interface Window {
    pipan?: PipanHostGlobalApi;
  }
}
