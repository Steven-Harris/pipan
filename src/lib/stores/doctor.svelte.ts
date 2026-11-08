import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { chatStore } from '#lib/stores/chat.svelte.ts';
import { githubStore } from '#lib/stores/github.svelte.ts';
import { terminalStore } from '#lib/stores/terminal.svelte.ts';
import { workspaceStore } from '#lib/stores/workspace.svelte.ts';
import type {
  CustomModelEndpoint,
  GithubAuthDetails,
  PiAuthAndSettingsOverview,
  PiSettingsConfig
} from '#lib/types/auth.ts';
import type {
  DoctorFixLogEvent,
  DoctorFixResult,
  DoctorReport
} from '#lib/types/doctor.ts';

export type SettingsModalTab = 'pi-auth' | 'pi-settings' | 'custom-models' | 'github';

class DoctorStore {
  doctorModalOpen = $state<boolean>(false);
  settingsModalOpen = $state<boolean>(false);
  settingsActiveTab = $state<SettingsModalTab>('pi-auth');

  report = $state<DoctorReport | null>(null);
  runningDoctor = $state<boolean>(false);
  doctorError = $state<string | null>(null);

  fixingCheckId = $state<string | null>(null);
  fixLogs = $state<Record<string, DoctorFixLogEvent[]>>({});
  fixFeedback = $state<Record<string, { success: boolean; message: string }>>({});
  activeGuidedCheckId = $state<string | null>(null);

  piOverview = $state<PiAuthAndSettingsOverview | null>(null);
  ghDetails = $state<GithubAuthDetails | null>(null);
  loadingAuth = $state<boolean>(false);
  savingAuth = $state<boolean>(false);
  authFeedback = $state<{ type: 'success' | 'error'; message: string } | null>(null);

  private initialized = false;
  private unlisteners: UnlistenFn[] = [];

  async init(): Promise<void> {
    if (!this.initialized) {
      this.initialized = true;
      try {
        const unlistenLog = await listen<DoctorFixLogEvent>('doctor:fix-log', (event) => {
          const item = event.payload;
          if (!item?.checkId) return;
          const prev = this.fixLogs[item.checkId] ?? [];
          this.fixLogs = {
            ...this.fixLogs,
            [item.checkId]: [...prev.slice(-199), item]
          };
        });
        this.unlisteners.push(unlistenLog);
      } catch {
        // Running outside Tauri runtime
      }
    }

    await Promise.all([this.runDoctor(), this.loadAuthAndSettings()]);
  }

  destroy(): void {
    for (const unlisten of this.unlisteners) {
      unlisten();
    }
    this.unlisteners = [];
    this.initialized = false;
  }

  openDoctorModal(): void {
    this.doctorModalOpen = true;
    void this.runDoctor();
    void this.loadAuthAndSettings();
    void this.loadGithubAuthDetails();
  }

  closeDoctorModal(): void {
    this.doctorModalOpen = false;
    this.activeGuidedCheckId = null;
  }

  openSettingsModal(tab: SettingsModalTab = 'pi-auth'): void {
    this.settingsActiveTab = tab;
    this.settingsModalOpen = true;
    this.authFeedback = null;
    void this.loadAuthAndSettings();
    void this.loadGithubAuthDetails();
  }

  closeSettingsModal(): void {
    this.settingsModalOpen = false;
    this.authFeedback = null;
  }

  async runDoctor(): Promise<DoctorReport | null> {
    this.runningDoctor = true;
    this.doctorError = null;
    try {
      const res = await invoke<DoctorReport>('run_doctor');
      this.report = res;
      return res;
    } catch (err) {
      this.doctorError = err instanceof Error ? err.message : String(err);
      return null;
    } finally {
      this.runningDoctor = false;
    }
  }

  async fixIssue(
    checkId: string,
    opts?: { gitUserName?: string; gitUserEmail?: string }
  ): Promise<DoctorFixResult | null> {
    if (checkId === 'pi_auth' || checkId === 'gh_auth') {
      this.activeGuidedCheckId = checkId;
      void this.loadAuthAndSettings();
      void this.loadGithubAuthDetails();
      return {
        checkId,
        success: true,
        message:
          checkId === 'pi_auth'
            ? 'Guided Pi login & API key setup opened below.'
            : 'Guided GitHub login flow opened below.',
        outputLines: [],
        openFlow: checkId
      };
    }

    const check = this.report?.checks.find((c) => c.id === checkId);
    if (
      checkId === 'git' &&
      check?.fixAction === 'git_config' &&
      !opts?.gitUserName &&
      !opts?.gitUserEmail &&
      this.activeGuidedCheckId !== 'git'
    ) {
      this.activeGuidedCheckId = 'git';
      return null;
    }

    this.fixingCheckId = checkId;
    this.fixLogs = { ...this.fixLogs, [checkId]: [] };
    const nextFeedback = { ...this.fixFeedback };
    delete nextFeedback[checkId];
    this.fixFeedback = nextFeedback;

    try {
      const res = await invoke<DoctorFixResult>('fix_doctor_issue', {
        checkId,
        gitUserName: opts?.gitUserName?.trim() || null,
        gitUserEmail: opts?.gitUserEmail?.trim() || null
      });

      if (res.updatedReport) {
        this.report = res.updatedReport;
      } else {
        await this.runDoctor();
      }

      this.fixFeedback = {
        ...this.fixFeedback,
        [checkId]: {
          success: res.success,
          message: res.message
        }
      };

      if (res.openFlow === 'pi_auth' || res.openFlow === 'gh_auth') {
        this.activeGuidedCheckId = res.openFlow;
      } else if (res.success && this.activeGuidedCheckId === checkId) {
        this.activeGuidedCheckId = null;
      }

      return res;
    } catch (err) {
      const msg = err instanceof Error ? err.message : String(err);
      this.fixFeedback = {
        ...this.fixFeedback,
        [checkId]: {
          success: false,
          message: msg
        }
      };
      return null;
    } finally {
      this.fixingCheckId = null;
    }
  }

  async loadAuthAndSettings(): Promise<void> {
    this.loadingAuth = true;
    try {
      const overview = await invoke<PiAuthAndSettingsOverview>('get_pi_auth_and_settings');
      this.piOverview = overview;
    } catch (err) {
      this.authFeedback = {
        type: 'error',
        message: err instanceof Error ? err.message : String(err)
      };
    } finally {
      this.loadingAuth = false;
    }
  }

  async loadGithubAuthDetails(): Promise<void> {
    try {
      const details = await invoke<GithubAuthDetails>('get_github_auth_details');
      this.ghDetails = details;
    } catch {
      // Fallback when gh is unavailable
    }
  }

  async saveProviderKey(provider: string, apiKey: string): Promise<boolean> {
    this.savingAuth = true;
    this.authFeedback = null;
    try {
      const overview = await invoke<PiAuthAndSettingsOverview>('save_pi_provider_auth', {
        provider: provider.trim(),
        apiKey: apiKey.trim()
      });
      this.piOverview = overview;
      this.authFeedback = {
        type: 'success',
        message: `Saved API key for ${provider.trim()}.`
      };
      void chatStore.refreshConfiguredModels();
      void this.runDoctor();
      return true;
    } catch (err) {
      this.authFeedback = {
        type: 'error',
        message: err instanceof Error ? err.message : String(err)
      };
      return false;
    } finally {
      this.savingAuth = false;
    }
  }

  async removeProviderKey(provider: string): Promise<boolean> {
    this.savingAuth = true;
    this.authFeedback = null;
    try {
      const overview = await invoke<PiAuthAndSettingsOverview>('remove_pi_provider_auth', {
        provider: provider.trim()
      });
      this.piOverview = overview;
      this.authFeedback = {
        type: 'success',
        message: `Removed stored credentials for ${provider.trim()}.`
      };
      void this.runDoctor();
      return true;
    } catch (err) {
      this.authFeedback = {
        type: 'error',
        message: err instanceof Error ? err.message : String(err)
      };
      return false;
    } finally {
      this.savingAuth = false;
    }
  }

  async savePiSettings(settings: PiSettingsConfig): Promise<boolean> {
    this.savingAuth = true;
    this.authFeedback = null;
    try {
      const overview = await invoke<PiAuthAndSettingsOverview>('save_pi_settings', {
        settings
      });
      this.piOverview = overview;
      if (settings.githubDefaultRepo !== undefined) {
        githubStore.setRepoOverride(settings.githubDefaultRepo ?? '');
      }
      this.authFeedback = {
        type: 'success',
        message: 'Saved Pi & GitHub preferences to settings.json.'
      };
      void chatStore.refreshConfiguredModels();
      void this.loadGithubAuthDetails();
      return true;
    } catch (err) {
      this.authFeedback = {
        type: 'error',
        message: err instanceof Error ? err.message : String(err)
      };
      return false;
    } finally {
      this.savingAuth = false;
    }
  }

  async saveCustomModel(endpoint: CustomModelEndpoint): Promise<boolean> {
    this.savingAuth = true;
    this.authFeedback = null;
    try {
      const overview = await invoke<PiAuthAndSettingsOverview>('save_custom_model_endpoint', {
        endpoint
      });
      this.piOverview = overview;
      this.authFeedback = {
        type: 'success',
        message: `Saved custom model endpoint ${endpoint.provider}/${endpoint.id}.`
      };
      void chatStore.refreshConfiguredModels();
      void this.runDoctor();
      return true;
    } catch (err) {
      this.authFeedback = {
        type: 'error',
        message: err instanceof Error ? err.message : String(err)
      };
      return false;
    } finally {
      this.savingAuth = false;
    }
  }

  async deleteCustomModel(provider: string, modelId: string): Promise<boolean> {
    this.savingAuth = true;
    this.authFeedback = null;
    try {
      const overview = await invoke<PiAuthAndSettingsOverview>('delete_custom_model_endpoint', {
        provider,
        modelId
      });
      this.piOverview = overview;
      this.authFeedback = {
        type: 'success',
        message: `Removed custom model ${provider}/${modelId}.`
      };
      void chatStore.refreshConfiguredModels();
      return true;
    } catch (err) {
      this.authFeedback = {
        type: 'error',
        message: err instanceof Error ? err.message : String(err)
      };
      return false;
    } finally {
      this.savingAuth = false;
    }
  }

  async launchInteractivePiLogin(command = 'pi'): Promise<void> {
    const sid = chatStore.activeSessionId ?? 'default';
    const cwd = chatStore.sessionStatus.cwd ?? null;
    workspaceStore.openChatView();
    workspaceStore.openPanel('terminal');
    this.closeSettingsModal();
    this.closeDoctorModal();
    await terminalStore.executeCommand(sid, command, cwd);
  }
}

export const doctorStore = new DoctorStore();
