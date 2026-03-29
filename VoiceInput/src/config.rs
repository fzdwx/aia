use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub language: String,
    pub llm_enabled: bool,
    pub llm_api_base: String,
    pub llm_api_key: String,
    pub llm_model: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            language: "zh-CN".to_string(),
            llm_enabled: false,
            llm_api_base: "https://api.openai.com/v1".to_string(),
            llm_api_key: String::new(),
            llm_model: "gpt-4o-mini".to_string(),
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let config = confy::load("voice-input", "config")?;
        Ok(config)
    }

    pub fn save(&self) -> Result<()> {
        confy::store("voice-input", "config", self)?;
        Ok(())
    }

    pub fn available_languages() -> Vec<LanguageInfo> {
        vec![
            LanguageInfo {
                code: "zh-CN",
                name: "简体中文",
            },
            LanguageInfo {
                code: "zh-TW",
                name: "繁體中文",
            },
            LanguageInfo {
                code: "en",
                name: "English",
            },
            LanguageInfo {
                code: "ja",
                name: "日本語",
            },
            LanguageInfo {
                code: "ko",
                name: "한국어",
            },
        ]
    }
}

pub struct LanguageInfo {
    pub code: &'static str,
    pub name: &'static str,
}
