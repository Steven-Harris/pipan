# 9. VS Code-Style Extension / Plugin System (Item #17)

## Architecture & Manifest Contract (`src/lib/extensions/registry.svelte.ts`, `src-tauri/src/extensions.rs`)
- **Extension Manifest (`PipanExtensionManifest`)**:
  ```ts
  export interface PipanExtensionManifest {
    id: string;
    name: string;
    version: string;
    description: string;
    author: string;
    builtin?: boolean;
    enabledByDefault?: boolean;
    contributes: {
      panels?: Array<{ id: string; title: string; icon: string; defaultPosition?: 'split' | 'bottom' }>;
      sidebarTabs?: Array<{ id: string; title: string; icon: string }>;
      mainViews?: Array<{ id: string; title: string; icon: string }>;
      statusBarItems?: Array<{ id: string; alignment: 'left' | 'right'; priority?: number }>;
      slashCommands?: Array<{ name: string; description: string }>;
    };
    entry?: string; // Relative JS module path for external plugins
  }
  ```
- **Repackaged Built-in Extensions**:
  - Repackage Pipan's modular capabilities as 5 first-party extensions managed through the exact same registry and toggleable/installable in the **Extensions Marketplace / Manager** tab:
    1. `pipan.node-graph` — **Orchestration Node Graph (DAG)** (contributes the Nodes split panel & graph inspector)
    2. `pipan.github-prs` — **GitHub Pull Requests & Reviews** (contributes the PR sidebar tab, PR session panel, and webhook status bar item)
    3. `pipan.terminal` — **Integrated Workspace Terminal** (contributes the Terminal session panel)
    4. `pipan.files` — **Workspace File Explorer** (contributes the Files session panel and prompt file tree picker)
    5. `pipan.cost-analytics` — **Cost & Usage Analytics** (contributes the Cost main view and status bar spend widget)
- **External / Custom User Plugins**:
  - Stored in `<app_local_data_dir>/extensions/<ext-id>/` or `~/.pipan/extensions/<ext-id>/`.
  - Backend commands (`src-tauri/src/extensions.rs`):
    - `list_external_extensions()`: Scans local extension directories and returns manifests + entry script contents.
    - `install_extension_from_folder(source_path)`: Copies/links a local plugin directory into `~/.pipan/extensions/`.
    - `install_extension_from_git(repo_url)`: Clones/downloads an extension from a GitHub repo URL into `~/.pipan/extensions/`.
    - `uninstall_extension(extension_id)`: Removes an installed extension.
  - External plugins register custom panels, sidebar views, status bar items, or slash commands via a sandboxed `window.pipan.registerExtension(...)` host API.
