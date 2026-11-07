# 8. In-App Login/Configuration & Dependency Doctor (Items #14, #15)

## Login & Configuration Modal/View (`src-tauri/src/config.rs`, `src/lib/components/views/SettingsView.svelte`) (Item #14)
- **Pi Authentication & Settings**:
  - Reads and writes `<agent-dir>/auth.json` (`~/.pi/agent/auth.json`) and `<agent-dir>/settings.json` safely in Rust:
    - Configure API keys for Anthropic, OpenAI, Google Gemini, OpenRouter, xAI, Mistral, Groq, or custom providers (stored with masked previews in UI).
    - Launch interactive `pi` provider OAuth/login helper in an embedded terminal flow if needed.
    - Edit default provider/model, default thinking level, `autoCompaction`, `autoRetry`, `steeringMode`, `followUpMode`, and custom model endpoints (`~/.pi/agent/models.json`).
- **GitHub Authentication & Settings**:
  - Shows current `gh auth status` (account login, scopes, protocol).
  - **Login with Browser (`gh auth login --web`)**: Displays the 8-character one-time device code with a "Copy & Open GitHub" button and polls/waits for the spawned `gh` process to finish authenticating.
  - **Login with Token**: Input field for Personal Access Token (`gh auth login --with-token`).
  - **Logout** button and default repository/webhook event preferences.

## System Doctor & Auto-Fixer (`src-tauri/src/doctor.rs`, `src/lib/components/modals/DoctorModal.svelte`) (Item #15)
- **Diagnostic Checks (`run_doctor`)**:
  1. `git`: Checks `git --version` and user `user.name` / `user.email` config.
  2. `node_npm`: Checks `node --version` and `npm`/`pnpm` availability.
  3. `pi_cli`: Checks `pi --version` on the augmented PATH.
  4. `pi_auth`: Checks whether at least one provider API key or credential is configured in `~/.pi/agent/auth.json` or environment variables.
  5. `gh_cli`: Checks `gh --version` on the augmented PATH.
  6. `gh_auth`: Checks `gh auth status` authentication state.
  7. `gh_webhook`: Checks `gh extension list` for `cli/gh-webhook`.
- **Auto-Fix Actions (`fix_doctor_issue(check_id)`)**:
  - Every failed or warning check displays both step-by-step manual instructions AND a **"Fix for Me"** button:
    - Missing `pi_cli`: Runs `npm install -g @earendil-works/pi-coding-agent` (or `pnpm add -g`) and refreshes augmented PATH cache.
    - Missing `gh_cli` / `git`: Runs platform package manager (`winget install --id GitHub.cli -e` / `brew install gh` / `scoop install gh`) with live output streaming.
    - Missing `gh_webhook`: Runs `gh extension install cli/gh-webhook`.
    - Missing `gh_auth` or `pi_auth`: Opens the corresponding guided login flow directly in the modal.
