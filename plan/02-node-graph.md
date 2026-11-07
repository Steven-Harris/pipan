# 2. Node Graph Rendering, Viewport Fitting & Per-Session Persistence (Item #2)

## Layout Direction: Top-to-Bottom & Left-to-Right
- `layout.ts` already exports `LayoutDirection = 'TB' | 'LR'`, and `layoutGraph()` already sets `rankdir` and the handle sides. Make `LR` a fully supported, first-class mode rather than a side toggle.
- **Layout and handles**: `layoutGraph()` stamps `sourcePosition` / `targetPosition` onto every node (`Right` / `Left` for `LR`, `Bottom` / `Top` for `TB`). Node components (`OrchestratorNode`, `WorkerNode`, `ActionNode`, `ApprovalNode`, `FlowNodeCard`) must render their `<Handle>`s from those props, not hardcoded sides, and edges must attach correctly in both directions.
- **Active rank axis placement**: A new child node placed beside its parent goes below the parent in `TB` and to the right of it in `LR`. Rank spacing uses node height in `TB` and node width in `LR`.
- **Toolbar control**: A labelled two-state toggle in the canvas toolbar (`Top → Bottom` / `Left → Right` icons, `aria-pressed`, tooltip), replacing the current unlabelled cycle button.
- **Direction change is structural**: Switching direction re-runs Dagre for unpinned nodes and re-fits the viewport (`fitView`). It does not rebuild the graph.
- **Positions are per direction**: Dragged coordinates only make sense in the direction they were saved in. Persist them keyed by direction (see persistence below), so toggling `TB` ↔ `LR` ↔ `TB` restores each layout. `Reset Layout` clears pinned coordinates for the active direction only.
- **Persistence and default**: Save the chosen direction per session as `sessions.graph_direction` (`'TB' | 'LR'`). The default is `'LR'` in the side-by-side split (wide, short canvas) and `'TB'` in the top/bottom split. Restore it on session open; an explicit user choice always wins over the split-based default.
- **Streaming stays stable**: Direction is part of the structural-change key only. Streaming deltas must not trigger re-layout in either direction.

## Root Cause Fixes
- **Decouple Streaming Deltas from Dagre Re-Layout**:
  - Previously, `$effect` in `FlowCanvas.svelte` re-ran `dagre.layout()` on every streaming character delta, overwriting dragged positions and causing sporadic node jumping.
  - Separate **structural graph topology changes** (node added/removed, edge added/removed, layout direction changed) from **data updates** (streaming text, thinking deltas, tool status updates). Data updates mutate `node.data` in place while preserving `node.position`.
- **Compact, Predictable Placement & Viewport Containment**:
  - Update `layout.ts` with tighter, consistent spacing and accurate node bounding sizes, tuned per direction: `TB` uses `nodesep: 28`, `ranksep: 44`; `LR` uses `nodesep: 24`, `ranksep: 72`, because ranks are separated by node width rather than height. Both use `marginx: 20`, `marginy: 20`.
  - When a new node is added, calculate layout only for unpinned nodes (or place new child nodes directly adjacent to their parent node along the active rank axis) so existing user-positioned nodes never jump.
  - Trigger `fitView({ padding: 0.16, maxZoom: 1.0, duration: 200 })` (or viewport pan-into-view) when structural nodes are added so nodes always render within the current visible canvas unless the canvas is smaller than a single node.
- **Per-Session Node Position Persistence**:
  - Track `onnodeclick` / `onnodedragstop` in `FlowCanvas.svelte` and save `{ [nodeId]: { x, y, pinned?: boolean } }` into the `node_positions` table of `pipan.db` for the active session and direction (columns `session_id`, `direction`, `node_id`, `x`, `y`, `pinned`; PK `(session_id, direction, node_id)`; via a `save_node_positions` command; loaded back as `{ [nodeId]: { x, y, pinned } }` for the active direction).
  - When navigating away (to Cost, Settings, PRs, or another session) and returning to a session, restore the saved `nodePositions` map before rendering. Provide a `"Reset Layout"` button in the canvas toolbar to clear pinned coordinates and re-run Dagre.
