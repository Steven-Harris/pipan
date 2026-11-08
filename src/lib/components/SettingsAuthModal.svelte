<script lang="ts">
  import { openUrl } from '@tauri-apps/plugin-opener';
  import {
    X,
    KeyRound,
    Sliders,
    Server,
    GitPullRequest,
    CheckCircle2,
    AlertCircle,
    Terminal,
    Trash2,
    Plus,
    Save,
    Globe,
    Copy,
    Check,
    LogOut,
    RefreshCw,
    Loader2,
    ShieldCheck,
    Eye,
    EyeOff
  } from 'lucide-svelte';
  import { doctorStore, type SettingsModalTab } from '#lib/stores/doctor.svelte.ts';
  import { githubStore } from '#lib/stores/github.svelte.ts';
  import { chatStore } from '#lib/stores/chat.svelte.ts';
  import type {
    CustomModelEndpoint,
    PiQueueDeliveryMode,
    PiThinkingLevelOption
  } from '#lib/types/auth.ts';

  const TABS: Array<{ id: SettingsModalTab; label: string }> = [
    { id: 'pi-auth', label: 'Pi Authentication' },
    { id: 'pi-settings', label: 'Pi Settings' },
    { id: 'custom-models', label: 'Custom Models' },
    { id: 'github', label: 'GitHub Auth & Webhooks' }
  ];

  const THINKING_LEVELS: Array<{ id: PiThinkingLevelOption; label: string }> = [
    { id: 'off', label: 'Off' },
    { id: 'minimal', label: 'Minimal' },
    { id: 'low', label: 'Low' },
    { id: 'medium', label: 'Medium (Default)' },
    { id: 'high', label: 'High' },
    { id: 'xhigh', label: 'Extra High (xhigh)' },
    { id: 'max', label: 'Max' }
  ];

  const QUEUE_MODES: Array<{ id: PiQueueDeliveryMode; label: string }> = [
    { id: 'one-at-a-time', label: 'One at a time' },
    { id: 'all', label: 'All queued messages at once' }
  ];

  const WEBHOOK_EVENT_OPTIONS: readonly string[] = [
    'issue_comment',
    'pull_request',
    'pull_request_review',
    'pull_request_review_comment',
    'check_run',
    'check_suite',
    'workflow_run',
    'status'
  ];

  // Provider API key draft inputs keyed by provider ID
  let keyDrafts = $state<Record<string, string>>({});
  let revealedKeys = $state<Record<string, boolean>>({});
  let customProviderId = $state('');
  let customProviderKey = $state('');

  // Pi Settings draft state
  let draftDefaultProvider = $state('');
  let draftDefaultModel = $state('');
  let draftThinkingLevel = $state<string>('medium');
  let draftAutoCompaction = $state(true);
  let draftAutoRetry = $state(true);
  let draftSteeringMode = $state<string>('one-at-a-time');
  let draftFollowUpMode = $state<string>('one-at-a-time');
  let draftGithubDefaultRepo = $state('');
  let draftWebhookEvents = $state<string[]>([...WEBHOOK_EVENT_OPTIONS]);

  // Custom model endpoint form state
  let modelProvider = $state('ollama');
  let modelId = $state('');
  let modelName = $state('');
  let modelBaseUrl = $state('http://localhost:11434/v1');
  let modelApi = $state('openai-completions');
  let modelApiKeyEnv = $state('');
  let modelContextWindow = $state('128000');
  let modelMaxTokens = $state('16384');
  let modelReasoning = $state(false);
  let modelSupportsImages = $state(false);

  // GitHub auth form state
  let ghHostnameInput = $state('github.com');
  let ghTokenInput = $state('');
  let copiedDeviceCode = $state(false);

  // Sync settings draft whenever piOverview loads
  $effect(() => {
    const s = doctorStore.piOverview?.settings;
    if (!s) return;
    draftDefaultProvider = s.defaultProvider ?? '';
    draftDefaultModel = s.defaultModel ?? '';
    draftThinkingLevel = s.defaultThinkingLevel || 'medium';
    draftAutoCompaction = s.autoCompaction;
    draftAutoRetry = s.autoRetry;
    draftSteeringMode = s.steeringMode || 'one-at-a-time';
    draftFollowUpMode = s.followUpMode || 'one-at-a-time';
    draftGithubDefaultRepo = s.githubDefaultRepo ?? '';
    draftWebhookEvents =
      s.githubWebhookEvents && s.githubWebhookEvents.length > 0
        ? [...s.githubWebhookEvents]
        : [...WEBHOOK_EVENT_OPTIONS];
  });

  async function handleSaveProviderKey(providerId: string) {
    const val = (keyDrafts[providerId] ?? '').trim();
    if (!val) return;
    const ok = await doctorStore.saveProviderKey(providerId, val);
    if (ok) {
      keyDrafts = { ...keyDrafts, [providerId]: '' };
    }
  }

  async function handleAddCustomProviderKey() {
    const prov = customProviderId.trim().toLowerCase();
    const key = customProviderKey.trim();
    if (!prov || !key) return;
    const ok = await doctorStore.saveProviderKey(prov, key);
    if (ok) {
      customProviderId = '';
      customProviderKey = '';
    }
  }

  async function handleSavePiSettings() {
    await doctorStore.savePiSettings({
      defaultProvider: draftDefaultProvider.trim() || null,
      defaultModel: draftDefaultModel.trim() || null,
      defaultThinkingLevel: draftThinkingLevel,
      autoCompaction: draftAutoCompaction,
      autoRetry: draftAutoRetry,
      steeringMode: draftSteeringMode,
      followUpMode: draftFollowUpMode,
      githubDefaultRepo: draftGithubDefaultRepo.trim() || null,
      githubWebhookEvents:
        draftWebhookEvents.length > 0 ? draftWebhookEvents : [...WEBHOOK_EVENT_OPTIONS]
    });
  }

  function populateCustomModelForm(m: CustomModelEndpoint) {
    modelProvider = m.provider;
    modelId = m.id;
    modelName = m.name;
    modelBaseUrl = m.baseUrl;
    modelApi = m.api;
    modelApiKeyEnv = m.apiKeyEnv ?? '';
    modelContextWindow = m.contextWindow ? String(m.contextWindow) : '';
    modelMaxTokens = m.maxTokens ? String(m.maxTokens) : '';
    modelReasoning = m.reasoning;
    modelSupportsImages = m.supportsImages;
  }

  async function handleSaveCustomModel() {
    if (!modelProvider.trim() || !modelId.trim() || !modelBaseUrl.trim()) return;
    const cw = Number.parseInt(modelContextWindow, 10);
    const mt = Number.parseInt(modelMaxTokens, 10);
    const ok = await doctorStore.saveCustomModel({
      provider: modelProvider.trim(),
      id: modelId.trim(),
      name: modelName.trim() || modelId.trim(),
      baseUrl: modelBaseUrl.trim(),
      api: modelApi.trim() || 'openai-completions',
      apiKeyEnv: modelApiKeyEnv.trim() || null,
      contextWindow: Number.isFinite(cw) && cw > 0 ? cw : null,
      maxTokens: Number.isFinite(mt) && mt > 0 ? mt : null,
      reasoning: modelReasoning,
      supportsImages: modelSupportsImages
    });
    if (ok) {
      modelId = '';
      modelName = '';
    }
  }

  function toggleWebhookEvent(ev: string) {
    if (draftWebhookEvents.includes(ev)) {
      draftWebhookEvents = draftWebhookEvents.filter((item) => item !== ev);
    } else {
      draftWebhookEvents = [...draftWebhookEvents, ev];
    }
  }

  async function handleGhBrowserLogin() {
    const ok = await githubStore.loginWithWeb(ghHostnameInput.trim() || 'github.com');
    await doctorStore.loadGithubAuthDetails();
    if (ok) {
      void doctorStore.runDoctor();
    }
  }

  async function handleGhTokenLogin() {
    if (!ghTokenInput.trim()) return;
    const ok = await githubStore.loginWithToken(
      ghTokenInput.trim(),
      ghHostnameInput.trim() || 'github.com'
    );
    if (ok) {
      ghTokenInput = '';
      await doctorStore.loadGithubAuthDetails();
      void doctorStore.runDoctor();
    }
  }

  async function handleGhLogout() {
    await githubStore.logout(ghHostnameInput.trim() || 'github.com');
    await doctorStore.loadGithubAuthDetails();
    void doctorStore.runDoctor();
  }

  async function copyAndOpenGithubDevice(code: string, uri: string) {
    try {
      await navigator.clipboard.writeText(code);
      copiedDeviceCode = true;
      setTimeout(() => {
        copiedDeviceCode = false;
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

{#if doctorStore.settingsModalOpen}
  <div
    class="fixed inset-0 z-50 flex items-center justify-center bg-black/75 p-4 backdrop-blur-sm"
    role="dialog"
    aria-modal="true"
    aria-label="Login and Configuration"
  >
    <div class="flex max-h-[90vh] w-full max-w-4xl flex-col overflow-hidden rounded-2xl border border-zinc-800 bg-zinc-950 text-zinc-100 shadow-2xl">
      <!-- Modal Header -->
      <div class="flex items-center justify-between border-b border-zinc-800/80 bg-zinc-900/70 px-5 py-3.5">
        <div class="flex items-center gap-2.5">
          <div class="flex h-8 w-8 items-center justify-center rounded-lg border border-indigo-500/30 bg-indigo-500/15 text-indigo-300">
            <KeyRound class="h-4 w-4" />
          </div>
          <div>
            <h2 class="text-sm font-semibold text-zinc-100">Login & Configuration</h2>
            <p class="text-[11px] text-zinc-400">
              Manage Pi provider credentials, startup settings, custom model endpoints, and GitHub authentication
            </p>
          </div>
        </div>

        <button
          type="button"
          onclick={() => doctorStore.closeSettingsModal()}
          class="rounded-lg p-1.5 text-zinc-400 transition hover:bg-zinc-800 hover:text-zinc-100 cursor-pointer"
          title="Close settings"
          aria-label="Close settings"
        >
          <X class="h-4 w-4" />
        </button>
      </div>

      <!-- Tab Switcher -->
      <div class="flex flex-wrap items-center gap-1.5 border-b border-zinc-800/80 bg-zinc-900/40 px-5 py-2">
        {#each TABS as tab (tab.id)}
          {@const active = doctorStore.settingsActiveTab === tab.id}
          <button
            type="button"
            onclick={() => {
              doctorStore.settingsActiveTab = tab.id;
              doctorStore.authFeedback = null;
            }}
            class="inline-flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-xs font-medium transition cursor-pointer {active
              ? 'bg-indigo-600/20 text-indigo-200 border border-indigo-500/40 shadow-sm'
              : 'text-zinc-400 hover:bg-zinc-800/60 hover:text-zinc-200 border border-transparent'}"
          >
            {#if tab.id === 'pi-auth'}
              <KeyRound class="h-3.5 w-3.5 text-indigo-400" />
            {:else if tab.id === 'pi-settings'}
              <Sliders class="h-3.5 w-3.5 text-sky-400" />
            {:else if tab.id === 'custom-models'}
              <Server class="h-3.5 w-3.5 text-emerald-400" />
            {:else}
              <GitPullRequest class="h-3.5 w-3.5 text-amber-400" />
            {/if}
            <span>{tab.label}</span>
          </button>
        {/each}
      </div>

      <!-- Feedback Banner -->
      {#if doctorStore.authFeedback}
        <div
          class="flex items-center justify-between gap-2 border-b px-5 py-2 text-xs {doctorStore.authFeedback.type === 'success'
            ? 'border-emerald-500/30 bg-emerald-950/40 text-emerald-200'
            : 'border-rose-500/30 bg-rose-950/40 text-rose-200'}"
        >
          <div class="flex items-center gap-2">
            {#if doctorStore.authFeedback.type === 'success'}
              <CheckCircle2 class="h-3.5 w-3.5 text-emerald-400 shrink-0" />
            {:else}
              <AlertCircle class="h-3.5 w-3.5 text-rose-400 shrink-0" />
            {/if}
            <span>{doctorStore.authFeedback.message}</span>
          </div>
          <button
            type="button"
            onclick={() => (doctorStore.authFeedback = null)}
            class="text-zinc-400 hover:text-zinc-200 cursor-pointer"
          >
            <X class="h-3.5 w-3.5" />
          </button>
        </div>
      {/if}

      <!-- Tab Body -->
      <div class="flex-1 overflow-y-auto p-5 space-y-5 text-xs">
        {#if doctorStore.settingsActiveTab === 'pi-auth'}
          <!-- File path info & OAuth helper -->
          <div class="grid gap-3 md:grid-cols-2">
            <div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-3.5 space-y-1.5">
              <div class="flex items-center justify-between">
                <span class="font-semibold text-zinc-200">Credentials File (`auth.json`)</span>
                <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-300">
                  Masked Preview
                </span>
              </div>
              <p class="font-mono text-[11px] text-zinc-400 break-all">
                {doctorStore.piOverview?.authFilePath ?? '~/.pi/agent/auth.json'}
              </p>
              <p class="text-[11px] text-zinc-400">
                API keys are written directly to <code class="text-zinc-300">auth.json</code> in Rust while preserving existing OAuth tokens.
              </p>
            </div>

            <div class="rounded-xl border border-indigo-500/30 bg-indigo-950/20 p-3.5 space-y-2">
              <div class="flex items-center justify-between">
                <span class="font-semibold text-indigo-200">Interactive Provider OAuth Login</span>
                <Terminal class="h-4 w-4 text-indigo-400" />
              </div>
              <p class="text-[11px] text-indigo-200/80 leading-relaxed">
                Need browser-based OAuth login for providers like Anthropic Claude Max or Google? Launch <code class="rounded bg-indigo-950/70 px-1 font-mono">pi</code> in the embedded terminal.
              </p>
              <button
                type="button"
                onclick={() => void doctorStore.launchInteractivePiLogin('pi')}
                class="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-600/25 px-3 py-1.5 text-xs font-medium text-indigo-100 transition hover:bg-indigo-600/40 cursor-pointer"
              >
                <Terminal class="h-3.5 w-3.5" />
                <span>Launch Interactive OAuth in Terminal</span>
              </button>
            </div>
          </div>

          <!-- Provider API Keys List -->
          <div class="space-y-2.5">
            <h3 class="text-xs font-semibold uppercase tracking-wider text-zinc-400">
              AI Provider API Keys & Credentials
            </h3>

            <div class="space-y-2">
              {#each doctorStore.piOverview?.providers ?? [] as prov (prov.id)}
                <div class="rounded-xl border border-zinc-800/90 bg-zinc-900/50 p-3 space-y-2">
                  <div class="flex flex-wrap items-center justify-between gap-2">
                    <div class="flex items-center gap-2">
                      <span class="font-semibold text-zinc-100">{prov.label}</span>
                      <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-400">
                        {prov.id}
                      </span>
                      {#if prov.configured}
                        <span class="inline-flex items-center gap-1 rounded-full border border-emerald-500/30 bg-emerald-500/10 px-2 py-0.5 text-[10px] font-medium text-emerald-300">
                          <CheckCircle2 class="h-3 w-3" />
                          {prov.authType === 'oauth'
                            ? 'OAuth'
                            : prov.authType === 'env'
                              ? 'Env Var'
                              : 'API Key'}
                        </span>
                      {:else}
                        <span class="rounded-full border border-zinc-700 bg-zinc-800/70 px-2 py-0.5 text-[10px] text-zinc-400">
                          Not configured
                        </span>
                      {/if}
                    </div>

                    {#if prov.maskedPreview}
                      <span class="font-mono text-[11px] text-zinc-300">
                        {prov.maskedPreview}
                      </span>
                    {:else if prov.envVar}
                      <span class="font-mono text-[10px] text-zinc-500">
                        Env: ${prov.envVar}
                      </span>
                    {/if}
                  </div>

                  <div class="flex items-center gap-2">
                    <div class="relative flex-1">
                      <input
                        type={revealedKeys[prov.id] ? 'text' : 'password'}
                        value={keyDrafts[prov.id] ?? ''}
                        oninput={(e) => {
                          keyDrafts = { ...keyDrafts, [prov.id]: e.currentTarget.value };
                        }}
                        onkeydown={(e) => {
                          if (e.key === 'Enter') {
                            e.preventDefault();
                            void handleSaveProviderKey(prov.id);
                          }
                        }}
                        placeholder={prov.configured
                          ? 'Enter new API key to replace stored credential…'
                          : `Enter ${prov.label} API key…`}
                        aria-label="{prov.label} API key"
                        class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 pr-8 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
                      />
                      <button
                        type="button"
                        onclick={() => {
                          revealedKeys = { ...revealedKeys, [prov.id]: !revealedKeys[prov.id] };
                        }}
                        class="absolute right-2 top-1/2 -translate-y-1/2 text-zinc-500 hover:text-zinc-300 cursor-pointer"
                        title={revealedKeys[prov.id] ? 'Hide key' : 'Show key'}
                      >
                        {#if revealedKeys[prov.id]}
                          <EyeOff class="h-3.5 w-3.5" />
                        {:else}
                          <Eye class="h-3.5 w-3.5" />
                        {/if}
                      </button>
                    </div>

                    <button
                      type="button"
                      disabled={doctorStore.savingAuth || !(keyDrafts[prov.id] ?? '').trim()}
                      onclick={() => void handleSaveProviderKey(prov.id)}
                      class="inline-flex items-center gap-1 rounded-lg border border-indigo-500/40 bg-indigo-600/20 px-3 py-1.5 text-xs font-medium text-indigo-100 transition hover:bg-indigo-600/35 disabled:opacity-40 cursor-pointer"
                    >
                      <Save class="h-3.5 w-3.5" />
                      <span>Save Key</span>
                    </button>

                    {#if prov.hasStoredCredential}
                      <button
                        type="button"
                        disabled={doctorStore.savingAuth}
                        onclick={() => void doctorStore.removeProviderKey(prov.id)}
                        class="inline-flex items-center gap-1 rounded-lg border border-rose-500/30 bg-rose-500/10 px-2.5 py-1.5 text-xs font-medium text-rose-300 transition hover:bg-rose-500/20 disabled:opacity-40 cursor-pointer"
                        title="Remove stored credential from auth.json"
                      >
                        <Trash2 class="h-3.5 w-3.5" />
                        <span>Remove</span>
                      </button>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>
          </div>

          <!-- Custom Provider Key Adder -->
          <div class="rounded-xl border border-zinc-800 bg-zinc-900/50 p-3.5 space-y-2.5">
            <h4 class="font-semibold text-zinc-200">Add Custom Provider API Key</h4>
            <div class="grid gap-2 sm:grid-cols-3">
              <input
                type="text"
                bind:value={customProviderId}
                placeholder="Provider ID (e.g. deepseek, together)"
                aria-label="Custom provider ID"
                class="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
              />
              <input
                type="password"
                bind:value={customProviderKey}
                placeholder="API Key…"
                aria-label="Custom provider API key"
                class="rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
              />
              <button
                type="button"
                disabled={doctorStore.savingAuth || !customProviderId.trim() || !customProviderKey.trim()}
                onclick={() => void handleAddCustomProviderKey()}
                class="inline-flex items-center justify-center gap-1.5 rounded-lg border border-emerald-500/40 bg-emerald-600/20 px-3 py-1.5 text-xs font-medium text-emerald-100 transition hover:bg-emerald-600/35 disabled:opacity-40 cursor-pointer"
              >
                <Plus class="h-3.5 w-3.5" />
                <span>Add Provider Key</span>
              </button>
            </div>
          </div>
        {:else if doctorStore.settingsActiveTab === 'pi-settings'}
          <div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-3.5 space-y-1">
            <div class="font-semibold text-zinc-200">Pi Agent Settings (`settings.json`)</div>
            <p class="font-mono text-[11px] text-zinc-400 break-all">
              {doctorStore.piOverview?.settingsFilePath ?? '~/.pi/agent/settings.json'}
            </p>
          </div>

          <div class="grid gap-4 sm:grid-cols-2">
            <label class="space-y-1">
              <span class="font-medium text-zinc-200">Default Provider</span>
              <input
                type="text"
                bind:value={draftDefaultProvider}
                placeholder="e.g. anthropic, openai, google"
                class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
              />
            </label>

            <label class="space-y-1">
              <span class="font-medium text-zinc-200">Default Model ID</span>
              <input
                type="text"
                bind:value={draftDefaultModel}
                list="pipan-known-models"
                placeholder="e.g. claude-sonnet-4-20250514"
                class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100 placeholder-zinc-500 focus:border-indigo-500/60 focus:outline-none"
              />
              <datalist id="pipan-known-models">
                {#each chatStore.configuredModels as m (`${m.provider}/${m.id}`)}
                  <option value={m.id}>{m.provider} — {m.name}</option>
                {/each}
              </datalist>
            </label>

            <label class="space-y-1">
              <span class="font-medium text-zinc-200">Default Thinking Level</span>
              <select
                bind:value={draftThinkingLevel}
                class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 text-xs text-zinc-100 focus:border-indigo-500/60 focus:outline-none"
              >
                {#each THINKING_LEVELS as lvl (lvl.id)}
                  <option value={lvl.id}>{lvl.label}</option>
                {/each}
              </select>
            </label>

            <label class="space-y-1">
              <span class="font-medium text-zinc-200">Steering Mode (`steeringMode`)</span>
              <select
                bind:value={draftSteeringMode}
                class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 text-xs text-zinc-100 focus:border-indigo-500/60 focus:outline-none"
              >
                {#each QUEUE_MODES as mode (mode.id)}
                  <option value={mode.id}>{mode.label}</option>
                {/each}
              </select>
            </label>

            <label class="space-y-1">
              <span class="font-medium text-zinc-200">Follow-Up Mode (`followUpMode`)</span>
              <select
                bind:value={draftFollowUpMode}
                class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 text-xs text-zinc-100 focus:border-indigo-500/60 focus:outline-none"
              >
                {#each QUEUE_MODES as mode (mode.id)}
                  <option value={mode.id}>{mode.label}</option>
                {/each}
              </select>
            </label>
          </div>

          <div class="grid gap-3 sm:grid-cols-2">
            <label class="flex items-start gap-2.5 rounded-xl border border-zinc-800 bg-zinc-900/50 p-3 cursor-pointer">
              <input
                type="checkbox"
                bind:checked={draftAutoCompaction}
                class="mt-0.5 rounded border-zinc-700 bg-zinc-950 text-indigo-500 focus:ring-indigo-500"
              />
              <div>
                <div class="font-semibold text-zinc-100">Automatic Context Compaction (`autoCompaction`)</div>
                <p class="text-[11px] text-zinc-400">
                  Automatically summarizes older conversation turns when approaching context window limits.
                </p>
              </div>
            </label>

            <label class="flex items-start gap-2.5 rounded-xl border border-zinc-800 bg-zinc-900/50 p-3 cursor-pointer">
              <input
                type="checkbox"
                bind:checked={draftAutoRetry}
                class="mt-0.5 rounded border-zinc-700 bg-zinc-950 text-indigo-500 focus:ring-indigo-500"
              />
              <div>
                <div class="font-semibold text-zinc-100">Automatic Provider Retry (`autoRetry`)</div>
                <p class="text-[11px] text-zinc-400">
                  Automatically retries transient rate-limit and network errors with exponential backoff.
                </p>
              </div>
            </label>
          </div>

          <div class="flex justify-end">
            <button
              type="button"
              disabled={doctorStore.savingAuth}
              onclick={() => void handleSavePiSettings()}
              class="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-600/25 px-4 py-2 text-xs font-semibold text-indigo-100 transition hover:bg-indigo-600/40 disabled:opacity-40 cursor-pointer"
            >
              <Save class="h-3.5 w-3.5" />
              <span>Save Pi Settings</span>
            </button>
          </div>
        {:else if doctorStore.settingsActiveTab === 'custom-models'}
          <div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-3.5 space-y-1">
            <div class="font-semibold text-zinc-200">Custom Model Endpoints (`models.json`)</div>
            <p class="font-mono text-[11px] text-zinc-400 break-all">
              {doctorStore.piOverview?.modelsFilePath ?? '~/.pi/agent/models.json'}
            </p>
          </div>

          <!-- Existing custom models -->
          <div class="space-y-2">
            <h3 class="text-xs font-semibold uppercase tracking-wider text-zinc-400">
              Configured Custom Endpoints ({doctorStore.piOverview?.customModels.length ?? 0})
            </h3>

            {#if !doctorStore.piOverview?.customModels.length}
              <div class="rounded-xl border border-zinc-800/80 bg-zinc-900/40 p-4 text-center text-zinc-400">
                No custom model endpoints configured yet. Add an OpenAI-compatible or local endpoint (Ollama, vLLM, LM Studio) below.
              </div>
            {:else}
              <div class="space-y-2">
                {#each doctorStore.piOverview.customModels as cm (`${cm.provider}/${cm.id}`)}
                  <div class="flex flex-wrap items-center justify-between gap-2 rounded-xl border border-zinc-800 bg-zinc-900/60 p-3">
                    <div class="space-y-0.5">
                      <div class="flex items-center gap-2">
                        <span class="font-semibold text-zinc-100">{cm.name}</span>
                        <span class="rounded bg-indigo-500/15 px-1.5 py-0.5 font-mono text-[10px] text-indigo-300">
                          {cm.provider}/{cm.id}
                        </span>
                        <span class="rounded bg-zinc-800 px-1.5 py-0.5 font-mono text-[10px] text-zinc-400">
                          {cm.api}
                        </span>
                      </div>
                      <div class="font-mono text-[11px] text-zinc-400">
                        {cm.baseUrl}
                        {#if cm.contextWindow}
                          · ctx {cm.contextWindow.toLocaleString()}
                        {/if}
                      </div>
                    </div>

                    <div class="flex items-center gap-1.5">
                      <button
                        type="button"
                        onclick={() => populateCustomModelForm(cm)}
                        class="rounded-lg border border-zinc-700 bg-zinc-800 px-2.5 py-1 text-[11px] text-zinc-200 hover:bg-zinc-700 cursor-pointer"
                      >
                        Edit
                      </button>
                      <button
                        type="button"
                        onclick={() => void doctorStore.deleteCustomModel(cm.provider, cm.id)}
                        class="inline-flex items-center gap-1 rounded-lg border border-rose-500/30 bg-rose-500/10 px-2.5 py-1 text-[11px] text-rose-300 hover:bg-rose-500/20 cursor-pointer"
                      >
                        <Trash2 class="h-3 w-3" />
                        <span>Remove</span>
                      </button>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          </div>

          <!-- Add / Update Custom Model Form -->
          <div class="rounded-xl border border-zinc-800 bg-zinc-900/50 p-4 space-y-3">
            <h4 class="font-semibold text-zinc-200">Add or Update Custom Model Endpoint</h4>

            <div class="grid gap-3 sm:grid-cols-2">
              <label class="space-y-1">
                <span class="text-zinc-300">Provider ID</span>
                <input
                  type="text"
                  bind:value={modelProvider}
                  placeholder="ollama"
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                />
              </label>

              <label class="space-y-1">
                <span class="text-zinc-300">Model ID</span>
                <input
                  type="text"
                  bind:value={modelId}
                  placeholder="qwen2.5-coder:32b"
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                />
              </label>

              <label class="space-y-1">
                <span class="text-zinc-300">Display Name</span>
                <input
                  type="text"
                  bind:value={modelName}
                  placeholder="Qwen 2.5 Coder 32B (Local)"
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 text-xs text-zinc-100"
                />
              </label>

              <label class="space-y-1">
                <span class="text-zinc-300">Base URL</span>
                <input
                  type="url"
                  bind:value={modelBaseUrl}
                  placeholder="http://localhost:11434/v1"
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                />
              </label>

              <label class="space-y-1">
                <span class="text-zinc-300">API Protocol</span>
                <select
                  bind:value={modelApi}
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 text-xs text-zinc-100"
                >
                  <option value="openai-completions">openai-completions</option>
                  <option value="openai-responses">openai-responses</option>
                  <option value="anthropic-messages">anthropic-messages</option>
                </select>
              </label>

              <label class="space-y-1">
                <span class="text-zinc-300">API Key / Env Var (Optional)</span>
                <input
                  type="text"
                  bind:value={modelApiKeyEnv}
                  placeholder="OLLAMA_API_KEY"
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                />
              </label>

              <label class="space-y-1">
                <span class="text-zinc-300">Context Window Tokens</span>
                <input
                  type="number"
                  bind:value={modelContextWindow}
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                />
              </label>

              <label class="space-y-1">
                <span class="text-zinc-300">Max Output Tokens</span>
                <input
                  type="number"
                  bind:value={modelMaxTokens}
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                />
              </label>
            </div>

            <div class="flex flex-wrap items-center justify-between gap-3 pt-1">
              <div class="flex items-center gap-4">
                <label class="inline-flex items-center gap-2 text-zinc-300 cursor-pointer">
                  <input type="checkbox" bind:checked={modelReasoning} class="rounded border-zinc-700 bg-zinc-950" />
                  <span>Supports Reasoning / Thinking</span>
                </label>

                <label class="inline-flex items-center gap-2 text-zinc-300 cursor-pointer">
                  <input type="checkbox" bind:checked={modelSupportsImages} class="rounded border-zinc-700 bg-zinc-950" />
                  <span>Supports Vision / Images</span>
                </label>
              </div>

              <button
                type="button"
                disabled={doctorStore.savingAuth || !modelProvider.trim() || !modelId.trim() || !modelBaseUrl.trim()}
                onclick={() => void handleSaveCustomModel()}
                class="inline-flex items-center gap-1.5 rounded-lg border border-emerald-500/40 bg-emerald-600/20 px-3.5 py-1.5 text-xs font-medium text-emerald-100 transition hover:bg-emerald-600/35 disabled:opacity-40 cursor-pointer"
              >
                <Plus class="h-3.5 w-3.5" />
                <span>Save Custom Endpoint</span>
              </button>
            </div>
          </div>
        {:else if doctorStore.settingsActiveTab === 'github'}
          <!-- GitHub Auth Status Card -->
          <div class="rounded-xl border border-zinc-800 bg-zinc-900/60 p-4 space-y-3">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <div class="flex items-center gap-2">
                <ShieldCheck
                  class="h-4 w-4 {doctorStore.ghDetails?.authenticated || githubStore.status?.authenticated
                    ? 'text-emerald-400'
                    : 'text-amber-400'}"
                />
                <span class="font-semibold text-zinc-100">GitHub CLI (`gh auth status`)</span>
                {#if doctorStore.ghDetails?.authenticated || githubStore.status?.authenticated}
                  <span class="rounded-full border border-emerald-500/30 bg-emerald-500/15 px-2 py-0.5 text-[10px] font-medium text-emerald-300">
                    Authenticated
                  </span>
                {:else}
                  <span class="rounded-full border border-amber-500/30 bg-amber-500/15 px-2 py-0.5 text-[10px] font-medium text-amber-300">
                    Signed Out
                  </span>
                {/if}
              </div>

              <div class="flex items-center gap-2">
                <button
                  type="button"
                  onclick={() => void doctorStore.loadGithubAuthDetails()}
                  class="inline-flex items-center gap-1 rounded-lg border border-zinc-700 bg-zinc-800 px-2.5 py-1 text-[11px] text-zinc-200 hover:bg-zinc-700 cursor-pointer"
                >
                  <RefreshCw class="h-3 w-3" />
                  <span>Refresh</span>
                </button>

                {#if doctorStore.ghDetails?.authenticated || githubStore.status?.authenticated}
                  <button
                    type="button"
                    disabled={githubStore.authBusy}
                    onclick={() => void handleGhLogout()}
                    class="inline-flex items-center gap-1 rounded-lg border border-rose-500/30 bg-rose-500/10 px-2.5 py-1 text-[11px] text-rose-300 hover:bg-rose-500/20 disabled:opacity-40 cursor-pointer"
                  >
                    <LogOut class="h-3 w-3" />
                    <span>Logout</span>
                  </button>
                {/if}
              </div>
            </div>

            <div class="grid gap-2 sm:grid-cols-4 text-[11px]">
              <div class="rounded-lg border border-zinc-800/80 bg-zinc-950/60 p-2.5">
                <div class="text-zinc-500">Account Login</div>
                <div class="mt-0.5 font-mono font-semibold text-zinc-100">
                  {doctorStore.ghDetails?.account ?? githubStore.status?.account ?? '—'}
                </div>
              </div>

              <div class="rounded-lg border border-zinc-800/80 bg-zinc-950/60 p-2.5">
                <div class="text-zinc-500">Host & Protocol</div>
                <div class="mt-0.5 font-mono font-semibold text-zinc-100">
                  {doctorStore.ghDetails?.hostname ?? 'github.com'} ({doctorStore.ghDetails?.gitProtocol ?? 'https'})
                </div>
              </div>

              <div class="rounded-lg border border-zinc-800/80 bg-zinc-950/60 p-2.5">
                <div class="text-zinc-500">Token Scopes</div>
                <div class="mt-0.5 flex flex-wrap gap-1">
                  {#if doctorStore.ghDetails?.scopes?.length}
                    {#each doctorStore.ghDetails.scopes as scope (scope)}
                      <span class="rounded bg-zinc-800 px-1.5 py-0.2 font-mono text-[10px] text-sky-300">
                        {scope}
                      </span>
                    {/each}
                  {:else}
                    <span class="font-mono text-zinc-400">default</span>
                  {/if}
                </div>
              </div>

              <div class="rounded-lg border border-zinc-800/80 bg-zinc-950/60 p-2.5">
                <div class="text-zinc-500">Webhook Extension</div>
                <div class="mt-0.5 font-mono font-semibold {doctorStore.ghDetails?.webhookExtension ? 'text-emerald-300' : 'text-amber-300'}">
                  {doctorStore.ghDetails?.webhookExtension ? 'cli/gh-webhook installed' : 'Not installed'}
                </div>
              </div>
            </div>
          </div>

          <!-- Browser & Token Login Controls -->
          <div class="grid gap-4 md:grid-cols-2">
            <!-- Browser OAuth Device Flow -->
            <div class="rounded-xl border border-zinc-800 bg-zinc-900/50 p-4 space-y-3">
              <div class="flex items-center gap-2 font-semibold text-zinc-100">
                <Globe class="h-4 w-4 text-sky-400" />
                <span>Login with Browser (`gh auth login --web`)</span>
              </div>
              <p class="text-[11px] text-zinc-400 leading-relaxed">
                Generates an 8-character one-time device code and waits for browser authorization to complete.
              </p>

              <label class="block space-y-1">
                <span class="text-[11px] text-zinc-400">GitHub Hostname</span>
                <input
                  type="text"
                  bind:value={ghHostnameInput}
                  placeholder="github.com"
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                />
              </label>

              {#if githubStore.authCode}
                <div class="rounded-xl border border-sky-500/40 bg-sky-950/30 p-3 space-y-2">
                  <div class="text-[11px] text-sky-200">
                    One-time device code for <span class="font-mono">{githubStore.authCode.hostname}</span>:
                  </div>
                  <div class="flex items-center justify-between gap-2">
                    <span class="rounded-lg bg-zinc-950 px-3 py-1.5 font-mono text-base font-bold tracking-widest text-sky-300">
                      {githubStore.authCode.code}
                    </span>
                    <button
                      type="button"
                      onclick={() =>
                        githubStore.authCode &&
                        void copyAndOpenGithubDevice(
                          githubStore.authCode.code,
                          githubStore.authCode.verificationUri
                        )}
                      class="inline-flex items-center gap-1.5 rounded-lg bg-sky-500 px-3 py-1.5 text-xs font-semibold text-zinc-950 hover:bg-sky-400 cursor-pointer"
                    >
                      {#if copiedDeviceCode}
                        <Check class="h-3.5 w-3.5" />
                        <span>Copied & Opened!</span>
                      {:else}
                        <Copy class="h-3.5 w-3.5" />
                        <span>Copy & Open GitHub</span>
                      {/if}
                    </button>
                  </div>
                </div>
              {/if}

              <button
                type="button"
                disabled={githubStore.authBusy}
                onclick={() => void handleGhBrowserLogin()}
                class="inline-flex w-full items-center justify-center gap-2 rounded-lg border border-sky-500/40 bg-sky-600/20 px-3 py-2 text-xs font-semibold text-sky-100 transition hover:bg-sky-600/35 disabled:opacity-40 cursor-pointer"
              >
                {#if githubStore.authBusy}
                  <Loader2 class="h-3.5 w-3.5 animate-spin" />
                  <span>Waiting for browser authentication…</span>
                {:else}
                  <Globe class="h-3.5 w-3.5" />
                  <span>Login with Browser</span>
                {/if}
              </button>
            </div>

            <!-- Personal Access Token Login -->
            <div class="rounded-xl border border-zinc-800 bg-zinc-900/50 p-4 space-y-3">
              <div class="flex items-center gap-2 font-semibold text-zinc-100">
                <KeyRound class="h-4 w-4 text-amber-400" />
                <span>Login with Token (`gh auth login --with-token`)</span>
              </div>
              <p class="text-[11px] text-zinc-400 leading-relaxed">
                Authenticate directly with a GitHub Personal Access Token (`repo`, `read:org`, `workflow` scopes recommended).
              </p>

              <label class="block space-y-1">
                <span class="text-[11px] text-zinc-400">Personal Access Token</span>
                <input
                  type="password"
                  bind:value={ghTokenInput}
                  placeholder="ghp_••••••••••••••••••••••••••••••••••••"
                  class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
                />
              </label>

              {#if githubStore.authError}
                <p class="text-[11px] text-rose-300">{githubStore.authError}</p>
              {/if}

              <button
                type="button"
                disabled={githubStore.authBusy || !ghTokenInput.trim()}
                onclick={() => void handleGhTokenLogin()}
                class="inline-flex w-full items-center justify-center gap-2 rounded-lg border border-amber-500/40 bg-amber-600/20 px-3 py-2 text-xs font-semibold text-amber-100 transition hover:bg-amber-600/35 disabled:opacity-40 cursor-pointer"
              >
                <KeyRound class="h-3.5 w-3.5" />
                <span>Login with Token</span>
              </button>
            </div>
          </div>

          <!-- Default Repository & Webhook Event Preferences -->
          <div class="rounded-xl border border-zinc-800 bg-zinc-900/50 p-4 space-y-3">
            <h4 class="font-semibold text-zinc-200">Default Repository & Webhook Event Preferences</h4>

            <label class="block space-y-1">
              <span class="text-zinc-300">Default Repository (`owner/repo`)</span>
              <input
                type="text"
                bind:value={draftGithubDefaultRepo}
                placeholder="e.g. cli/cli (leave blank to auto-detect from workspace git remote)"
                class="w-full rounded-lg border border-zinc-800 bg-zinc-950 px-3 py-1.5 font-mono text-xs text-zinc-100"
              />
            </label>

            <div class="space-y-1.5">
              <span class="text-zinc-300">Forwarded Webhook Events (`gh webhook forward`)</span>
              <div class="grid grid-cols-2 gap-2 sm:grid-cols-4">
                {#each WEBHOOK_EVENT_OPTIONS as ev (ev)}
                  <label class="flex items-center gap-2 rounded-lg border border-zinc-800 bg-zinc-950/70 px-2.5 py-1.5 font-mono text-[11px] text-zinc-200 cursor-pointer">
                    <input
                      type="checkbox"
                      checked={draftWebhookEvents.includes(ev)}
                      onchange={() => toggleWebhookEvent(ev)}
                      class="rounded border-zinc-700 bg-zinc-900 text-indigo-500"
                    />
                    <span class="truncate">{ev}</span>
                  </label>
                {/each}
              </div>
            </div>

            <div class="flex justify-end pt-1">
              <button
                type="button"
                disabled={doctorStore.savingAuth}
                onclick={() => void handleSavePiSettings()}
                class="inline-flex items-center gap-1.5 rounded-lg border border-indigo-500/40 bg-indigo-600/25 px-3.5 py-1.5 text-xs font-semibold text-indigo-100 hover:bg-indigo-600/40 disabled:opacity-40 cursor-pointer"
              >
                <Save class="h-3.5 w-3.5" />
                <span>Save GitHub Preferences</span>
              </button>
            </div>
          </div>
        {/if}
      </div>
    </div>
  </div>
{/if}
