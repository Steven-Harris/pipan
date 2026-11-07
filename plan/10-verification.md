# 10. Verification & Test Scenarios

1. **Type & Build Verification**:
   - `pnpm check` (`svelte-kit sync && svelte-check`) passes with zero TypeScript/Svelte errors.
   - `cargo check` and `cargo test` in `src-tauri/` compile cleanly and pass all unit tests (GitHub CLI args validation, SQLite session storage (in-memory DB: migrations, CRUD, cascade delete, legacy JSON import), usage JSONL parser, doctor checks, extension manifest validation).
2. **Chat-First & Split Panel Verification**:
   - Launch app -> verify clean Chat window renders by default with no Node canvas forced open.
   - Open Node View panel -> verify it opens in a **vertical (side-by-side)** split by default and can be toggled to a **horizontal (top/bottom)** split.
   - Open Terminal, File, and PR panels from the session window and verify tab switching and splitter resizing.
3. **Node Graph Stability & Persistence**:
   - Stream a multi-step prompt with thinking and tool calls -> verify nodes appear compactly inside the current viewport without overlapping or jumping during text streaming.
   - Drag nodes to custom positions, switch to another session or the Cost page, and switch back -> verify exact node positions are restored.
   - Toggle the graph between Top → Bottom and Left → Right -> verify handles and edges attach on the correct sides, nodes stay inside the viewport, and positions dragged in each direction are restored when toggling back. Reopen the session and verify the chosen direction persists.
4. **Session Lifecycle**:
   - Create multiple sessions, rename a session, assign groups, search/filter/sort in the sidebar, stop a running session mid-flight, archive a session, and delete a session.
5. **Model, Skills & Drag-and-Drop**:
   - Switch models and thinking levels from the selector; pick an installed skill via the Skills popover; drag and drop both an image and a source file into the prompt and verify they attach and send properly.
6. **GitHub PR Workflow**:
   - Verify default PR views (`Assigned to Me`, `Open PRs`), create a custom persistent `gh` search view, inspect PR checks/diffs, post an inline review comment, approve/request changes, and test merge options.
7. **Cost Page, Status Bar, Auth Config, Doctor & Extensions**:
   - Open Cost page and toggle Day / Week / Month / Year views; verify bottom status bar metrics; run Doctor and test auto-fix actions; open Extensions manager and toggle/install/disable built-in and custom extensions.
