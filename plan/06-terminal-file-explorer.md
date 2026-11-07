# 6. Session Terminal & File Explorer Panels (Item #16)

## Integrated Workspace Terminal (`src-tauri/src/terminal.rs`, `src/lib/components/panels/TerminalPanel.svelte`)
- **Backend Shell Session Manager (`terminal.rs`)**:
  - Manages persistent interactive shell processes per session (`powershell.exe` / `cmd.exe` on Windows, `$SHELL` / `zsh` / `bash` on macOS/Linux) rooted in the active session's `cwd` or worktree.
  - Tauri commands: `start_terminal_session(session_id, cwd)`, `send_terminal_input(session_id, data)`, `resize_terminal`, `stop_terminal_session`.
  - Streams output lines/chunks over `terminal:output` and exit status over `terminal:exit`.
- **Frontend Terminal Panel (`TerminalPanel.svelte`)**:
  - Renders streaming ANSI/text output, command input with up/down command history, working directory indicator, Clear and Kill/Interrupt (`Ctrl+C`) buttons, and a **"Send to Pi (`bash` RPC)"** toggle that executes the command via Pi's `bash` RPC command so its output is automatically added to the LLM conversation context.

## Workspace File Explorer Panel (`src-tauri/src/files.rs`, `src/lib/components/panels/FilePanel.svelte`)
- **Backend File & Git Status Commands (`files.rs`)**:
  - `list_workspace_files(cwd, relative_dir)`: Lists directory entries (respecting `.gitignore` or toggling hidden files) with file size and type.
  - `get_git_file_statuses(cwd)`: Runs `git status --porcelain=v1` to annotate modified (`M`), added (`A`), untracked (`?`), and deleted (`D`) files.
  - `read_workspace_file(cwd, relative_path, max_bytes)`: Reads file content or `git diff` for preview.
- **Frontend File Panel (`FilePanel.svelte`)**:
  - Expandable directory tree with git status badges, quick file filter, read-only code/diff viewer, and a one-click **"+ Attach to Prompt"** button (or drag into `ChatInput`).
