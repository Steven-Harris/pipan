//! In-App Login & Configuration (`~/.pi/agent/auth.json`, `settings.json`, `models.json`)
//! and detailed GitHub CLI (`gh auth status`) inspection & preferences.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::github::{gh_status, run_gh, webhook_extension_installed};
use crate::models::resolve_pi_agent_dir;
use crate::webhook::DEFAULT_EVENTS;

pub struct BuiltinProviderDef {
    pub id: &'static str,
    pub label: &'static str,
    pub env_vars: &'static [&'static str],
}

pub const BUILTIN_PROVIDERS: &[BuiltinProviderDef] = &[
    BuiltinProviderDef {
        id: "anthropic",
        label: "Anthropic",
        env_vars: &["ANTHROPIC_API_KEY"],
    },
    BuiltinProviderDef {
        id: "openai",
        label: "OpenAI",
        env_vars: &["OPENAI_API_KEY"],
    },
    BuiltinProviderDef {
        id: "google",
        label: "Google Gemini",
        env_vars: &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
    },
    BuiltinProviderDef {
        id: "openrouter",
        label: "OpenRouter",
        env_vars: &["OPENROUTER_API_KEY"],
    },
    BuiltinProviderDef {
        id: "xai",
        label: "xAI (Grok)",
        env_vars: &["XAI_API_KEY"],
    },
    BuiltinProviderDef {
        id: "mistral",
        label: "Mistral",
        env_vars: &["MISTRAL_API_KEY"],
    },
    BuiltinProviderDef {
        id: "groq",
        label: "Groq",
        env_vars: &["GROQ_API_KEY"],
    },
];

pub const VALID_THINKING_LEVELS: &[&str] =
    &["off", "minimal", "low", "medium", "high", "xhigh", "max"];
pub const VALID_QUEUE_MODES: &[&str] = &["one-at-a-time", "all"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderAuthStatus {
    pub id: String,
    pub label: String,
    pub is_builtin: bool,
    pub configured: bool,
    /// `"api_key"` | `"oauth"` | `"env"`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env_var: Option<String>,
    pub has_env_key: bool,
    pub has_stored_credential: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub oauth_email: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiSettingsConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    pub default_thinking_level: String,
    pub auto_compaction: bool,
    pub auto_retry: bool,
    pub steering_mode: String,
    pub follow_up_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub github_default_repo: Option<String>,
    pub github_webhook_events: Vec<String>,
}

impl Default for PiSettingsConfig {
    fn default() -> Self {
        Self {
            default_provider: None,
            default_model: None,
            default_thinking_level: "medium".to_string(),
            auto_compaction: true,
            auto_retry: true,
            steering_mode: "one-at-a-time".to_string(),
            follow_up_mode: "one-at-a-time".to_string(),
            github_default_repo: None,
            github_webhook_events: DEFAULT_EVENTS.iter().map(|s| (*s).to_string()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomModelEndpoint {
    pub provider: String,
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub api: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    pub reasoning: bool,
    pub supports_images: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PiAuthAndSettingsOverview {
    pub agent_dir: String,
    pub auth_file_path: String,
    pub settings_file_path: String,
    pub models_file_path: String,
    pub providers: Vec<ProviderAuthStatus>,
    pub settings: PiSettingsConfig,
    pub custom_models: Vec<CustomModelEndpoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubAuthDetails {
    pub installed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub authenticated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    pub hostname: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_protocol: Option<String>,
    pub scopes: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_source: Option<String>,
    pub webhook_extension: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_repo: Option<String>,
    pub webhook_events: Vec<String>,
    pub raw_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedGhAuthStatus {
    pub hostname: Option<String>,
    pub account: Option<String>,
    pub git_protocol: Option<String>,
    pub scopes: Vec<String>,
    pub token_source: Option<String>,
    pub authenticated: bool,
}

/// Masks an API key or secret token so raw values are never exposed to the UI.
pub fn mask_secret(secret: &str) -> String {
    let trimmed = secret.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let chars: Vec<char> = trimmed.chars().collect();
    if chars.len() <= 8 {
        return "••••••••".to_string();
    }
    let prefix: String = chars[..4].iter().collect();
    let suffix: String = chars[chars.len() - 4..].iter().collect();
    format!("{prefix}••••{suffix}")
}

pub fn validate_provider_id(provider: &str) -> Result<String, String> {
    let trimmed = provider.trim().to_lowercase();
    if trimmed.is_empty() {
        return Err("Provider ID must not be empty".to_string());
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        return Err(format!("Invalid provider ID: {provider}"));
    }
    Ok(trimmed)
}

fn strip_utf8_bom(content: &str) -> &str {
    content.strip_prefix('\u{feff}').unwrap_or(content)
}

fn read_json_object(path: &Path) -> Result<Map<String, Value>, String> {
    if !path.exists() {
        return Ok(Map::new());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
    let cleaned = strip_utf8_bom(&raw).trim();
    if cleaned.is_empty() {
        return Ok(Map::new());
    }
    let val: Value = serde_json::from_str(cleaned)
        .map_err(|e| format!("Invalid JSON in {}: {}", path.display(), e))?;
    match val {
        Value::Object(map) => Ok(map),
        _ => Err(format!("Expected a JSON object in {}", path.display())),
    }
}

fn write_json_object(path: &Path, map: &Map<String, Value>) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }
    let serialized = serde_json::to_string_pretty(&Value::Object(map.clone()))
        .map_err(|e| format!("Failed to serialize JSON for {}: {}", path.display(), e))?;
    std::fs::write(path, format!("{serialized}\n"))
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))
}

fn inspect_stored_credential(cred: &Value) -> (Option<String>, Option<String>, Option<String>) {
    match cred {
        Value::String(s) if !s.trim().is_empty() => {
            (Some("api_key".to_string()), Some(mask_secret(s)), None)
        }
        Value::Object(obj) => {
            let cred_type = obj.get("type").and_then(Value::as_str).unwrap_or("api_key");
            if cred_type == "oauth" {
                let email = obj
                    .get("email")
                    .or_else(|| obj.get("account"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let has_tokens = obj
                    .get("access")
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty())
                    || obj
                        .get("refresh")
                        .and_then(Value::as_str)
                        .is_some_and(|s| !s.trim().is_empty());
                if has_tokens {
                    let preview = match &email {
                        Some(addr) => format!("OAuth ({addr})"),
                        None => "OAuth token".to_string(),
                    };
                    return (Some("oauth".to_string()), Some(preview), email);
                }
                (None, None, None)
            } else {
                let key = obj
                    .get("key")
                    .or_else(|| obj.get("apiKey"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                if let Some(k) = key {
                    (Some("api_key".to_string()), Some(mask_secret(k)), None)
                } else {
                    (None, None, None)
                }
            }
        }
        _ => (None, None, None),
    }
}

fn check_env_vars(vars: &[&str]) -> Option<(String, String)> {
    for var_name in vars {
        if let Ok(val) = std::env::var(var_name) {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                return Some(((*var_name).to_string(), mask_secret(trimmed)));
            }
        }
    }
    None
}

pub fn load_provider_statuses_from_dir(agent_dir: &Path) -> Vec<ProviderAuthStatus> {
    let auth_path = agent_dir.join("auth.json");
    let auth_map = read_json_object(&auth_path).unwrap_or_default();

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    for def in BUILTIN_PROVIDERS {
        seen.insert(def.id.to_string());
        let stored = auth_map.get(def.id);
        let (stored_type, stored_preview, oauth_email) = stored
            .map(inspect_stored_credential)
            .unwrap_or((None, None, None));
        let has_stored_credential = stored_type.is_some();

        let env_hit = check_env_vars(def.env_vars);
        let has_env_key = env_hit.is_some();
        let primary_env = def.env_vars.first().map(|s| (*s).to_string());

        let (auth_type, masked_preview) = if has_stored_credential {
            (stored_type, stored_preview)
        } else if let Some((env_name, env_masked)) = env_hit {
            (
                Some("env".to_string()),
                Some(format!("${env_name} ({env_masked})")),
            )
        } else {
            (None, None)
        };

        out.push(ProviderAuthStatus {
            id: def.id.to_string(),
            label: def.label.to_string(),
            is_builtin: true,
            configured: has_stored_credential || has_env_key,
            auth_type,
            masked_preview,
            env_var: primary_env,
            has_env_key,
            has_stored_credential,
            oauth_email,
        });
    }

    let mut custom_keys: Vec<&String> = auth_map
        .keys()
        .filter(|k| !seen.contains(k.as_str()))
        .collect();
    custom_keys.sort();

    for key in custom_keys {
        if let Some(val) = auth_map.get(key) {
            let (stored_type, stored_preview, oauth_email) = inspect_stored_credential(val);
            let has_stored_credential = stored_type.is_some();
            out.push(ProviderAuthStatus {
                id: key.clone(),
                label: key.clone(),
                is_builtin: false,
                configured: has_stored_credential,
                auth_type: stored_type,
                masked_preview: stored_preview,
                env_var: None,
                has_env_key: false,
                has_stored_credential,
                oauth_email,
            });
        }
    }

    out
}

/// Checks whether at least one provider API key or credential is configured in
/// `<agent_dir>/auth.json`, `<agent_dir>/models.json`, or known environment variables.
pub fn has_any_pi_credential_in_dir(agent_dir: Option<&Path>) -> (bool, Vec<String>) {
    let mut configured_sources = Vec::new();

    if let Some(dir) = agent_dir {
        for status in load_provider_statuses_from_dir(dir) {
            if status.configured {
                let kind = status.auth_type.unwrap_or_else(|| "key".to_string());
                configured_sources.push(format!("{} ({})", status.id, kind));
            }
        }
        let custom_models = load_custom_models_from_dir(dir);
        if !custom_models.is_empty() && configured_sources.is_empty() {
            configured_sources.push(format!("{} custom model endpoint(s)", custom_models.len()));
        }
    } else {
        for def in BUILTIN_PROVIDERS {
            if check_env_vars(def.env_vars).is_some() {
                configured_sources.push(format!("{} (env)", def.id));
            }
        }
    }

    (!configured_sources.is_empty(), configured_sources)
}

pub fn save_provider_api_key_in_dir(
    agent_dir: &Path,
    provider: &str,
    api_key: &str,
) -> Result<(), String> {
    let provider_id = validate_provider_id(provider)?;
    let trimmed_key = api_key.trim();
    let auth_path = agent_dir.join("auth.json");
    let mut map = read_json_object(&auth_path)?;

    if trimmed_key.is_empty() {
        map.remove(&provider_id);
    } else {
        map.insert(
            provider_id,
            json!({
                "type": "api_key",
                "key": trimmed_key
            }),
        );
    }

    write_json_object(&auth_path, &map)
}

pub fn remove_provider_auth_in_dir(agent_dir: &Path, provider: &str) -> Result<(), String> {
    let provider_id = validate_provider_id(provider)?;
    let auth_path = agent_dir.join("auth.json");
    let mut map = read_json_object(&auth_path)?;
    map.remove(&provider_id);
    write_json_object(&auth_path, &map)
}

pub fn load_settings_from_dir(agent_dir: &Path) -> PiSettingsConfig {
    let settings_path = agent_dir.join("settings.json");
    let Ok(map) = read_json_object(&settings_path) else {
        return PiSettingsConfig::default();
    };

    let str_opt = |keys: &[&str]| -> Option<String> {
        for k in keys {
            if let Some(s) = map
                .get(*k)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return Some(s.to_string());
            }
        }
        None
    };

    let default_provider = str_opt(&["defaultProvider", "provider"]);
    let default_model = str_opt(&["defaultModel", "model"]);
    let default_thinking_level = str_opt(&["defaultThinkingLevel", "thinkingLevel", "thinking"])
        .filter(|lvl| VALID_THINKING_LEVELS.contains(&lvl.as_str()))
        .unwrap_or_else(|| "medium".to_string());

    let auto_compaction = map
        .get("autoCompaction")
        .and_then(Value::as_bool)
        .or_else(|| {
            map.get("compaction")
                .and_then(Value::as_object)
                .and_then(|c| c.get("enabled"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(true);

    let auto_retry = map
        .get("autoRetry")
        .and_then(Value::as_bool)
        .or_else(|| {
            map.get("retry")
                .and_then(Value::as_object)
                .and_then(|r| r.get("enabled"))
                .and_then(Value::as_bool)
        })
        .unwrap_or(true);

    let steering_mode = str_opt(&["steeringMode"])
        .filter(|m| VALID_QUEUE_MODES.contains(&m.as_str()))
        .unwrap_or_else(|| "one-at-a-time".to_string());

    let follow_up_mode = str_opt(&["followUpMode"])
        .filter(|m| VALID_QUEUE_MODES.contains(&m.as_str()))
        .unwrap_or_else(|| "one-at-a-time".to_string());

    let gh_obj = map.get("pipanGithub").and_then(Value::as_object);
    let github_default_repo = gh_obj
        .and_then(|g| g.get("defaultRepo"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let github_webhook_events = gh_obj
        .and_then(|g| g.get("webhookEvents"))
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_EVENTS.iter().map(|s| (*s).to_string()).collect());

    PiSettingsConfig {
        default_provider,
        default_model,
        default_thinking_level,
        auto_compaction,
        auto_retry,
        steering_mode,
        follow_up_mode,
        github_default_repo,
        github_webhook_events,
    }
}

pub fn save_settings_in_dir(agent_dir: &Path, settings: &PiSettingsConfig) -> Result<(), String> {
    if !VALID_THINKING_LEVELS.contains(&settings.default_thinking_level.as_str()) {
        return Err(format!(
            "Invalid defaultThinkingLevel `{}`",
            settings.default_thinking_level
        ));
    }
    if !VALID_QUEUE_MODES.contains(&settings.steering_mode.as_str()) {
        return Err(format!("Invalid steeringMode `{}`", settings.steering_mode));
    }
    if !VALID_QUEUE_MODES.contains(&settings.follow_up_mode.as_str()) {
        return Err(format!(
            "Invalid followUpMode `{}`",
            settings.follow_up_mode
        ));
    }

    let settings_path = agent_dir.join("settings.json");
    let mut map = read_json_object(&settings_path)?;

    match settings
        .default_provider
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(p) => {
            map.insert("defaultProvider".to_string(), Value::String(p.to_string()));
        }
        None => {
            map.remove("defaultProvider");
        }
    }

    match settings
        .default_model
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(m) => {
            map.insert("defaultModel".to_string(), Value::String(m.to_string()));
        }
        None => {
            map.remove("defaultModel");
        }
    }

    map.insert(
        "defaultThinkingLevel".to_string(),
        Value::String(settings.default_thinking_level.clone()),
    );
    map.insert(
        "autoCompaction".to_string(),
        Value::Bool(settings.auto_compaction),
    );
    map.insert("autoRetry".to_string(), Value::Bool(settings.auto_retry));
    map.insert(
        "steeringMode".to_string(),
        Value::String(settings.steering_mode.clone()),
    );
    map.insert(
        "followUpMode".to_string(),
        Value::String(settings.follow_up_mode.clone()),
    );

    // Sync nested `compaction.enabled` while preserving other compaction fields.
    let mut compaction_obj = map
        .get("compaction")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    compaction_obj.insert("enabled".to_string(), Value::Bool(settings.auto_compaction));
    map.insert("compaction".to_string(), Value::Object(compaction_obj));

    // Sync nested `retry.enabled` while preserving other retry fields.
    let mut retry_obj = map
        .get("retry")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    retry_obj.insert("enabled".to_string(), Value::Bool(settings.auto_retry));
    map.insert("retry".to_string(), Value::Object(retry_obj));

    // Persist GitHub default repo & webhook event preferences under `pipanGithub`.
    let clean_events: Vec<String> = settings
        .github_webhook_events
        .iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let events_to_store = if clean_events.is_empty() {
        DEFAULT_EVENTS
            .iter()
            .map(|s| (*s).to_string())
            .collect::<Vec<_>>()
    } else {
        clean_events
    };

    let mut gh_obj = map
        .get("pipanGithub")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    match settings
        .github_default_repo
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(repo) => {
            gh_obj.insert("defaultRepo".to_string(), Value::String(repo.to_string()));
        }
        None => {
            gh_obj.remove("defaultRepo");
        }
    }
    gh_obj.insert("webhookEvents".to_string(), json!(events_to_store));
    map.insert("pipanGithub".to_string(), Value::Object(gh_obj));

    write_json_object(&settings_path, &map)
}

pub fn load_custom_models_from_dir(agent_dir: &Path) -> Vec<CustomModelEndpoint> {
    let models_path = agent_dir.join("models.json");
    let Ok(map) = read_json_object(&models_path) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    let mut seen = HashSet::new();

    if let Some(providers_obj) = map.get("providers").and_then(Value::as_object) {
        for (prov_name, prov_val) in providers_obj {
            let Some(prov_obj) = prov_val.as_object() else {
                continue;
            };
            let prov_base_url = prov_obj
                .get("baseUrl")
                .or_else(|| prov_obj.get("base_url"))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let prov_api = prov_obj
                .get("api")
                .and_then(Value::as_str)
                .unwrap_or("openai-completions")
                .trim()
                .to_string();
            let prov_api_key_env = prov_obj
                .get("apiKey")
                .or_else(|| prov_obj.get("apiKeyEnv"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            if let Some(models_arr) = prov_obj.get("models").and_then(Value::as_array) {
                for item in models_arr {
                    let Some(m_obj) = item.as_object() else {
                        continue;
                    };
                    let Some(id) = m_obj
                        .get("id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    else {
                        continue;
                    };
                    let key = (prov_name.to_lowercase(), id.to_lowercase());
                    if !seen.insert(key) {
                        continue;
                    }
                    let name = m_obj
                        .get("name")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .unwrap_or(id)
                        .to_string();
                    let base_url = m_obj
                        .get("baseUrl")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| prov_base_url.clone());
                    let api = m_obj
                        .get("api")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                        .unwrap_or_else(|| prov_api.clone());
                    let context_window = m_obj
                        .get("contextWindow")
                        .or_else(|| m_obj.get("context_window"))
                        .and_then(Value::as_u64);
                    let max_tokens = m_obj
                        .get("maxTokens")
                        .or_else(|| m_obj.get("max_tokens"))
                        .and_then(Value::as_u64);
                    let reasoning = m_obj
                        .get("reasoning")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let supports_images = m_obj
                        .get("supportsImages")
                        .and_then(Value::as_bool)
                        .or_else(|| {
                            m_obj
                                .get("input")
                                .and_then(Value::as_array)
                                .map(|arr| arr.iter().any(|v| v.as_str() == Some("image")))
                        })
                        .unwrap_or(false);

                    out.push(CustomModelEndpoint {
                        provider: prov_name.clone(),
                        id: id.to_string(),
                        name,
                        base_url,
                        api,
                        api_key_env: prov_api_key_env.clone(),
                        context_window,
                        max_tokens,
                        reasoning,
                        supports_images,
                    });
                }
            }
        }
    }

    out
}

pub fn save_custom_model_in_dir(
    agent_dir: &Path,
    endpoint: &CustomModelEndpoint,
) -> Result<(), String> {
    let provider_id = validate_provider_id(&endpoint.provider)?;
    let model_id = endpoint.id.trim();
    if model_id.is_empty() {
        return Err("Model ID must not be empty".to_string());
    }
    let base_url = endpoint.base_url.trim();
    if base_url.is_empty() {
        return Err("Base URL must not be empty".to_string());
    }
    let display_name = if endpoint.name.trim().is_empty() {
        model_id.to_string()
    } else {
        endpoint.name.trim().to_string()
    };
    let api_kind = if endpoint.api.trim().is_empty() {
        "openai-completions".to_string()
    } else {
        endpoint.api.trim().to_string()
    };

    let models_path = agent_dir.join("models.json");
    let mut root = read_json_object(&models_path)?;

    let mut providers_obj = root
        .get("providers")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let mut prov_entry = providers_obj
        .get(&provider_id)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    prov_entry.insert("baseUrl".to_string(), Value::String(base_url.to_string()));
    prov_entry.insert("api".to_string(), Value::String(api_kind.clone()));
    if let Some(key_env) = endpoint
        .api_key_env
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        prov_entry.insert("apiKey".to_string(), Value::String(key_env.to_string()));
    }

    let mut models_arr = prov_entry
        .get("models")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let mut input_types = vec![Value::String("text".to_string())];
    if endpoint.supports_images {
        input_types.push(Value::String("image".to_string()));
    }

    let mut model_obj = Map::new();
    model_obj.insert("id".to_string(), Value::String(model_id.to_string()));
    model_obj.insert("name".to_string(), Value::String(display_name));
    model_obj.insert("reasoning".to_string(), Value::Bool(endpoint.reasoning));
    model_obj.insert(
        "supportsImages".to_string(),
        Value::Bool(endpoint.supports_images),
    );
    model_obj.insert("input".to_string(), Value::Array(input_types));
    if let Some(cw) = endpoint.context_window {
        model_obj.insert("contextWindow".to_string(), json!(cw));
    }
    if let Some(mt) = endpoint.max_tokens {
        model_obj.insert("maxTokens".to_string(), json!(mt));
    }

    let new_model_val = Value::Object(model_obj);
    if let Some(existing_slot) = models_arr.iter_mut().find(|item| {
        item.get("id")
            .and_then(Value::as_str)
            .is_some_and(|id| id.eq_ignore_ascii_case(model_id))
    }) {
        *existing_slot = new_model_val;
    } else {
        models_arr.push(new_model_val);
    }

    prov_entry.insert("models".to_string(), Value::Array(models_arr));
    providers_obj.insert(provider_id, Value::Object(prov_entry));
    root.insert("providers".to_string(), Value::Object(providers_obj));

    write_json_object(&models_path, &root)
}

pub fn delete_custom_model_in_dir(
    agent_dir: &Path,
    provider: &str,
    model_id: &str,
) -> Result<(), String> {
    let provider_id = validate_provider_id(provider)?;
    let clean_model_id = model_id.trim();
    if clean_model_id.is_empty() {
        return Err("Model ID must not be empty".to_string());
    }

    let models_path = agent_dir.join("models.json");
    let mut root = read_json_object(&models_path)?;
    if let Some(providers_obj) = root.get_mut("providers").and_then(Value::as_object_mut) {
        let mut remove_provider = false;
        if let Some(prov_entry) = providers_obj
            .get_mut(&provider_id)
            .and_then(Value::as_object_mut)
            && let Some(models_arr) = prov_entry.get_mut("models").and_then(Value::as_array_mut)
        {
            models_arr.retain(|item| {
                !item
                    .get("id")
                    .and_then(Value::as_str)
                    .is_some_and(|id| id.eq_ignore_ascii_case(clean_model_id))
            });
            if models_arr.is_empty() {
                remove_provider = true;
            }
        }
        if remove_provider {
            providers_obj.remove(&provider_id);
        }
    }

    write_json_object(&models_path, &root)
}

pub fn load_overview_from_dir(agent_dir: &Path) -> PiAuthAndSettingsOverview {
    PiAuthAndSettingsOverview {
        agent_dir: agent_dir.to_string_lossy().to_string(),
        auth_file_path: agent_dir.join("auth.json").to_string_lossy().to_string(),
        settings_file_path: agent_dir
            .join("settings.json")
            .to_string_lossy()
            .to_string(),
        models_file_path: agent_dir.join("models.json").to_string_lossy().to_string(),
        providers: load_provider_statuses_from_dir(agent_dir),
        settings: load_settings_from_dir(agent_dir),
        custom_models: load_custom_models_from_dir(agent_dir),
    }
}

fn require_agent_dir() -> Result<PathBuf, String> {
    resolve_pi_agent_dir().ok_or_else(|| "Could not resolve ~/.pi/agent directory".to_string())
}

/// Parses `gh auth status` stdout/stderr into structured fields (account, protocol, scopes, hostname).
pub fn parse_gh_auth_status_output(text: &str) -> ParsedGhAuthStatus {
    let mut out = ParsedGhAuthStatus::default();

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        // Top-level hostname line (e.g., "github.com")
        if out.hostname.is_none()
            && !line.starts_with('-')
            && !line.starts_with('✓')
            && !line.starts_with('X')
            && !line.contains(' ')
            && line.contains('.')
        {
            out.hostname = Some(line.to_string());
            continue;
        }

        // Account line: "✓ Logged in to github.com account Steven-Harris (keyring)"
        // or "✓ Logged in to github.com as Steven-Harris (oauth_token)"
        if line.contains("Logged in to ") {
            out.authenticated = true;
            if let Some((_, after_logged)) = line.split_once("Logged in to ") {
                let mut parts = after_logged.split_whitespace();
                if let Some(host_tok) = parts.next()
                    && out.hostname.is_none()
                {
                    out.hostname = Some(host_tok.to_string());
                }
            }
            if let Some((_, after_acct)) = line
                .split_once(" account ")
                .or_else(|| line.split_once(" as "))
            {
                let mut words = after_acct.split_whitespace();
                if let Some(acct) = words.next() {
                    out.account = Some(acct.trim().to_string());
                }
                if let Some(src_raw) = words.next() {
                    let src = src_raw.trim_matches(|c: char| c == '(' || c == ')');
                    if !src.is_empty() {
                        out.token_source = Some(src.to_string());
                    }
                }
            }
        }

        if let Some((_, proto)) = line.split_once("Git operations protocol:") {
            let p = proto.trim();
            if !p.is_empty() {
                out.git_protocol = Some(p.to_string());
            }
        }

        if let Some((_, scopes_raw)) = line.split_once("Token scopes:") {
            let parsed_scopes: Vec<String> = scopes_raw
                .split(',')
                .map(|s| s.trim().trim_matches('\'').trim_matches('"').to_string())
                .filter(|s| !s.is_empty() && s != "none")
                .collect();
            out.scopes = parsed_scopes;
        }
    }

    out
}

// ============================================================================
// Tauri Commands
// ============================================================================

#[tauri::command]
pub async fn get_pi_auth_and_settings() -> Result<PiAuthAndSettingsOverview, String> {
    tokio::task::spawn_blocking(|| {
        let dir = require_agent_dir()?;
        Ok(load_overview_from_dir(&dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn save_pi_provider_auth(
    provider: String,
    api_key: String,
) -> Result<PiAuthAndSettingsOverview, String> {
    tokio::task::spawn_blocking(move || {
        let dir = require_agent_dir()?;
        save_provider_api_key_in_dir(&dir, &provider, &api_key)?;
        Ok(load_overview_from_dir(&dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn remove_pi_provider_auth(
    provider: String,
) -> Result<PiAuthAndSettingsOverview, String> {
    tokio::task::spawn_blocking(move || {
        let dir = require_agent_dir()?;
        remove_provider_auth_in_dir(&dir, &provider)?;
        Ok(load_overview_from_dir(&dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn save_pi_settings(
    settings: PiSettingsConfig,
) -> Result<PiAuthAndSettingsOverview, String> {
    tokio::task::spawn_blocking(move || {
        let dir = require_agent_dir()?;
        save_settings_in_dir(&dir, &settings)?;
        Ok(load_overview_from_dir(&dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn save_custom_model_endpoint(
    endpoint: CustomModelEndpoint,
) -> Result<PiAuthAndSettingsOverview, String> {
    tokio::task::spawn_blocking(move || {
        let dir = require_agent_dir()?;
        save_custom_model_in_dir(&dir, &endpoint)?;
        Ok(load_overview_from_dir(&dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn delete_custom_model_endpoint(
    provider: String,
    model_id: String,
) -> Result<PiAuthAndSettingsOverview, String> {
    tokio::task::spawn_blocking(move || {
        let dir = require_agent_dir()?;
        delete_custom_model_in_dir(&dir, &provider, &model_id)?;
        Ok(load_overview_from_dir(&dir))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_github_auth_details() -> Result<GithubAuthDetails, String> {
    let basic = gh_status().await?;
    let settings = resolve_pi_agent_dir()
        .map(|d| load_settings_from_dir(&d))
        .unwrap_or_default();

    if !basic.installed {
        return Ok(GithubAuthDetails {
            installed: false,
            version: None,
            authenticated: false,
            account: None,
            hostname: "github.com".to_string(),
            git_protocol: None,
            scopes: Vec::new(),
            token_source: None,
            webhook_extension: false,
            default_repo: settings.github_default_repo,
            webhook_events: settings.github_webhook_events,
            raw_status: String::new(),
            error: basic.error,
        });
    }

    let raw_status = match run_gh(&["auth".to_string(), "status".to_string()], None).await {
        Ok(out) => out,
        Err(err) => err,
    };
    let parsed = parse_gh_auth_status_output(&raw_status);

    Ok(GithubAuthDetails {
        installed: true,
        version: basic.version,
        authenticated: basic.authenticated || parsed.authenticated,
        account: basic.account.or(parsed.account),
        hostname: parsed.hostname.unwrap_or_else(|| "github.com".to_string()),
        git_protocol: parsed.git_protocol,
        scopes: parsed.scopes,
        token_source: parsed.token_source,
        webhook_extension: basic.webhook_extension || webhook_extension_installed().await,
        default_repo: settings.github_default_repo,
        webhook_events: settings.github_webhook_events,
        raw_status,
        error: basic.error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_agent_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pipan-auth-test-{}-{}-{}",
            label,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn mask_secret_hides_middle_and_short_strings() {
        assert_eq!(mask_secret(""), "");
        assert_eq!(mask_secret("short"), "••••••••");
        assert_eq!(mask_secret("12345678"), "••••••••");
        assert_eq!(mask_secret("sk-ant-api03-abcdef9876"), "sk-a••••9876");
    }

    #[test]
    fn saves_and_removes_provider_keys_without_clobbering_oauth() {
        let dir = temp_agent_dir("auth-crud");
        let auth_path = dir.join("auth.json");
        std::fs::write(
            &auth_path,
            r#"{
  "antigravity": {
    "type": "oauth",
    "access": "acc-token-12345",
    "refresh": "ref-token-67890",
    "expires": 1900000000,
    "email": "dev@example.com"
  }
}"#,
        )
        .unwrap();

        save_provider_api_key_in_dir(&dir, "anthropic", "sk-ant-api03-secretkey1234").unwrap();
        save_provider_api_key_in_dir(&dir, "custom-llm", "custom-secret-key-9999").unwrap();

        let statuses = load_provider_statuses_from_dir(&dir);
        let anth = statuses.iter().find(|p| p.id == "anthropic").unwrap();
        assert!(anth.configured);
        assert!(anth.has_stored_credential);
        assert_eq!(anth.auth_type.as_deref(), Some("api_key"));
        assert_eq!(anth.masked_preview.as_deref(), Some("sk-a••••1234"));

        let oauth = statuses.iter().find(|p| p.id == "antigravity").unwrap();
        assert!(oauth.configured);
        assert_eq!(oauth.auth_type.as_deref(), Some("oauth"));
        assert_eq!(oauth.oauth_email.as_deref(), Some("dev@example.com"));

        let custom = statuses.iter().find(|p| p.id == "custom-llm").unwrap();
        assert!(custom.configured);
        assert!(!custom.is_builtin);
        assert_eq!(custom.masked_preview.as_deref(), Some("cust••••9999"));

        remove_provider_auth_in_dir(&dir, "anthropic").unwrap();
        let after = load_provider_statuses_from_dir(&dir);
        let anth_after = after.iter().find(|p| p.id == "anthropic").unwrap();
        assert!(!anth_after.has_stored_credential);
        // OAuth entry is still intact
        assert!(after.iter().any(|p| p.id == "antigravity" && p.configured));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn saves_settings_preserving_unrelated_keys() {
        let dir = temp_agent_dir("settings-preserve");
        let settings_path = dir.join("settings.json");
        std::fs::write(
            &settings_path,
            r#"{
  "lastChangelogVersion": "1.5.0",
  "packages": ["npm:pi-subagents"],
  "compaction": {
    "enabled": true,
    "reserveTokens": 24000
  }
}"#,
        )
        .unwrap();

        let updated = PiSettingsConfig {
            default_provider: Some("anthropic".to_string()),
            default_model: Some("claude-sonnet-4-20250514".to_string()),
            default_thinking_level: "high".to_string(),
            auto_compaction: false,
            auto_retry: false,
            steering_mode: "all".to_string(),
            follow_up_mode: "one-at-a-time".to_string(),
            github_default_repo: Some("owner/repo".to_string()),
            github_webhook_events: vec!["pull_request".to_string(), "check_run".to_string()],
        };
        save_settings_in_dir(&dir, &updated).unwrap();

        let reloaded = load_settings_from_dir(&dir);
        assert_eq!(reloaded, updated);

        let raw_map = read_json_object(&settings_path).unwrap();
        assert_eq!(
            raw_map.get("lastChangelogVersion").and_then(Value::as_str),
            Some("1.5.0")
        );
        assert_eq!(
            raw_map
                .get("compaction")
                .and_then(|c| c.get("reserveTokens"))
                .and_then(Value::as_u64),
            Some(24000)
        );
        assert_eq!(
            raw_map
                .get("compaction")
                .and_then(|c| c.get("enabled"))
                .and_then(Value::as_bool),
            Some(false)
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn custom_models_crud_roundtrip() {
        let dir = temp_agent_dir("custom-models");
        let endpoint = CustomModelEndpoint {
            provider: "ollama".to_string(),
            id: "qwen2.5-coder:32b".to_string(),
            name: "Qwen 2.5 Coder 32B".to_string(),
            base_url: "http://localhost:11434/v1".to_string(),
            api: "openai-completions".to_string(),
            api_key_env: None,
            context_window: Some(131072),
            max_tokens: Some(16384),
            reasoning: true,
            supports_images: false,
        };

        save_custom_model_in_dir(&dir, &endpoint).unwrap();
        let list = load_custom_models_from_dir(&dir);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0], endpoint);

        delete_custom_model_in_dir(&dir, "ollama", "qwen2.5-coder:32b").unwrap();
        assert!(load_custom_models_from_dir(&dir).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_gh_auth_status_output_accurately() {
        let sample = "\
github.com
  ✓ Logged in to github.com account Steven-Harris (keyring)
  - Active account: true
  - Git operations protocol: https
  - Token: gho_************************************
  - Token scopes: 'gist', 'read:org', 'repo', 'workflow'
";
        let parsed = parse_gh_auth_status_output(sample);
        assert!(parsed.authenticated);
        assert_eq!(parsed.hostname.as_deref(), Some("github.com"));
        assert_eq!(parsed.account.as_deref(), Some("Steven-Harris"));
        assert_eq!(parsed.token_source.as_deref(), Some("keyring"));
        assert_eq!(parsed.git_protocol.as_deref(), Some("https"));
        assert_eq!(parsed.scopes, vec!["gist", "read:org", "repo", "workflow"]);
    }
}
