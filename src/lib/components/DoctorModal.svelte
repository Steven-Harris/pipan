<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import {
    X,
    HeartPulse,
    CheckCircle2,
    AlertTriangle,
    XCircle,
    RefreshCw,
    Wrench,
    Loader2,
    Terminal,
    KeyRound,
    Globe,
    Copy,
    Check,
    Settings
  } from 'lucide-svelte';
  import { doctorStore } from '#lib/stores/doctor.svelte.ts';
  import { githubStore } from '#lib/stores/github.svelte.ts';
  import { BUILTIN_PROVIDERS_LIST } from '#lib/components/doctor-helpers.ts';

  // Inline guided fix inputs inside DoctorModal
  let gitNameInput = $state('');
  let gitEmailInput = $state('');

  let quickPiProvider = $state('anthropic');
  let quickPiApiKey = $state('');

  let quickGhToken = $state('');
  let copiedCode = $state(false);

  async function handleQuickSavePiKey() {
    if (!quickPiApiKey.trim()) return;
    const ok = await doctorStore.saveProviderKey(quickPiProvider, quickPiApiKey.trim());
    if (ok) {
      quickPiApiKey = '';
      doctorStore.activeGuidedCheckId = null;
      await doctorStore.runDoctor();
    }
  }

  async function handleQuickGhWebLogin() {
    const ok = await githubStore.loginWithWeb('github.com');
    if (ok) {
      doctorStore.activeGuidedCheckId = null;
      await doctorStore.loadGithubAuthDetails();
      await doctorStore.runDoctor();
    }
  }

  async function handleQuickGhTokenLogin() {
    if (!quickGhToken.trim()) return;
    const ok = await githubStore.loginWithToken(quickGhToken.trim(), 'github.com');
    if (ok) {
      quickGhToken = '';
      doctorStore.activeGuidedCheckId = null;
      await doctorStore.loadGithubAuthDetails();
      await doctorStore.runDoctor();
    }
  }

  async function copyAndOpenDevice(code: string, uri: string) {
    try {
      await navigator.clipboard.writeText(code);
      copiedCode = true;
      setTimeout(() => {
        copiedCode = false;
      }, 2500);
    } catch {
      // Ignore clipboard failures
    }
    try {
      await openUrl(uri);
    } catch {
      window.open(uri, '_blank', 'noopener,noreferrer');
    }
  }
</script>

{#if doctorStore.doctorModalOpen}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm"
    role="dialog"
    aria-modal="true"
    aria-label="PiPan Dependency Doctor"
  >
    <div class="flex max-h-[90vh] w-full max-w-3xl flex-col overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-950 text-zinc-100 shadow-2xl">
      <!-- Header -->
      <div class="flex flex-wrap items-center justify-between gap-3 border-b border-zinc-800/80 bg-zinc-900/70 px-5 py-3.5">
        <div class="flex items-center gap-2.5">
          <div
            class="flex h-8 w-8 items-center justify-center rounded-lg border {doctorStore.report?.overallStatus === 'fail'
              ? 'border-rose-500/30 bg-rose-500/15 text-rose-300'
              : doctorStore.report?.overallStatus === 'warn'
                ? 'border-amber-500/30 bg-amber-500/15 text-amber-300'
                : 'border-emerald-500/30 bg-emerald-500/15 text-emerald-300'}"
          >
            <HeartPulse class="h-4 w-4" />
          </div>
          <div>
            <h2 class="text-sm font-semibold text-zinc-100">System Dependency Doctor</h2>
            <p class="text-[11px] text-zinc-400">
              Diagnostic checks and one-click auto-fixers for Git, Node, Pi CLI, and GitHub CLI
            </p>
          </div>
        </div>

        <div class="flex items-center gap-2">
          <button
            type="button"
            onclick={() => {
              doctorStore.closeDoctorModal();
              doctorStore.openSettingsModal('pi-auth');
            }}
            class="inline-flex items-center gap-1.5 rounded-lg border border-zinc-700 bg-zinc-800/80 px-2.5 py-1.5 text-xs font-medium text-zinc-200 transition hover:bg-zinc-700 cursor-pointer"
          >
            <Settings class="h-3.5 w-3.5 text-indigo-400" />
            <span>Login & Settings</span>
          </button>

          <button
            type="button"
            disabled={doctorStore.runningDoctor}
            onclick={() => void doctorStore.runDoctor()}
            class="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-600/20 px-3 py-1.5 text-xs font-medium text-indigo-100 transition hover:bg-indigo-600/35 disabled:opacity-50 cursor-pointer"
          >
            <RefreshCw class="h-3.5 w-3.5 {doctorStore.runningDoctor ? 'animate-spin' : ''}" />
            <span>Re-run Checks</span>
          </button>

          <button
            type="button"
            onclick={() => doctorStore.closeDoctorModal()}
            class="rounded-lg p-1.5 text-zinc-400 transition hover:bg-zinc-800 hover:text-zinc-100 cursor-pointer"
            title="Close Doctor"
            aria-label="Close Doctor"
          >
            <X class="h-4 w-4" />
          </button>
        </div>
      </div>

      <!-- Summary Strip -->
      {#if doctorStore.report}
        <div class="grid grid-cols-3 gap-2 border-b border-zinc-800/80 bg-zinc-900/40 px-5 py-2.5 text-xs">
          <div class="flex items-center gap-2 rounded-lg border border-emerald-500/25 bg-emerald-950/20 px-3 py-1.5">
            <CheckCircle2 class="h-4 w-4 text-emerald-400" />
            <span class="font-semibold text-emerald-300">{doctorStore.report.passCount} Passed</span>
          </div>
          <div class="flex items-center gap-2 rounded-lg border border-amber-500/25 bg-amber-950/20 px-3 py-1.5">
            <AlertTriangle class="h-4 w-4 text-amber-400" />
            <span class="font-semibold text-amber-300">{doctorStore.report.warnCount} Warnings</span>
          </div>
          <div class="flex items-center gap-2 rounded-lg border border-rose-500/25 bg-rose-950/20 px-3 py-1.5">
            <XCircle class="h-4 w-4 text-rose-400" />
            <span class="font-semibold text-rose-300">{doctorStore.report.failCount} Failed</span>
          </div>
        </div>
      {/if}

      <!-- Checks List -->
      <div class="flex-1 overflow-y-auto p-5 space-y-3 text-xs">
        {#if doctorStore.doctorError}
          <div class="rounded-xl border border-rose-500/30 bg-rose-950/40 p-3 text-rose-200">
            {doctorStore.doctorError}
          </div>
        {/if}

        {#if !doctorStore.report && doctorStore.runningDoctor}
          <div class="flex items-center justify-center gap-2 py-12 text-zinc-400">
            <Loader2 class="h-5 w-5 animate-spin text-indigo-400" />
            <span>Running system diagnostics…</span>
          </div>
        {:else if doctorStore.report}
          {#each doctorStore.report.checks as check (check.id)}
            {@const logs = doctorStore.fixLogs[check.id] ?? []}
            {@const feedback = doctorStore.fixFeedback[check.id]}
            {@const isFixing = doctorStore.fixingCheckId === check.id}
            <div
              class="rounded-xl border p-3.5 space-y-2.5 transition {check.status === 'pass'
                ? 'border-zinc-800/90 bg-zinc-900/40'
                : check.status === 'warn'
                  ? 'border-amber-500/35 bg-amber-950/15'
                  : 'border-rose-500/35 bg-rose-950/15'}"
            >
              <!-- Check Row 1: Title, Status Badge, Fix Button -->
              <div class="flex flex-wrap items-start justify-between gap-2">
                <div class="flex items-start gap-2.5">
                  {#if check.status === 'pass'}
                    <CheckCircle2 class="mt-0.5 h-4 w-4 shrink-0 text-emerald-400" />
                  {:else if check.status === 'warn'}
                    <AlertTriangle class="mt-0.5 h-4 w-4 shrink-0 text-amber-400" />
                  {:else}
                    <XCircle class="mt-0.5 h-4 w-4 shrink-0 text-rose-400" />
                  {/if}

                  <div class="space-y-0.5">
                    <div class="flex flex-wrap items-center gap-2">
                      <span class="font-semibold text-zinc-100">{check.title}</span>
                      <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-400">
                        {check.id}
                      </span>
                      <span
                        class="rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider {check.status === 'pass'
                          ? 'bg-emerald-500/15 text-emerald-300'
                          : check.status === 'warn'
                            ? 'bg-amber-500/15 text-amber-300'
                            : 'bg-rose-500/15 text-rose-300'}"
                      >
                        {check.status}
                      </span>
                    </div>

                    <p class="font-mono text-[11px] text-zinc-300">{check.summary}</p>
                    {#if check.details}
                      <p class="text-[11px] text-zinc-400">{check.details}</p>
                    {/if}
                  </div>
                </div>

                {#if check.status !== 'pass' || check.canAutoFix}
                  <button
                    type="button"
                    disabled={isFixing}
                    onclick={() => void doctorStore.fixIssue(check.id)}
                    class="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-600/25 px-3 py-1.5 text-xs font-semibold text-indigo-100 transition hover:bg-indigo-600/40 disabled:opacity-50 cursor-pointer shrink-0"
                  >
                    {#if isFixing}
                      <Loader2 class="h-3.5 w-3.5 animate-spin" />
                      <span>Fixing…</span>
                    {:else}
                      <Wrench class="h-3.5 w-3.5 text-indigo-300" />
                      <span>{check.fixLabel || 'Fix for Me'}</span>
                    {/if}
                  </button>
                {/if}
              </div>

              <!-- Step-by-step Manual Instructions for non-passing checks -->
              {#if check.status !== 'pass' && check.manualInstructions.length > 0}
                <div class="rounded-lg border border-zinc-800/80 bg-zinc-950/70 p-2.5 space-y-1">
                  <div class="text-[10px] font-semibold uppercase tracking-wider text-zinc-400">
                    Manual Instructions
                  </div>
                  <ol class="list-decimal list-inside space-y-1 text-[11px] text-zinc-300 font-mono">
                    {#each check.manualInstructions as step (step)}
                      <li>{step}</li>
                    {/each}
                  </ol>
                </div>
              {/if}

              <!-- Guided Inline Flow for `git` Identity -->
              {#if check.id === 'git' && doctorStore.activeGuidedCheckId === 'git'}
                <div class="rounded-xl border border-indigo-500/30 bg-indigo-950/20 p-3 space-y-2.5">
                  <div class="font-semibold text-indigo-200">
                    Configure Global Git Identity (`user.name` & `user.email`)
                  </div>
                  <div class="grid gap-2 sm:grid-cols-2">
                    <input
                      type="text"
                      bind:value={gitNameInput}
                      placeholder="Your Name"
                      class="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 text-xs text-zinc-100"
                    />
                    <input
                      type="email"
                      bind:value={gitEmailInput}
                      placeholder="you@example.com"
                      class="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 text-xs text-zinc-100"
                    />
                  </div>
                  <div class="flex items-center gap-2">
                    <button
                      type="button"
                      disabled={isFixing}
                      onclick={() =>
                        void doctorStore.fixIssue('git', {
                          gitUserName: gitNameInput,
                          gitUserEmail: gitEmailInput
                        })}
                      class="inline-flex items-center gap-1.5 rounded-lg border border-emerald-500/40 bg-emerald-600/25 px-3 py-1.5 text-xs font-semibold text-emerald-100 hover:bg-emerald-600/40 cursor-pointer"
                    >
                      <Check class="h-3.5 w-3.5" />
                      <span>Apply Git Config</span>
                    </button>
                    <button
                      type="button"
                      onclick={() => (doctorStore.activeGuidedCheckId = null)}
                      class="rounded-lg border border-zinc-700 bg-zinc-800 px-2.5 py-1.5 text-xs text-zinc-300 hover:bg-zinc-700 cursor-pointer"
                    >
                      Cancel
                    </button>
                  </div>
                </div>
              {/if}

              <!-- Guided Inline Flow for `pi_auth` -->
              {#if check.id === 'pi_auth' && doctorStore.activeGuidedCheckId === 'pi_auth'}
                <div class="rounded-xl border border-indigo-500/35 bg-indigo-950/20 p-3.5 space-y-3">
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <div class="flex items-center gap-1.5 font-semibold text-indigo-200">
                      <KeyRound class="h-4 w-4 text-indigo-400" />
                      <span>Guided Pi Provider Setup (`~/.pi/agent/auth.json`)</span>
                    </div>
                    <button
                      type="button"
                      onclick={() => {
                        doctorStore.closeDoctorModal();
                        doctorStore.openSettingsModal('pi-auth');
                      }}
                      class="text-[11px] font-medium text-indigo-300 underline hover:text-indigo-200 cursor-pointer"
                    >
                      Open Full Login & Settings Modal →
                    </button>
                  </div>

                  <div class="grid gap-2 sm:grid-cols-3">
                    <select
                      bind:value={quickPiProvider}
                      class="rounded-lg border border-zinc-800 bg-zinc-950 px-2.5 py-1.5 text-xs text-zinc-100"
                    >
                      {#each BUILTIN_PROVIDERS_LIST as p (p.id)}
                        <option value={p.id}>{p.label}</option>
                      {/each}
                    </select>

                    <input
                      type="password"
                      bind:value={quickPiApiKey}
                      placeholder="Paste API key…"
                      class="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                    />

                    <button
                      type="button"
                      disabled={doctorStore.savingAuth || !quickPiApiKey.trim()}
                      onclick={() => void handleQuickSavePiKey()}
                      class="inline-flex items-center justify-center gap-1.5 rounded-lg border border-emerald-500/40 bg-emerald-600/25 px-3 py-1.5 text-xs font-semibold text-emerald-100 hover:bg-emerald-600/40 disabled:opacity-40 cursor-pointer"
                    >
                      <Check class="h-3.5 w-3.5" />
                      <span>Save API Key</span>
                    </button>
                  </div>

                  <div class="flex flex-wrap items-center justify-between gap-2 pt-1 border-t border-indigo-500/20">
                    <span class="text-[11px] text-indigo-200/80">
                      Or authenticate interactively with OAuth in the embedded terminal:
                    </span>
                    <button
                      type="button"
                      onclick={() => void doctorStore.launchInteractivePiLogin('pi')}
                      class="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-600/25 px-2.5 py-1 text-[11px] font-medium text-indigo-100 hover:bg-indigo-600/40 cursor-pointer"
                    >
                      <Terminal class="h-3 w-3" />
                      <span>Launch `pi` in Terminal</span>
                    </button>
                  </div>
                </div>
              {/if}

              <!-- Guided Inline Flow for `gh_auth` -->
              {#if check.id === 'gh_auth' && doctorStore.activeGuidedCheckId === 'gh_auth'}
                <div class="rounded-xl border border-sky-500/35 bg-sky-950/20 p-3.5 space-y-3">
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <div class="flex items-center gap-1.5 font-semibold text-sky-200">
                      <Globe class="h-4 w-4 text-sky-400" />
                      <span>Guided GitHub Authentication</span>
                    </div>
                    <button
                      type="button"
                      onclick={() => {
                        doctorStore.closeDoctorModal();
                        doctorStore.openSettingsModal('github');
                      }}
                      class="text-[11px] font-medium text-sky-300 underline hover:text-sky-200 cursor-pointer"
                    >
                      Open GitHub Settings →
                    </button>
                  </div>

                  {#if githubStore.authCode}
                    <div class="flex flex-wrap items-center justify-between gap-2 rounded-lg border border-sky-500/40 bg-zinc-950 p-2.5">
                      <div>
                        <div class="text-[10px] text-sky-300">One-time device code:</div>
                        <div class="font-mono text-sm font-bold tracking-widest text-sky-200">
                          {githubStore.authCode.code}
                        </div>
                      </div>
                      <button
                        type="button"
                        onclick={() =>
                          githubStore.authCode &&
                          void copyAndOpenDevice(
                            githubStore.authCode.code,
                            githubStore.authCode.verificationUri
                          )}
                        class="inline-flex items-center gap-1.5 rounded-lg bg-sky-500 px-3 py-1.5 text-xs font-semibold text-zinc-950 hover:bg-sky-400 cursor-pointer"
                      >
                        {#if copiedCode}
                          <Check class="h-3.5 w-3.5" />
                          <span>Copied & Opened!</span>
                        {:else}
                          <Copy class="h-3.5 w-3.5" />
                          <span>Copy & Open GitHub</span>
                        {/if}
                      </button>
                    </div>
                  {/if}

                  <div class="grid gap-2 sm:grid-cols-3">
                    <button
                      type="button"
                      disabled={githubStore.authBusy}
                      onclick={() => void handleQuickGhWebLogin()}
                      class="inline-flex items-center justify-center gap-1.5 rounded-lg border border-sky-500/40 bg-sky-600/25 px-3 py-1.5 text-xs font-semibold text-sky-100 hover:bg-sky-600/40 disabled:opacity-40 cursor-pointer"
                    >
                      {#if githubStore.authBusy}
                        <Loader2 class="h-3.5 w-3.5 animate-spin" />
                        <span>Waiting…</span>
                      {:else}
                        <Globe class="h-3.5 w-3.5" />
                        <span>Login with Browser</span>
                      {/if}
                    </button>

                    <input
                      type="password"
                      bind:value={quickGhToken}
                      placeholder="Or paste PAT (ghp_…)"
                      class="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                    />

                    <button
                      type="button"
                      disabled={githubStore.authBusy || !quickGhToken.trim()}
                      onclick={() => void handleQuickGhTokenLogin()}
                      class="inline-flex items-center justify-center gap-1.5 rounded-lg border border-amber-500/40 bg-amber-600/20 px-3 py-1.5 text-xs font-semibold text-amber-100 hover:bg-amber-600/35 disabled:opacity-40 cursor-pointer"
                    >
                      <KeyRound class="h-3.5 w-3.5" />
                      <span>Login with Token</span>
                    </button>
                  </div>
                </div>
              {/if}

              <!-- Live Output Streaming Logs -->
              {#if logs.length > 0}
                <div class="max-h-36 overflow-y-auto rounded-lg border border-zinc-800 bg-zinc-950 p-2.5 font-mono text-[11px] space-y-0.5">
                  {#each logs as log, i (`${log.timestamp}-${i}`)}
                    <div class={log.stream === 'stderr' ? 'text-amber-300' : 'text-zinc-300'}>
                      {log.line}
                    </div>
                  {/each}
                </div>
              {/if}

              <!-- Auto-Fix Feedback -->
              {#if feedback}
                <div
                  class="rounded-lg border px-2.5 py-1.5 text-[11px] {feedback.success
                    ? 'border-emerald-500/30 bg-emerald-950/30 text-emerald-200'
                    : 'border-rose-500/30 bg-rose-950/30 text-rose-200'}"
                >
                  {feedback.message}
                </div>
              {/if}
            </div>
          {/each}
        {/if}
      </div>
    </div>
  </div>
{/if}
