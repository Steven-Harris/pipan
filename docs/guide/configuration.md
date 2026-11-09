# Configuration

Configure models, providers, agent behaviors, and workspace preferences.

## Overview

Pipan stores settings in local JSON files and an SQLite database. You can manage most settings through the in-app Settings modal or by editing configuration files directly.

## Provider & Model Setup

Pipan supports all major LLM providers supported by the `pi` engine.

### Supported Providers

- **Anthropic**: Claude 3.5 Sonnet, Claude 3 Opus, Claude 3.5 Haiku.
- **OpenAI**: GPT-4o, GPT-4o mini, o1, o3-mini.
- **Google Gemini**: Gemini 1.5 Pro, Gemini 2.0 Flash.
- **OpenRouter**: Access hundreds of open-source and proprietary models.
- **Custom Endpoints**: Connect to any OpenAI-compatible local or remote proxy.

### Managing Credentials

Store API keys securely in `~/.pi/agent/auth.json`:

```json
{
  "anthropic": {
    "apiKey": "sk-ant-..."
  },
  "openai": {
    "apiKey": "sk-proj-..."
  }
}
```

The Pipan Settings dialog masks your keys in the UI to protect sensitive credentials during screen shares.

## Agent Behavior

Tune how the agent behaves during long sessions.

### Thinking Level

Controls reasoning trace detail for thinking models:
- **Off**: Skips explicit reasoning tokens.
- **Low**: Produces brief internal plans.
- **Medium**: Balances depth and speed (recommended).
- **High**: Emits comprehensive step-by-step reasoning.

### Context Compaction

When conversation history grows close to model context limits, Pipan triggers compaction:
- Compresses previous turns into a concise summary.
- Preserves file paths, test results, and recent context.
- Prevents context overflow errors.

### Steering and Follow-up Modes

- **Steering Mode**: Injects instructions directly into the active generation stream.
- **Follow-up Mode**: Queues instructions and sends them when the current turn finishes.
- **Auto-Retry**: Automatically retries failed network calls after temporary rate limits.

## GitHub Authentication

Pipan supports two login methods for GitHub features:

### 1. Browser Login (Recommended)

1. Open **Settings > GitHub**.
2. Click **Login with Browser**.
3. Pipan copies a one-time 8-character device code and opens GitHub in your browser.
4. Paste the code and grant access.
5. Pipan detects successful authentication automatically.

### 2. Personal Access Token

1. Generate a token on GitHub with `repo`, `read:org`, and `workflow` scopes.
2. Open **Settings > GitHub**.
3. Paste the token into the **Personal Access Token** field and click **Save**.

## Configuration Files

Pipan uses standard paths for configuration and data:

| File / Folder | Purpose |
| :--- | :--- |
| `~/.pi/agent/auth.json` | API keys and provider credentials |
| `~/.pi/agent/settings.json` | Default provider, model, and agent flags |
| `~/.pi/agent/models.json` | Custom model definitions and local endpoints |
| `<app_data>/pipan.db` | Local SQLite database (sessions, tags, graph coordinates) |
| `<app_data>/sessions/` | Raw `.jsonl` session transcript files |

`<app_data>` maps to:
- **Windows**: `%LOCALAPPDATA%\pipan`
- **macOS**: `~/Library/Application Support/pipan`
- **Linux**: `~/.local/share/pipan`

## Reference Settings {#reference}

Summary of common configuration properties:

| Key | Type | Default | Description |
| :--- | :--- | :--- | :--- |
| `defaultProvider` | string | `"anthropic"` | Active LLM provider |
| `defaultModel` | string | `"claude-3-5-sonnet"` | Model used for new sessions |
| `thinkingLevel` | string | `"medium"` | Reasoning detail (`off`, `low`, `medium`, `high`) |
| `autoCompaction` | boolean | `true` | Compresses long conversation context |
| `autoRetry` | boolean | `true` | Retries failed provider requests |
| `splitOrientation` | string | `"vertical"` | Default workspace layout (`vertical`, `horizontal`) |
| `graphDirection` | string | `"TB"` | Default DAG canvas layout (`TB`, `LR`) |
