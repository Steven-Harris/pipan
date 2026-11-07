# 2. Node Graph Rendering, Viewport Fitting & Per-Session Persistence (Item #2)

## Root Cause Fixes
- **Decouple Streaming Deltas from Dagre Re-Layout**:
  - Previously, `$effect` in `FlowCanvas.svelte` re-ran `dagre.layout()` on every streaming character delta, overwriting dragged positions and causing sporadic node jumping.
  - Separate **structural graph topology changes** (node added/removed, edge added/removed, layout direction changed) from **data updates** (streaming text, thinking deltas, tool status updates). Data updates mutate `node.data` in place while preserving `node.position`.
- **Compact, Predictable Placement & Viewport Containment**:
  - Update `layout.ts` with tighter, consistent spacing (`nodesep: 28`, `ranksep: 44`, `marginx: 20`, `marginy: 20`) and accurate node bounding sizes.
  - When a new node is added, calculate layout only for unpinned nodes (or place new child nodes directly adjacent to their parent node along the active rank axis) so existing user-positioned nodes never jump.
  - Trigger `fitView({ padding: 0.16, maxZoom: 1.0, duration: 200 })` (or viewport pan-into-view) when structural nodes are added so nodes always render within the current visible canvas unless the canvas is smaller than a single node.
- **Per-Session Node Position Persistence**:
  - Track `onnodeclick` / `onnodedragstop` in `FlowCanvas.svelte` and save `{ [nodeId]: { x, y, pinned?: boolean } }` into the active session's metadata (`sessionMeta.nodePositions`).
  - When navigating away (to Cost, Settings, PRs, or another session) and returning to a session, restore the saved `nodePositions` map before rendering. Provide a `"Reset Layout"` button in the canvas toolbar to clear pinned coordinates and re-run Dagre.
