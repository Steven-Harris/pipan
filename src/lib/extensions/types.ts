import type { Network } from 'lucide-svelte';
import type { ChannelApi, ChannelMessageRenderer } from '#lib/channels/plugin-api.svelte.ts';

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

/**
 * A topic of the plugin's own channel (the channel id is the plugin id) that PiPan renders with a
 * built-in primitive. Declaring it gives the panel a fixed title, icon and placement, and lists
 * it in the panel picker before the producer has published anything.
 */
export interface ExtensionChannelContribution {
  topic: string;
  primitive: 'graph';
  title: string;
  icon?: string;
  defaultPosition?: ExtensionPanelDefaultPosition;
}

export interface PiPanExtensionContributes {
  panels?: ExtensionPanelContribution[];
  sidebarTabs?: ExtensionSidebarTabContribution[];
  mainViews?: ExtensionMainViewContribution[];
  statusBarItems?: ExtensionStatusBarItemContribution[];
  slashCommands?: ExtensionSlashCommandContribution[];
  channels?: ExtensionChannelContribution[];
}

export interface PiPanExtensionManifest {
  id: string;
  name: string;
  version: string;
  description: string;
  author: string;
  builtin?: boolean;
  enabledByDefault?: boolean;
  contributes: PiPanExtensionContributes;
  entry?: string; // Relative JS module path for external plugins
}

export interface ExternalExtensionBundle {
  manifest: PiPanExtensionManifest;
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

export interface PiPanExtensionRenderContext {
  extensionId: string;
  contributionId: string;
  kind: 'panel' | 'sidebarTab' | 'mainView';
  manifest: PiPanExtensionManifest;
}

export type PiPanContributionRenderFn = (
  container: HTMLElement,
  context: PiPanExtensionRenderContext
) => void | (() => void);

export interface PiPanRuntimeStatusBarItemState {
  text: string;
  tooltip?: string;
  badge?: string;
  onClick?: () => void;
}

export interface PiPanRuntimeExtensionRegistration {
  id: string;
  manifest?: Partial<PiPanExtensionManifest>;
  panels?: Record<string, PiPanContributionRenderFn | { html?: string; text?: string }>;
  sidebarTabs?: Record<string, PiPanContributionRenderFn | { html?: string; text?: string }>;
  mainViews?: Record<string, PiPanContributionRenderFn | { html?: string; text?: string }>;
  statusBarItems?: Record<string, PiPanRuntimeStatusBarItemState>;
  slashCommands?: Record<string, (args: string) => void | Promise<void> | string | Promise<string>>;
  /**
   * Renders log messages the plugin's producer wrote with `as: "message"`, in the chat transcript.
   * The key is the log `name`. Messages without a renderer stay hidden.
   */
  messageRenderers?: Record<string, ChannelMessageRenderer>;
  activate?: () => void | (() => void);
  deactivate?: () => void;
}

export interface RegisteredExtension {
  manifest: PiPanExtensionManifest;
  builtin: boolean;
  installed: boolean;
  enabled: boolean;
  aliases: string[];
  installPath?: string;
  sourceUrl?: string;
  entryCode?: string;
  error?: string | null;
}

export interface PiPanHostGlobalApi {
  readonly version: string;
  registerExtension: (registration: PiPanRuntimeExtensionRegistration) => void;
  openPanel: (panelId: string) => void;
  closePanel: (panelId: string) => void;
  openMainView: (viewId: string) => void;
  /** Channel API for the Pi extension that publishes under `extensionId`. */
  channel: (extensionId: string) => ChannelApi;
}

declare global {
  interface Window {
    pipan?: PiPanHostGlobalApi;
  }
}
