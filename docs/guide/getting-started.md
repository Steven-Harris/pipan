# Getting Started

Get up and running with Pipan.

## Introduction {#introduction}

Pipan is a native desktop app for the `pi` AI coding agent. It runs on Windows, macOS, and Linux.

Most agent tools wrap a terminal window. Pipan takes a different path. It runs `pi` as a background JSON-RPC child process. This architecture gives you total visibility and control:

- Read internal thoughts as they stream.
- Inspect tool calls, arguments, and run times.
- Steer the agent in mid-flight without waiting for it to finish.
- Keep your work tree safe in sandboxed folders.

## Installation {#installation}

Follow these steps to install Pipan and its requirements.

### Prerequisites

You need the `pi` CLI installed on your system PATH:

```bash
# Verify pi is installed
pi --version
```

If you do not have `pi`, install it first:

```bash
npm install -g @earendil-works/pi-coding-agent
```

Optional tools:
- `git`: Required for source control and worktree features.
- `gh`: Required for GitHub pull request views and reviews.

### Download Pipan

Download the latest installer from the [GitHub Releases](https://github.com/Steven-Harris/pipan/releases) page:

- **Windows**: Download the `.msi` or `.exe` installer.
- **macOS**: Download the `.dmg` file. Pipan supports both Apple Silicon and Intel chips.
- **Linux**: Download the `.AppImage`, `.deb`, or `.rpm` package.

### Build from Source

You can also build Pipan from source code:

```bash
# Clone the repository
git clone https://github.com/Steven-Harris/pipan.git
cd pipan

# Install dependencies
pnpm install

# Start the desktop app in development mode
pnpm tauri dev
```

## Quickstart {#quickstart}

Start your first session in three simple steps:

### Step 1: Launch Pipan

Open Pipan on your machine. The app opens to a clean chat window.

### Step 2: Choose a Workspace

Select how you want to work from the top bar:

- **Start Headless**: Pipan creates a temporary scratchpad folder. Use this mode to test ideas safely.
- **Open Folder**: Pick any project directory on your computer. Pipan sets this folder as the working directory.

### Step 3: Prompt and Steer

Type your instruction in the composer and click **Send**.

As the agent works:
- Click **Thinking** blocks to review the agent's internal plan.
- Click **Tool Call** cards to inspect file edits, searches, and commands.
- Use **Steer** to send notes while the agent is running.
- Use **Follow-up** to queue tasks for the next turn.
- Click **Stop** to cancel the current run immediately.
