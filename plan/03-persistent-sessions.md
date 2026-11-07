# 3. Persistent Sessions: Stop, Search, Group, Filter, Sort, Rename, Archive & Delete (Items #6, #7, #8, #9)

## Backend Changes (`src-tauri/src/pi_rpc.rs`, `src-tauri/src/storage.rs`)
- **Real Pi Session Persistence**:
  - Update `build_pi_command_with_args` so persistent sessions run without `--no-session`, passing `--session <session_jsonl_path>` (stored under `<app_local_data_dir>/sessions/<session_id>.jsonl` or `~/.pi/agent/sessions/`) and `--name <label>` when configured. Keep an explicit ephemeral option only when requested.
  - Support switching/resuming sessions via Pi RPC `switch_session` (`{"type": "switch_session", "sessionPath": "..."}`) and `get_messages` / `get_state` hydration when reopening a saved session.
- **Pipan Local Metadata Store (`src-tauri/src/storage.rs`)**:
  - Persist `<app_local_data_dir>/pipan-metadata.json` containing:
    - `sessions: Record<sessionId, SessionRecord>` with `id`, `name`, `sessionFilePath`, `cwd`, `mode`, `group`, `tags`, `archived`, `pinned`, `createdAt`, `updatedAt`, `messageCount`, `toolCallCount`, `totalTokens`, `totalCostUsd`, `modelId`, `provider`, and `nodePositions`.
    - Tauri commands: `list_saved_sessions`, `save_session_metadata`, `rename_session`, `archive_session`, `delete_session`.

## Frontend Changes (`src/lib/stores/chat.svelte.ts`, `src/lib/components/Sidebar.svelte`, `SessionHeader.svelte`)
- **Stop Running Sessions (#6)**:
  - Add a Stop button directly on every running session item in `Sidebar.svelte`, in `SessionHeader.svelte`, and in `ChatInput.svelte`.
  - Clicking Stop on an active generation sends `clear_queue` + `abort`; clicking Stop Process terminates the child sidecar (`stop_pi_session` or `stop_agent_session`).
- **Search, Group, Filter, and Sort (#7)**:
  - **Search**: Real-time filter input matching session name, folder path, group, tags, or branch.
  - **Filter**: Quick filter pills/dropdown for `Active` (non-archived default), `Running`, `Archived`, `Directory`, `Headless`, `Worktree`.
  - **Group By**: `None`, `Workspace Folder`, `Custom Group`, `Date Bucket (Today / This Week / Older)`.
  - **Sort By**: `Last Updated` (default), `Created Date`, `Name (A–Z)`, `Total Cost`, `Message Count`.
- **Rename (#9)**:
  - Inline double-click / edit button on session cards and in `SessionHeader.svelte`. Updates `SessionRecord.name` in `pipan-metadata.json` and sends `{"type": "set_session_name", "name": newName}` over Pi RPC if connected.
- **Archive & Delete (#8)**:
  - Archive toggle moves sessions into/out of the Archived filter view.
  - Delete button removes the session from metadata, deletes its `.jsonl` session file on disk, stops any running process for that session, and prunes any associated git worktree.
