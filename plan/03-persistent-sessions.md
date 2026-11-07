# 3. Persistent Sessions: Stop, Search, Group, Filter, Sort, Rename, Archive & Delete (Items #6, #7, #8, #9)

## Backend Changes (`src-tauri/src/pi_rpc.rs`, `src-tauri/src/storage.rs`)
- **Real Pi Session Persistence**:
  - Update `build_pi_command_with_args` so persistent sessions run without `--no-session`, passing `--session <session_jsonl_path>` (stored under `<app_local_data_dir>/sessions/<session_id>.jsonl` or `~/.pi/agent/sessions/`) and `--name <label>` when configured. Keep an explicit ephemeral option only when requested.
  - Support switching/resuming sessions via Pi RPC `switch_session` (`{"type": "switch_session", "sessionPath": "..."}`) and `get_messages` / `get_state` hydration when reopening a saved session.
- **Pipan Local SQLite Store (`src-tauri/src/storage.rs`)**:
  - Persist all Pipan-owned state in a single SQLite database at `<app_local_data_dir>/pipan.db`, using `rusqlite` (with the `bundled` feature) behind a `Mutex<Connection>` in Tauri managed state. Open with `PRAGMA journal_mode = WAL`, `PRAGMA foreign_keys = ON`, and track schema versions with `PRAGMA user_version` plus ordered migrations run at startup.
  - Schema:
    - `sessions`: `id` (PK), `name`, `session_file_path`, `cwd`, `mode`, `group_name`, `archived`, `pinned`, `created_at`, `updated_at`, `message_count`, `tool_call_count`, `total_tokens`, `total_cost_usd`, `model_id`, `provider`, `graph_direction` (`'TB'` | `'LR'`). Indexes on `updated_at`, `archived`, and `cwd`.
    - `session_tags`: `session_id` (FK, `ON DELETE CASCADE`), `tag`; PK `(session_id, tag)`.
    - `node_positions`: `session_id` (FK, `ON DELETE CASCADE`), `direction` (`'TB'` | `'LR'`), `node_id`, `x`, `y`, `pinned`; PK `(session_id, direction, node_id)`.
  - Tauri commands (all run via `spawn_blocking` so SQLite I/O never blocks the async runtime): `list_saved_sessions`, `save_session_metadata`, `rename_session`, `archive_session`, `delete_session`.
  - Search/filter/sort (#7) are pushed down into SQL queries (`LIKE` on name/cwd/group/tags, `ORDER BY` on `updated_at`, `created_at`, `name`, `total_cost_usd`, `message_count`) instead of loading and filtering the full list in the frontend.
  - **One-time import**: On first launch, if a legacy `pipan-metadata.json` exists, import it into `pipan.db` inside a single transaction, then rename it to `pipan-metadata.json.migrated`.
  - Pi itself still writes its native `.jsonl` transcript for each session (passed via `--session`); SQLite stores only Pipan's metadata and indexes, and references the transcript through `session_file_path`.

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
  - Inline double-click / edit button on session cards and in `SessionHeader.svelte`. Updates `sessions.name` in `pipan.db` and sends `{"type": "set_session_name", "name": newName}` over Pi RPC if connected.
- **Archive & Delete (#8)**:
  - Archive toggle moves sessions into/out of the Archived filter view.
  - Delete button removes the session row from `pipan.db` (child rows cascade), deletes its `.jsonl` session file on disk, stops any running process for that session, and prunes any associated git worktree.
