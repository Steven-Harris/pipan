# Extensions

Extend PiPan with custom views, panels, and tools.

## Plugin Guide {#plugin-guide}

PiPan features a modular plugin architecture inspired by VS Code. You can install community plugins or build your own.

### Built-in Extensions

PiPan includes five core plugins by default:

1. **`pipan.node-graph`**: Live Directed Acyclic Graph (DAG) for agent workflows.
2. **`pipan.github-prs`**: GitHub pull request viewer, code review, and merge workflows.
3. **`pipan.terminal`**: Integrated terminal panel with direct Pi command injection.
4. **`pipan.files`**: Project file tree explorer with Git status indicators.
5. **`pipan.cost-analytics`**: Token usage breakdown and cost tracking.

### Plugin Folder Structure

PiPan discovers extensions in `~/.pipan/extensions/`:

```
~/.pipan/extensions/
└── my-custom-plugin/
    ├── manifest.json
    ├── index.js
    └── icon.svg
```

### Creating Your First Plugin

Follow these steps to create a simple plugin:

1. Create a folder in `~/.pipan/extensions/hello-plugin/`.
2. Add a `manifest.json` file:

```json
{
  "id": "com.example.hello-plugin",
  "name": "Hello Plugin",
  "version": "1.0.0",
  "description": "Adds a greeting panel to PiPan.",
  "author": "Your Name",
  "entry": "index.js",
  "contributes": {
    "panels": [
      {
        "id": "hello-panel",
        "title": "Hello Panel",
        "icon": "hand",
        "defaultPosition": "split"
      }
    ]
  }
}
```

3. Add `index.js`:

```javascript
window.pipan.registerExtension({
  id: "com.example.hello-plugin",
  renderPanel(container, context) {
    container.innerHTML = `
      <div style="padding: 1rem;">
        <h2>Hello from PiPan Extension!</h2>
        <p>Current workspace: ${context.workspacePath}</p>
      </div>
    `;
  }
});
```

4. Open PiPan. The new panel appears in your auxiliary panel list.

### Channel API

A plugin whose id equals a Pi extension's [channel](./channels) id can build custom UI for it. `window.pipan.channel(id)` returns:

| Method | Purpose |
| :--- | :--- |
| `subscribe(topic, callback)` | Calls `callback` with the latest state of a topic for the viewed session, then on every change. `null` means nothing is published. Returns an unsubscribe function. |
| `onRequest(kind, handler)` | Renders requests of that kind from the channel. `handler(container, request)` draws into `container` and finishes with `request.answer(payload)` or `request.cancel()`. Returns an unsubscribe function. |
| `intent(name, payload)` | Sends an intent to the producer. Resolves `false` when no producer can receive it. |

`registerExtension` also accepts `messageRenderers`, a map from a log message name to `(message, container) => cleanup`. It draws the producer's `as: "message"` log entries in the chat transcript. Messages without a renderer stay hidden.

The containers that plugins draw into (panels, request views and message renderers) give native `button`, `input`, `textarea` and `select` elements a plain dark baseline, because PiPan's CSS reset would otherwise leave them looking like text. Styles you set yourself take precedence.

Channel ids are self-declared, so any plugin can read any channel. Plugins run with the same trust as the rest of the webview.

[`examples/channel-plugin`](https://github.com/Steven-Harris/pipan/tree/main/examples/channel-plugin) shows all three. [Writing a channel producer](./channels-authoring) covers the Pi side.

### Installing and Managing Plugins

- **From Local Directory**: Click **Install from Folder** in the Extensions tab and select the extension folder.
- **From Git**: Click **Install from Git** and paste a repository URL.
- **Enable / Disable**: Toggle the switch next to any extension to activate or deactivate it.

## Manifest Schema {#manifest-schema}

Every plugin requires a valid `manifest.json` file in its root directory.

### TypeScript Definition

```typescript
export interface PiPanExtensionManifest {
  /** Unique reverse-domain identifier */
  id: string;

  /** Display name shown in UI */
  name: string;

  /** Semantic version string */
  version: string;

  /** Short description of plugin features */
  description: string;

  /** Author or organization name */
  author: string;

  /** True for built-in PiPan extensions */
  builtin?: boolean;

  /** Whether the plugin enables automatically on install */
  enabledByDefault?: boolean;

  /** Relative path to the plugin JavaScript entry point */
  entry?: string;

  /** Extension points contributed by this plugin */
  contributes: {
    /** Auxiliary workspace panels */
    panels?: Array<{
      id: string;
      title: string;
      icon: string;
      defaultPosition?: 'split' | 'bottom';
    }>;

    /** Sidebar navigation tabs */
    sidebarTabs?: Array<{
      id: string;
      title: string;
      icon: string;
    }>;

    /** Full-page views */
    mainViews?: Array<{
      id: string;
      title: string;
      icon: string;
    }>;

    /** Status bar widgets */
    statusBarItems?: Array<{
      id: string;
      alignment: 'left' | 'right';
      priority?: number;
    }>;

    /** Chat slash commands */
    slashCommands?: Array<{
      name: string;
      description: string;
    }>;

    /** Topics of the plugin's own channel that PiPan renders with a built-in primitive */
    channels?: Array<{
      topic: string;
      primitive: 'graph';
      title: string;
      icon?: string;
      defaultPosition?: 'split' | 'bottom';
    }>;
  };
}
```

### Schema Properties

| Property | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `id` | string | Yes | Unique extension identifier |
| `name` | string | Yes | User-facing display name |
| `version` | string | Yes | SemVer version string (e.g. `1.0.0`) |
| `description` | string | Yes | Short explanation of what the plugin does |
| `author` | string | Yes | Author or team name |
| `entry` | string | No | Path to the main JavaScript module |
| `builtin` | boolean | No | Identifies internal PiPan components |
| `enabledByDefault` | boolean | No | Default enablement state |
| `contributes` | object | Yes | Contribution points registered by the plugin |

### Channel contributions

`contributes.channels` fixes the title, icon and placement of a graph panel for a topic of the plugin's own channel (the channel id is the plugin id). The panel is listed in the panel picker before the producer publishes anything, and opens by itself the first time the producer publishes that topic in a session. `primitive` must be `graph`.
