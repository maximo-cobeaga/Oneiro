//! AI settings: which provider to use and how to reach it. API keys are never written to disk:
//! the config stores only the *name* of the environment variable that holds the key.

use crate::providers::{anthropic::AnthropicApi, cli::AgentCli, openai::OpenAiCompatible};
use crate::types::Provider;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProviderKind {
    ClaudeCli,
    CodexCli,
    AnthropicApi,
    OpenAiCompatible,
    CustomCli,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 5] =
        [ProviderKind::ClaudeCli, ProviderKind::CodexCli, ProviderKind::AnthropicApi, ProviderKind::OpenAiCompatible, ProviderKind::CustomCli];

    pub fn id(self) -> &'static str {
        match self {
            ProviderKind::ClaudeCli => "claude-cli",
            ProviderKind::CodexCli => "codex-cli",
            ProviderKind::AnthropicApi => "anthropic",
            ProviderKind::OpenAiCompatible => "openai",
            ProviderKind::CustomCli => "custom-cli",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::ClaudeCli => "Claude Code CLI (your Claude login)",
            ProviderKind::CodexCli => "Codex CLI (your ChatGPT/Codex login)",
            ProviderKind::AnthropicApi => "Anthropic API (API key)",
            ProviderKind::OpenAiCompatible => "OpenAI-compatible / local (Ollama, LM Studio...)",
            ProviderKind::CustomCli => "Custom command (stdin -> stdout)",
        }
    }

    pub fn from_id(s: &str) -> Option<ProviderKind> {
        ProviderKind::ALL.into_iter().find(|k| k.id() == s)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AiConfig {
    pub kind: ProviderKind,
    pub model: String,
    pub base_url: String,
    /// Name of the environment variable holding the API key (the key itself is never stored).
    pub api_key_env: String,
    pub custom_command: String,
    pub vision: bool,
    pub self_check: bool,
    pub max_rounds: usize,
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            kind: ProviderKind::ClaudeCli,
            model: String::new(),
            base_url: "http://localhost:11434/v1".into(),
            api_key_env: String::new(),
            custom_command: String::new(),
            vision: false,
            self_check: false,
            max_rounds: 4,
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("oneiro").join("ai.conf")
}

impl AiConfig {
    pub fn parse(text: &str) -> AiConfig {
        let mut c = AiConfig::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim().to_string());
            match k {
                "provider" => {
                    if let Some(kind) = ProviderKind::from_id(&v) {
                        c.kind = kind;
                    }
                }
                "model" => c.model = v,
                "base_url" => c.base_url = v,
                "api_key_env" => c.api_key_env = v,
                "custom_command" => c.custom_command = v,
                "vision" => c.vision = v == "true",
                "self_check" => c.self_check = v == "true",
                "max_rounds" => c.max_rounds = v.parse().unwrap_or(4).clamp(1, 10),
                _ => {}
            }
        }
        c
    }

    pub fn to_text(&self) -> String {
        format!(
            "# Oneiro AI settings. API keys are never stored here: api_key_env is only the NAME of an environment variable.\nprovider = {}\nmodel = {}\nbase_url = {}\napi_key_env = {}\ncustom_command = {}\nvision = {}\nself_check = {}\nmax_rounds = {}\n",
            self.kind.id(),
            self.model,
            self.base_url,
            self.api_key_env,
            self.custom_command,
            self.vision,
            self.self_check,
            self.max_rounds
        )
    }

    pub fn load() -> AiConfig {
        std::fs::read_to_string(config_path()).map(|t| AiConfig::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let p = config_path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, self.to_text()).map_err(|e| format!("{}: {}", p.display(), e))
    }

    /// Whether this provider needs a key and where it would come from.
    pub fn key_env_name(&self) -> String {
        if !self.api_key_env.is_empty() {
            self.api_key_env.clone()
        } else if self.kind == ProviderKind::AnthropicApi {
            "ANTHROPIC_API_KEY".into()
        } else {
            String::new()
        }
    }

    /// Builds the provider. `session_key` (typed in the IDE) overrides the environment variable.
    pub fn build_provider(&self, session_key: Option<&str>) -> Result<Box<dyn Provider>, String> {
        let env_key = {
            let n = self.key_env_name();
            if n.is_empty() {
                None
            } else {
                std::env::var(&n).ok().filter(|k| !k.is_empty())
            }
        };
        let key = session_key.filter(|k| !k.is_empty()).map(|k| k.to_string()).or(env_key);
        Ok(match self.kind {
            ProviderKind::ClaudeCli => Box::new(AgentCli::claude(Some(self.model.clone()))),
            ProviderKind::CodexCli => Box::new(AgentCli::codex(Some(self.model.clone()))),
            ProviderKind::AnthropicApi => {
                let key = key.ok_or_else(|| format!("no API key: set the {} environment variable or paste a key for this session", self.key_env_name()))?;
                Box::new(AnthropicApi::new(key, Some(self.model.clone()), Some(self.base_url.clone()).filter(|b| !b.contains("11434"))))
            }
            ProviderKind::OpenAiCompatible => {
                if self.base_url.is_empty() {
                    return Err("set the base URL of the OpenAI-compatible server (for Ollama: http://localhost:11434/v1)".into());
                }
                Box::new(OpenAiCompatible::new(&self.base_url, key, &self.model, self.vision))
            }
            ProviderKind::CustomCli => {
                let mut parts = self.custom_command.split_whitespace().map(|s| s.to_string());
                let program = parts.next().ok_or("write the command to run in the custom command field")?;
                Box::new(AgentCli::custom(&program, parts.collect()))
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trips_and_never_contains_keys() {
        let c = AiConfig { kind: ProviderKind::OpenAiCompatible, model: "llama3.1".into(), api_key_env: "MY_KEY".into(), vision: true, max_rounds: 6, ..Default::default() };
        let t = c.to_text();
        assert!(t.contains("api_key_env = MY_KEY") && !t.to_lowercase().contains("sk-"));
        assert_eq!(AiConfig::parse(&t), c);
    }

    #[test]
    fn providers_build_and_report_missing_keys() {
        let mut c = AiConfig { kind: ProviderKind::AnthropicApi, api_key_env: "ONEIRO_TEST_KEY_THAT_IS_NOT_SET".into(), ..Default::default() };
        assert!(c.build_provider(None).is_err());
        assert!(c.build_provider(Some("k")).is_ok());
        c.kind = ProviderKind::ClaudeCli;
        assert!(c.build_provider(None).unwrap().name().contains("Claude"));
        c.kind = ProviderKind::CustomCli;
        assert!(c.build_provider(None).is_err());
        c.custom_command = "my-agent --flag".into();
        assert!(c.build_provider(None).is_ok());
    }
}
