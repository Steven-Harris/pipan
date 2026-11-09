# Features

Explore the core features of PiPan.

## Split Workspace {#split-workspace}

PiPan adapts to your workflow. It opens to a simple chat view by default. You can open auxiliary tools only when you need them.

### Layout Options

- **Vertical Split**: Places the chat on the left and the active tool panel on the right. Best for widescreen monitors.
- **Horizontal Split**: Places the active tool panel on top and the chat on the bottom. Best for compact displays.
- **Draggable Splitter**: Drag the divider line to change panel sizes. PiPan remembers your split ratio.

### Auxiliary Panels

Toggle any panel from the top toolbar:
- **Node Graph**: Shows the active workflow as a visual graph.
- **Terminal**: Opens a command-line shell in the workspace folder.
- **Files**: Lets you browse and attach project files.
- **Pull Requests**: Displays GitHub pull requests and diffs.

## DAG Canvas {#dag-canvas}

The Directed Acyclic Graph (DAG) canvas displays agent activity as an interactive flowchart. It uses `@xyflow/svelte` and `dagre`.

### Node Types

- **Orchestrator Node**: The parent planning agent. Shows main goals and task status.
- **Worker Node**: Subagents assigned to specific subtasks.
- **Action Node**: Individual tool executions, such as file edits, searches, or bash commands.
- **Approval Node**: Interactive checkpoints. The agent pauses until you approve or reject a dangerous operation.

### Direction & Layout

- **Top-to-Bottom (`TB`)**: Hierarchical tree layout flowing downward.
- **Left-to-Right (`LR`)**: Horizontal timeline layout flowing left to right.
- **Per-Session Coordinates**: Move any node with your mouse. PiPan saves your custom positions in SQLite.
- **Reset Layout**: Click to re-run auto-layout and snap nodes back into standard order.

## GitHub Integration {#github-integration}

PiPan connects directly to GitHub through the native `gh` CLI. It does not need background polling loops or heavy MCP servers.

### Pull Request Browser

Filter and inspect PRs with built-in views:
- **Assigned to Me**: Open pull requests assigned to your account.
- **Review Requested**: Pull requests waiting for your review.
- **Created by Me**: Pull requests you authored.
- **Custom Filters**: Build custom queries with standard GitHub search syntax.

### Review and Merge

- **Diff Viewer**: Review changes with file tree navigation and colored unified diffs.
- **Inline Comments**: Click any line in a diff to add a review comment.
- **Submit Reviews**: Approve, comment, or request changes with one click.
- **Merge Actions**: Merge via merge commit, squash, or rebase, with optional branch deletion.
- **Live Checks**: View passing, failing, and running CI workflow checks.

## Terminal & Explorer {#terminal-and-explorer}

Work directly with your project files and system shell inside PiPan.

### Workspace Terminal

- Spawns a real shell process inside the current project root.
- Runs PowerShell or CMD on Windows; Bash or Zsh on macOS and Linux.
- Supports full ANSI color output and scrollback history.
- **Send to Pi**: Click to route command output directly into the agent's context window.

### File Explorer

- Shows the full project directory tree.
- Respects `.gitignore` rules automatically.
- Highlights Git file changes: Added (`A`), Modified (`M`), Untracked (`?`), and Deleted (`D`).
- Click **Attach to Prompt** to include file paths and snippets in your next chat message.

## Doctor {#doctor}

The Doctor utility verifies your system environment and repairs missing requirements.

### Automated Checks

1. **Git**: Checks Git installation and your configured user name and email.
2. **Node & Package Manager**: Validates Node.js and `npm` or `pnpm`.
3. **Pi CLI**: Confirms the `pi` command exists on your system PATH.
4. **Pi Auth**: Checks for valid provider credentials in `~/.pi/agent/auth.json`.
5. **GitHub CLI**: Confirms `gh` is installed and ready.
6. **GitHub Auth**: Validates your current `gh auth status`.
7. **Webhook Extension**: Verifies the `gh-webhook` CLI extension.

### One-Click Auto-Fix

When an issue appears, Doctor displays instructions and a **Fix for Me** button:
- Installs missing CLI tools via your system package manager.
- Launches device code login flows for GitHub and Pi providers.
- Refreshes environment variables without restarting the application.
