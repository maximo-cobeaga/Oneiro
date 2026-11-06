//! Voice settings shared by every project (`%APPDATA%/oneiro/voice.conf`). API keys are never
//! written to disk: the config stores only the *name* of the environment variable that holds them.

use std::path::PathBuf;

/// Which engine produces a voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProviderKind {
    /// Recorded or imported audio only (no synthesis).
    File,
    /// Built-in formant babble. Instant and offline; for previews only.
    Demo,
    /// Windows built-in speech (System.Speech). Offline, no setup.
    Sapi,
    /// ElevenLabs cloud voices (API key).
    ElevenLabs,
    /// Any OpenAI-compatible `/audio/speech` server: OpenAI, Kokoro-FastAPI, openedai-speech...
    OpenAi,
    /// Your own command (Piper, espeak-ng, Coqui, a script...).
    Command,
}

impl ProviderKind {
    pub const ALL: [ProviderKind; 6] =
        [ProviderKind::File, ProviderKind::Demo, ProviderKind::Sapi, ProviderKind::ElevenLabs, ProviderKind::OpenAi, ProviderKind::Command];

    pub fn id(self) -> &'static str {
        match self {
            ProviderKind::File => "file",
            ProviderKind::Demo => "demo",
            ProviderKind::Sapi => "sapi",
            ProviderKind::ElevenLabs => "elevenlabs",
            ProviderKind::OpenAi => "openai",
            ProviderKind::Command => "command",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ProviderKind::File => "Recorded / imported files",
            ProviderKind::Demo => "Demo babble (preview only)",
            ProviderKind::Sapi => "Windows speech (offline)",
            ProviderKind::ElevenLabs => "ElevenLabs (API key)",
            ProviderKind::OpenAi => "OpenAI-compatible TTS / local server",
            ProviderKind::Command => "Custom command (Piper, espeak...)",
        }
    }

    pub fn from_id(s: &str) -> Option<ProviderKind> {
        let s = s.trim().to_lowercase();
        ProviderKind::ALL.into_iter().find(|k| k.id() == s || (s == "eleven" && *k == ProviderKind::ElevenLabs) || (s == "local" && *k == ProviderKind::Command))
    }

    /// Whether this provider can synthesize (everything except `file`).
    pub fn synthesizes(self) -> bool {
        self != ProviderKind::File
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoiceConfig {
    /// Used for characters whose voice provider is `file`, and as the IDE default.
    pub default_provider: ProviderKind,
    pub elevenlabs_key_env: String,
    pub elevenlabs_model: String,
    pub openai_base_url: String,
    pub openai_model: String,
    pub openai_voice: String,
    pub openai_key_env: String,
    /// Template with `{text}`, `{out}`, `{voice}`, `{speed}`. Without `{text}` the text goes to stdin.
    pub command: String,
    pub sapi_voice: String,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        VoiceConfig {
            default_provider: if cfg!(windows) { ProviderKind::Sapi } else { ProviderKind::Demo },
            elevenlabs_key_env: "ELEVENLABS_API_KEY".into(),
            elevenlabs_model: "eleven_multilingual_v2".into(),
            openai_base_url: "http://localhost:8880/v1".into(),
            openai_model: "tts-1".into(),
            openai_voice: "alloy".into(),
            openai_key_env: String::new(),
            command: String::new(),
            sapi_voice: String::new(),
        }
    }
}

pub fn config_path() -> PathBuf {
    let base = std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(std::env::temp_dir);
    base.join("oneiro").join("voice.conf")
}

impl VoiceConfig {
    pub fn parse(text: &str) -> VoiceConfig {
        let mut c = VoiceConfig::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim().to_string());
            match k {
                "default_provider" => {
                    if let Some(p) = ProviderKind::from_id(&v) {
                        c.default_provider = p;
                    }
                }
                "elevenlabs_key_env" => c.elevenlabs_key_env = v,
                "elevenlabs_model" => c.elevenlabs_model = v,
                "openai_base_url" => c.openai_base_url = v,
                "openai_model" => c.openai_model = v,
                "openai_voice" => c.openai_voice = v,
                "openai_key_env" => c.openai_key_env = v,
                "command" => c.command = v,
                "sapi_voice" => c.sapi_voice = v,
                _ => {}
            }
        }
        c
    }

    pub fn to_text(&self) -> String {
        format!(
            "# Oneiro voice settings. API keys are never stored here: *_key_env is only the NAME of an environment variable.\ndefault_provider = {}\nelevenlabs_key_env = {}\nelevenlabs_model = {}\nopenai_base_url = {}\nopenai_model = {}\nopenai_voice = {}\nopenai_key_env = {}\ncommand = {}\nsapi_voice = {}\n",
            self.default_provider.id(),
            self.elevenlabs_key_env,
            self.elevenlabs_model,
            self.openai_base_url,
            self.openai_model,
            self.openai_voice,
            self.openai_key_env,
            self.command,
            self.sapi_voice
        )
    }

    pub fn load() -> VoiceConfig {
        std::fs::read_to_string(config_path()).map(|t| VoiceConfig::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let p = config_path();
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
        }
        std::fs::write(&p, self.to_text()).map_err(|e| format!("{}: {}", p.display(), e))
    }

    /// Name of the environment variable that holds the key for `kind` (empty when none is needed).
    pub fn key_env(&self, kind: ProviderKind) -> &str {
        match kind {
            ProviderKind::ElevenLabs => &self.elevenlabs_key_env,
            ProviderKind::OpenAi => &self.openai_key_env,
            _ => "",
        }
    }
}
