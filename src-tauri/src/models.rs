//! Model configuration discovery from `~/.pi/agent/models.json` and `~/.pi/agent/settings.json`,
//! with fallback metadata for standard Pi providers so model selection works before a session starts.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostInfo {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_write: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ConfiguredModelInfo {
    pub id: String,
    pub name: String,
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_window: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    pub reasoning: bool,
    pub supports_images: bool,
    pub input: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost: Option<ModelCostInfo>,
    pub is_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_thinking_level: Option<String>,
}

pub fn resolve_user_home_dir() -> Option<PathBuf> {
    if let Some(home) = std::env::var_os("HOME").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(home));
    }
    if let Some(profile) = std::env::var_os("USERPROFILE").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(profile));
    }
    None
}

pub fn resolve_pi_agent_dir() -> Option<PathBuf> {
    if let Some(custom) = std::env::var_os("PI_CODING_AGENT_DIR").filter(|s| !s.is_empty()) {
        return Some(PathBuf::from(custom));
    }
    resolve_user_home_dir().map(|h| h.join(".pi").join("agent"))
}

#[derive(Debug, Default)]
struct ParsedPiSettings {
    default_provider: Option<String>,
    default_model: Option<String>,
    default_thinking_level: Option<String>,
    enabled_models: Vec<(Option<String>, String)>,
}

fn split_provider_model(raw: &str) -> (Option<String>, String) {
    let trimmed = raw.trim();
    if let Some((prov, model)) = trimmed.split_once('/') {
        let p = prov.trim();
        let m = model.trim();
        if !p.is_empty() && !m.is_empty() {
            return (Some(p.to_string()), m.to_string());
        }
    }
    (None, trimmed.to_string())
}

fn parse_settings_json(content: &str) -> ParsedPiSettings {
    let Ok(val) = serde_json::from_str::<Value>(content) else {
        return ParsedPiSettings::default();
    };

    let str_field = |keys: &[&str]| -> Option<String> {
        for k in keys {
            if let Some(s) = val
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

    let mut default_provider = str_field(&["defaultProvider", "provider"]);
    let mut default_model = str_field(&["defaultModel", "model"]);
    if let Some(raw_model) = default_model.clone() {
        let (maybe_prov, clean_model) = split_provider_model(&raw_model);
        if default_provider.is_none() {
            default_provider = maybe_prov;
        }
        default_model = Some(clean_model);
    }

    let default_thinking_level = str_field(&["defaultThinkingLevel", "thinkingLevel", "thinking"]);

    let mut enabled_models = Vec::new();
    if let Some(arr) = val.get("enabledModels").and_then(Value::as_array) {
        for item in arr {
            if let Some(s) = item.as_str().map(str::trim).filter(|s| !s.is_empty()) {
                enabled_models.push(split_provider_model(s));
            }
        }
    }

    ParsedPiSettings {
        default_provider,
        default_model,
        default_thinking_level,
        enabled_models,
    }
}

fn parse_cost_object(val: &Value) -> Option<ModelCostInfo> {
    let obj = val.as_object()?;
    let num = |keys: &[&str]| -> Option<f64> {
        for k in keys {
            if let Some(n) = obj.get(*k).and_then(Value::as_f64) {
                return Some(n);
            }
        }
        None
    };
    let input = num(&["input", "prompt"]);
    let output = num(&["output", "completion"]);
    let cache_read = num(&["cacheRead", "cache_read"]);
    let cache_write = num(&["cacheWrite", "cache_write"]);
    if input.is_none() && output.is_none() && cache_read.is_none() && cache_write.is_none() {
        None
    } else {
        Some(ModelCostInfo {
            input,
            output,
            cache_read,
            cache_write,
        })
    }
}

fn parse_single_model_entry(
    entry: &Value,
    fallback_provider: Option<&str>,
) -> Option<ConfiguredModelInfo> {
    let obj = entry.as_object()?;
    let raw_id = obj
        .get("id")
        .or_else(|| obj.get("modelId"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;

    let (id_provider, clean_id) = split_provider_model(raw_id);
    let provider = obj
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or(id_provider)
        .or_else(|| fallback_provider.map(str::to_string))
        .unwrap_or_else(|| "custom".to_string());

    let name = obj
        .get("name")
        .or_else(|| obj.get("label"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| clean_id.clone());

    let context_window = obj
        .get("contextWindow")
        .or_else(|| obj.get("context_window"))
        .or_else(|| obj.get("maxInputTokens"))
        .and_then(Value::as_u64);

    let max_tokens = obj
        .get("maxTokens")
        .or_else(|| obj.get("max_tokens"))
        .or_else(|| obj.get("maxOutputTokens"))
        .and_then(Value::as_u64);

    let reasoning = obj
        .get("reasoning")
        .or_else(|| obj.get("thinking"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    let mut input: Vec<String> = obj
        .get("input")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default();

    let explicit_images = obj
        .get("supportsImages")
        .or_else(|| obj.get("supports_images"))
        .or_else(|| obj.get("vision"))
        .and_then(Value::as_bool)
        .unwrap_or(false);

    if input.is_empty() {
        input.push("text".to_string());
    }
    if explicit_images && !input.iter().any(|i| i == "image") {
        input.push("image".to_string());
    }
    let supports_images = explicit_images || input.iter().any(|i| i == "image");

    let cost = obj.get("cost").and_then(parse_cost_object);

    Some(ConfiguredModelInfo {
        id: clean_id,
        name,
        provider,
        context_window,
        max_tokens,
        reasoning,
        supports_images,
        input,
        cost,
        is_default: false,
        default_thinking_level: None,
    })
}

fn parse_models_json(content: &str, fallback_provider: Option<&str>) -> Vec<ConfiguredModelInfo> {
    let Ok(val) = serde_json::from_str::<Value>(content) else {
        return Vec::new();
    };

    let mut out = Vec::new();

    if let Some(arr) = val.as_array() {
        for item in arr {
            if let Some(m) = parse_single_model_entry(item, fallback_provider) {
                out.push(m);
            }
        }
        return out;
    }

    if let Some(obj) = val.as_object() {
        // Standard Pi `models.json`: `{ "providers": { "<provider>": { "models": [...] } } }`
        if let Some(providers_obj) = obj.get("providers").and_then(Value::as_object) {
            for (prov_name, prov_val) in providers_obj {
                if let Some(models_arr) = prov_val.get("models").and_then(Value::as_array) {
                    for item in models_arr {
                        if let Some(m) = parse_single_model_entry(item, Some(prov_name)) {
                            out.push(m);
                        }
                    }
                } else if let Some(models_arr) = prov_val.as_array() {
                    for item in models_arr {
                        if let Some(m) = parse_single_model_entry(item, Some(prov_name)) {
                            out.push(m);
                        }
                    }
                }
            }
        }

        // Top-level `{ "models": [...] }`
        if let Some(models_arr) = obj.get("models").and_then(Value::as_array) {
            for item in models_arr {
                if let Some(m) = parse_single_model_entry(item, fallback_provider) {
                    out.push(m);
                }
            }
        }
    }

    out
}

fn builtin_catalog() -> Vec<ConfiguredModelInfo> {
    vec![
        ConfiguredModelInfo {
            id: "claude-sonnet-4-20250514".to_string(),
            name: "Claude Sonnet 4".to_string(),
            provider: "anthropic".to_string(),
            context_window: Some(200_000),
            max_tokens: Some(64_000),
            reasoning: true,
            supports_images: true,
            input: vec!["text".to_string(), "image".to_string()],
            cost: Some(ModelCostInfo {
                input: Some(3.0),
                output: Some(15.0),
                cache_read: Some(0.3),
                cache_write: Some(3.75),
            }),
            is_default: false,
            default_thinking_level: None,
        },
        ConfiguredModelInfo {
            id: "claude-opus-4-20250514".to_string(),
            name: "Claude Opus 4".to_string(),
            provider: "anthropic".to_string(),
            context_window: Some(200_000),
            max_tokens: Some(32_000),
            reasoning: true,
            supports_images: true,
            input: vec!["text".to_string(), "image".to_string()],
            cost: Some(ModelCostInfo {
                input: Some(15.0),
                output: Some(75.0),
                cache_read: Some(1.5),
                cache_write: Some(18.75),
            }),
            is_default: false,
            default_thinking_level: None,
        },
        ConfiguredModelInfo {
            id: "claude-3-7-sonnet-20250219".to_string(),
            name: "Claude 3.7 Sonnet".to_string(),
            provider: "anthropic".to_string(),
            context_window: Some(200_000),
            max_tokens: Some(64_000),
            reasoning: true,
            supports_images: true,
            input: vec!["text".to_string(), "image".to_string()],
            cost: Some(ModelCostInfo {
                input: Some(3.0),
                output: Some(15.0),
                cache_read: Some(0.3),
                cache_write: Some(3.75),
            }),
            is_default: false,
            default_thinking_level: None,
        },
        ConfiguredModelInfo {
            id: "gpt-4.1".to_string(),
            name: "GPT-4.1".to_string(),
            provider: "openai".to_string(),
            context_window: Some(1_047_576),
            max_tokens: Some(32_768),
            reasoning: false,
            supports_images: true,
            input: vec!["text".to_string(), "image".to_string()],
            cost: Some(ModelCostInfo {
                input: Some(2.0),
                output: Some(8.0),
                cache_read: Some(0.5),
                cache_write: None,
            }),
            is_default: false,
            default_thinking_level: None,
        },
        ConfiguredModelInfo {
            id: "o3".to_string(),
            name: "OpenAI o3".to_string(),
            provider: "openai".to_string(),
            context_window: Some(200_000),
            max_tokens: Some(100_000),
            reasoning: true,
            supports_images: true,
            input: vec!["text".to_string(), "image".to_string()],
            cost: Some(ModelCostInfo {
                input: Some(2.0),
                output: Some(8.0),
                cache_read: Some(0.5),
                cache_write: None,
            }),
            is_default: false,
            default_thinking_level: None,
        },
        ConfiguredModelInfo {
            id: "o4-mini".to_string(),
            name: "OpenAI o4-mini".to_string(),
            provider: "openai".to_string(),
            context_window: Some(200_000),
            max_tokens: Some(100_000),
            reasoning: true,
            supports_images: true,
            input: vec!["text".to_string(), "image".to_string()],
            cost: Some(ModelCostInfo {
                input: Some(1.1),
                output: Some(4.4),
                cache_read: Some(0.275),
                cache_write: None,
            }),
            is_default: false,
            default_thinking_level: None,
        },
        ConfiguredModelInfo {
            id: "gemini-2.5-pro".to_string(),
            name: "Gemini 2.5 Pro".to_string(),
            provider: "google".to_string(),
            context_window: Some(1_048_576),
            max_tokens: Some(65_536),
            reasoning: true,
            supports_images: true,
            input: vec!["text".to_string(), "image".to_string()],
            cost: Some(ModelCostInfo {
                input: Some(1.25),
                output: Some(10.0),
                cache_read: Some(0.31),
                cache_write: None,
            }),
            is_default: false,
            default_thinking_level: None,
        },
        ConfiguredModelInfo {
            id: "gemini-2.5-flash".to_string(),
            name: "Gemini 2.5 Flash".to_string(),
            provider: "google".to_string(),
            context_window: Some(1_048_576),
            max_tokens: Some(65_536),
            reasoning: true,
            supports_images: true,
            input: vec!["text".to_string(), "image".to_string()],
            cost: Some(ModelCostInfo {
                input: Some(0.3),
                output: Some(2.5),
                cache_read: Some(0.075),
                cache_write: None,
            }),
            is_default: false,
            default_thinking_level: None,
        },
    ]
}

pub fn load_configured_models_from_dir(agent_dir: Option<&Path>) -> Vec<ConfiguredModelInfo> {
    let settings = agent_dir
        .and_then(|dir| std::fs::read_to_string(dir.join("settings.json")).ok())
        .map(|s| parse_settings_json(&s))
        .unwrap_or_default();

    let custom_models = agent_dir
        .and_then(|dir| std::fs::read_to_string(dir.join("models.json")).ok())
        .map(|s| parse_models_json(&s, settings.default_provider.as_deref()))
        .unwrap_or_default();

    let builtins = builtin_catalog();
    let mut merged: Vec<ConfiguredModelInfo> = Vec::new();
    let mut seen: HashSet<(String, String)> = HashSet::new();

    for m in custom_models {
        let key = (m.provider.to_lowercase(), m.id.to_lowercase());
        if seen.insert(key) {
            merged.push(m);
        }
    }

    // Ensure any model referenced in `settings.json` (`defaultModel` or `enabledModels`) is present.
    let mut referenced = settings.enabled_models.clone();
    if let Some(def_model) = &settings.default_model {
        referenced.insert(0, (settings.default_provider.clone(), def_model.clone()));
    }

    for (maybe_prov, model_id) in referenced {
        let prov = maybe_prov
            .or_else(|| settings.default_provider.clone())
            .or_else(|| {
                builtins
                    .iter()
                    .find(|b| b.id.eq_ignore_ascii_case(&model_id))
                    .map(|b| b.provider.clone())
            })
            .unwrap_or_else(|| "anthropic".to_string());
        let key = (prov.to_lowercase(), model_id.to_lowercase());
        if seen.insert(key) {
            if let Some(builtin) = builtins.iter().find(|b| {
                b.provider.eq_ignore_ascii_case(&prov) && b.id.eq_ignore_ascii_case(&model_id)
            }) {
                merged.push(builtin.clone());
            } else {
                merged.push(ConfiguredModelInfo {
                    id: model_id.clone(),
                    name: model_id,
                    provider: prov,
                    context_window: None,
                    max_tokens: None,
                    reasoning: true,
                    supports_images: true,
                    input: vec!["text".to_string(), "image".to_string()],
                    cost: None,
                    is_default: false,
                    default_thinking_level: None,
                });
            }
        }
    }

    for b in builtins {
        let key = (b.provider.to_lowercase(), b.id.to_lowercase());
        if seen.insert(key) {
            merged.push(b);
        }
    }

    // Mark default model & thinking level from settings.json
    let mut marked_default = false;
    if let Some(def_model) = &settings.default_model {
        for m in &mut merged {
            let prov_matches = settings
                .default_provider
                .as_ref()
                .is_none_or(|p| m.provider.eq_ignore_ascii_case(p));
            if prov_matches && m.id.eq_ignore_ascii_case(def_model) {
                m.is_default = true;
                m.default_thinking_level = settings.default_thinking_level.clone();
                marked_default = true;
                break;
            }
        }
    }

    if !marked_default && let Some(first) = merged.first_mut() {
        first.is_default = true;
        first.default_thinking_level = settings.default_thinking_level;
    }

    merged
}

#[tauri::command]
pub async fn list_configured_models() -> Result<Vec<ConfiguredModelInfo>, String> {
    tokio::task::spawn_blocking(|| {
        let agent_dir = resolve_pi_agent_dir();
        Ok(load_configured_models_from_dir(agent_dir.as_deref()))
    })
    .await
    .map_err(|e| format!("Failed to join list_configured_models task: {e}"))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_dir(prefix: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pipan-models-test-{prefix}-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp test dir");
        dir
    }

    #[test]
    fn test_loads_custom_models_and_settings_default() {
        let dir = make_temp_dir("custom");
        std::fs::write(
            dir.join("models.json"),
            r#"{
                "providers": {
                    "openrouter": {
                        "models": [
                            {
                                "id": "deepseek-r1",
                                "name": "DeepSeek R1",
                                "contextWindow": 163840,
                                "maxTokens": 32768,
                                "reasoning": true,
                                "input": ["text"],
                                "cost": { "input": 0.55, "output": 2.19 }
                            }
                        ]
                    }
                }
            }"#,
        )
        .unwrap();

        std::fs::write(
            dir.join("settings.json"),
            r#"{
                "defaultProvider": "openrouter",
                "defaultModel": "deepseek-r1",
                "defaultThinkingLevel": "high"
            }"#,
        )
        .unwrap();

        let models = load_configured_models_from_dir(Some(&dir));
        let custom = models
            .iter()
            .find(|m| m.provider == "openrouter" && m.id == "deepseek-r1")
            .expect("custom model loaded");

        assert_eq!(custom.name, "DeepSeek R1");
        assert_eq!(custom.context_window, Some(163_840));
        assert!(custom.reasoning);
        assert!(!custom.supports_images);
        assert!(custom.is_default);
        assert_eq!(custom.default_thinking_level.as_deref(), Some("high"));
        assert_eq!(custom.cost.as_ref().and_then(|c| c.input), Some(0.55));
        assert_eq!(custom.cost.as_ref().and_then(|c| c.output), Some(2.19));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_fallback_when_agent_dir_missing() {
        let missing = PathBuf::from("/nonexistent/pipan/test/path");
        let models = load_configured_models_from_dir(Some(&missing));
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.is_default));
    }
}
