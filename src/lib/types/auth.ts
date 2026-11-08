/**
 * Types for In-App Login & Configuration (`src-tauri/src/auth.rs`).
 */

export type PiThinkingLevelOption =
  | 'off'
  | 'minimal'
  | 'low'
  | 'medium'
  | 'high'
  | 'xhigh'
  | 'max';

export type PiQueueDeliveryMode = 'one-at-a-time' | 'all';

export interface ProviderAuthStatus {
  id: string;
  label: string;
  isBuiltin: boolean;
  configured: boolean;
  authType?: 'api_key' | 'oauth' | 'env' | string;
  maskedPreview?: string;
  envVar?: string;
  hasEnvKey: boolean;
  hasStoredCredential: boolean;
  oauthEmail?: string;
}

export interface PiSettingsConfig {
  defaultProvider?: string | null;
  defaultModel?: string | null;
  defaultThinkingLevel: PiThinkingLevelOption | string;
  autoCompaction: boolean;
  autoRetry: boolean;
  steeringMode: PiQueueDeliveryMode | string;
  followUpMode: PiQueueDeliveryMode | string;
  githubDefaultRepo?: string | null;
  githubWebhookEvents: string[];
}

export interface CustomModelEndpoint {
  provider: string;
  id: string;
  name: string;
  baseUrl: string;
  api: 'openai-completions' | 'anthropic-messages' | 'openai-responses' | string;
  apiKeyEnv?: string | null;
  contextWindow?: number | null;
  maxTokens?: number | null;
  reasoning: boolean;
  supportsImages: boolean;
}

export interface PiAuthAndSettingsOverview {
  agentDir: string;
  authFilePath: string;
  settingsFilePath: string;
  modelsFilePath: string;
  providers: ProviderAuthStatus[];
  settings: PiSettingsConfig;
  customModels: CustomModelEndpoint[];
}

export interface GithubAuthDetails {
  installed: boolean;
  version?: string | null;
  authenticated: boolean;
  account?: string | null;
  hostname: string;
  gitProtocol?: string | null;
  scopes: string[];
  tokenSource?: string | null;
  webhookExtension: boolean;
  defaultRepo?: string | null;
  webhookEvents: string[];
  rawStatus: string;
  error?: string | null;
}
