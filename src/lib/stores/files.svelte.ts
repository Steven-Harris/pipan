import { invoke } from '@tauri-apps/api/core';
import { untrack } from 'svelte';
import { chatStore } from '#lib/stores/chat.svelte.ts';
import type {
  GitFileStatusCode,
  WorkspaceFileEntry,
  WorkspaceFilePreview
} from '#lib/types/pi.ts';

export interface FlattenedTreeNode {
  entry: WorkspaceFileEntry;
  depth: number;
  expanded: boolean;
  loading: boolean;
  gitStatus?: GitFileStatusCode;
}

class FileExplorerStore {
  cwd = $state<string | null>(null);
  entriesByDir = $state<Record<string, WorkspaceFileEntry[]>>({});
  expandedDirs = $state<Record<string, boolean>>({});
  loadingDirs = $state<Record<string, boolean>>({});
  gitStatuses = $state<Record<string, GitFileStatusCode>>({});

  includeHidden = $state<boolean>(false);
  filterQuery = $state<string>('');

  selectedRelativePath = $state<string | null>(null);
  preview = $state<WorkspaceFilePreview | null>(null);
  previewTab = $state<'code' | 'diff'>('code');
  loadingPreview = $state<boolean>(false);
  loadingRoot = $state<boolean>(false);
  error = $state<string | null>(null);

  recentlyAttachedPath = $state<string | null>(null);
  private attachTimer: ReturnType<typeof setTimeout> | null = null;
  private initializedOnce = false;

  /** Flattened visible tree rows in depth-first order, respecting expansion & filterQuery. */
  readonly visibleTreeNodes = $derived.by((): FlattenedTreeNode[] => {
    const query = this.filterQuery.trim().toLowerCase();
    const out: FlattenedTreeNode[] = [];

    const walk = (relDir: string, depth: number) => {
      const list = this.entriesByDir[relDir] ?? [];
      for (const entry of list) {
        const status = this.resolveEntryGitStatus(entry);
        const isExpanded = Boolean(this.expandedDirs[entry.relativePath]);
        const isLoading = Boolean(this.loadingDirs[entry.relativePath]);

        if (query) {
          const selfMatches =
            entry.name.toLowerCase().includes(query) ||
            entry.relativePath.toLowerCase().includes(query);
          const hasMatchingDescendant =
            entry.isDir && this.hasMatchingChild(entry.relativePath, query);

          if (!selfMatches && !hasMatchingDescendant) {
            continue;
          }

          out.push({
            entry,
            depth,
            expanded: isExpanded || hasMatchingDescendant,
            loading: isLoading,
            gitStatus: status
          });

          if (entry.isDir && (isExpanded || hasMatchingDescendant)) {
            walk(entry.relativePath, depth + 1);
          }
        } else {
          out.push({
            entry,
            depth,
            expanded: isExpanded,
            loading: isLoading,
            gitStatus: status
          });
          if (entry.isDir && isExpanded) {
            walk(entry.relativePath, depth + 1);
          }
        }
      }
    };

    walk('', 0);
    return out;
  });

  readonly modifiedCount = $derived(Object.keys(this.gitStatuses).length);

  private hasMatchingChild(relDir: string, query: string): boolean {
    const children = this.entriesByDir[relDir];
    if (!children) return false;
    for (const child of children) {
      if (
        child.name.toLowerCase().includes(query) ||
        child.relativePath.toLowerCase().includes(query)
      ) {
        return true;
      }
      if (child.isDir && this.hasMatchingChild(child.relativePath, query)) {
        return true;
      }
    }
    return false;
  }

  resolveEntryGitStatus(entry: WorkspaceFileEntry): GitFileStatusCode | undefined {
    const direct = this.gitStatuses[entry.relativePath];
    if (direct) return direct;
    if (!entry.isDir) return undefined;

    const prefix = `${entry.relativePath}/`;
    let foundStatus: GitFileStatusCode | undefined;
    for (const [relPath, code] of Object.entries(this.gitStatuses)) {
      if (relPath.startsWith(prefix)) {
        if (code === 'M' || code === 'D' || code === 'U') {
          return 'M';
        }
        foundStatus = foundStatus ?? code;
      }
    }
    return foundStatus;
  }

  async syncWorkspace(targetCwd: string | null): Promise<void> {
    const cleaned = targetCwd?.trim() || null;
    await untrack(async () => {
      if (this.initializedOnce && cleaned === this.cwd) {
        if (!cleaned || this.loadingRoot || this.entriesByDir[''] !== undefined) {
          return;
        }
      }
      this.initializedOnce = true;
      this.cwd = cleaned;
      this.entriesByDir = {};
      this.expandedDirs = {};
      this.loadingDirs = {};
      this.gitStatuses = {};
      this.selectedRelativePath = null;
      this.preview = null;
      this.error = null;

      if (cleaned) {
        await this.refresh();
      }
    });
  }

  async refresh(): Promise<void> {
    if (!this.cwd) return;
    this.loadingRoot = true;
    this.error = null;

    try {
      const expandedKeys = Object.entries(this.expandedDirs)
        .filter(([, open]) => open)
        .map(([k]) => k);

      await Promise.all([
        this.loadDirectory(''),
        this.refreshGitStatuses(),
        ...expandedKeys.map((dir) => this.loadDirectory(dir))
      ]);

      if (this.selectedRelativePath) {
        await this.loadFilePreview(this.selectedRelativePath, true);
      }
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loadingRoot = false;
    }
  }

  async refreshGitStatuses(): Promise<void> {
    if (!this.cwd) return;
    try {
      const statuses = await invoke<Record<string, GitFileStatusCode>>('get_git_file_statuses', {
        cwd: this.cwd
      });
      this.gitStatuses = statuses ?? {};
    } catch {
      this.gitStatuses = {};
    }
  }

  async loadDirectory(relativeDir: string): Promise<void> {
    if (!this.cwd) return;
    const key = relativeDir.trim();
    this.loadingDirs = { ...this.loadingDirs, [key]: true };

    try {
      const entries = await invoke<WorkspaceFileEntry[]>('list_workspace_files', {
        cwd: this.cwd,
        relativeDir: key || null,
        includeHidden: this.includeHidden
      });
      this.entriesByDir = {
        ...this.entriesByDir,
        [key]: entries
      };
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loadingDirs = { ...this.loadingDirs, [key]: false };
    }
  }

  async toggleDirectory(relativeDir: string): Promise<void> {
    const key = relativeDir.trim();
    const nextOpen = !this.expandedDirs[key];
    this.expandedDirs = {
      ...this.expandedDirs,
      [key]: nextOpen
    };
    if (nextOpen && !this.entriesByDir[key]) {
      await this.loadDirectory(key);
    }
  }

  async toggleHiddenFiles(): Promise<void> {
    this.includeHidden = !this.includeHidden;
    await this.refresh();
  }

  async selectEntry(entry: WorkspaceFileEntry): Promise<void> {
    if (entry.isDir) {
      await this.toggleDirectory(entry.relativePath);
      return;
    }
    await this.loadFilePreview(entry.relativePath, false);
  }

  async loadFilePreview(relativePath: string, preserveTab = false): Promise<void> {
    if (!this.cwd) return;
    this.selectedRelativePath = relativePath;
    this.loadingPreview = true;
    this.error = null;

    try {
      const result = await invoke<WorkspaceFilePreview>('read_workspace_file', {
        cwd: this.cwd,
        relativePath,
        maxBytes: 256 * 1024
      });
      const status = this.gitStatuses[relativePath] ?? result.gitStatus;
      this.preview = {
        ...result,
        gitStatus: status
      };
      if (!preserveTab) {
        if (!result.content && result.diff) {
          this.previewTab = 'diff';
        } else {
          this.previewTab = 'code';
        }
      } else if (this.previewTab === 'diff' && !result.diff) {
        this.previewTab = 'code';
      }
    } catch (err) {
      this.error = err instanceof Error ? err.message : String(err);
    } finally {
      this.loadingPreview = false;
    }
  }

  async attachToPrompt(fullPath: string): Promise<void> {
    const att = await chatStore.attachFileFromPath(fullPath);
    if (att) {
      this.recentlyAttachedPath = fullPath;
      if (this.attachTimer) clearTimeout(this.attachTimer);
      this.attachTimer = setTimeout(() => {
        if (this.recentlyAttachedPath === fullPath) {
          this.recentlyAttachedPath = null;
        }
      }, 2000);
    }
  }
}

export const fileExplorerStore = new FileExplorerStore();
