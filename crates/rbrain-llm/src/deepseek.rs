use rbrain_core::config::DeepSeekConfig;
use rbrain_core::error::{BrainError, Result};
use serde::{Deserialize, Serialize};
use std::env;
use std::time::Duration;

const DEFAULT_BASE_URL: &str = "https://api.deepseek.com/v1";
const DEFAULT_MODEL: &str = "deepseek-chat";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Intent {
    Entity,
    Temporal,
    Event,
    General,
}

impl std::str::FromStr for Intent {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "entity" => Ok(Intent::Entity),
            "temporal" => Ok(Intent::Temporal),
            "event" => Ok(Intent::Event),
            "general" => Ok(Intent::General),
            _ => Ok(Intent::General),
        }
    }
}

impl std::fmt::Display for Intent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Intent::Entity => write!(f, "entity"),
            Intent::Temporal => write!(f, "temporal"),
            Intent::Event => write!(f, "event"),
            Intent::General => write!(f, "general"),
        }
    }
}

#[derive(Debug, Serialize)]
struct ChatCompletionRequest {
    model: String,
    messages: Vec<Message>,
    temperature: f32,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize)]
struct Choice {
    message: MessageContent,
}

#[derive(Debug, Deserialize)]
struct MessageContent {
    content: String,
}

#[derive(Debug)]
pub struct DeepSeekClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl DeepSeekClient {
    pub fn from_config(cfg: &DeepSeekConfig) -> Result<Self> {
        let api_key = if !cfg.api_key.is_empty() {
            cfg.api_key.clone()
        } else {
            env::var("DEEPSEEK_API_KEY").map_err(|_| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: "DEEPSEEK_API_KEY not set and config.deepseek.api_key is empty".to_string(),
            })?
        };

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: e.to_string(),
            })?;

        Ok(Self {
            client,
            api_key,
            base_url: cfg.base_url.clone(),
            model: cfg.model.clone(),
        })
    }

    pub fn new(api_key: Option<&str>) -> Result<Self> {
        let api_key = api_key.map(String::from)
            .or_else(|| env::var("DEEPSEEK_API_KEY").ok())
            .ok_or_else(|| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: "DEEPSEEK_API_KEY not set".to_string(),
            })?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: e.to_string(),
            })?;

        Ok(Self {
            client,
            api_key,
            base_url: DEFAULT_BASE_URL.to_string(),
            model: DEFAULT_MODEL.to_string(),
        })
    }

    pub fn with_base_url(mut self, url: &str) -> Self {
        self.base_url = url.to_string();
        self
    }

    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// General chat completion: system prompt + user message → assistant response.
    pub async fn chat(&self, system: &str, user: &str) -> Result<String> {
        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![
                Message { role: "system".to_string(), content: system.to_string() },
                Message { role: "user".to_string(), content: user.to_string() },
            ],
            temperature: 0.3,
        };

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: format!("HTTP {}: {}", status, body),
            });
        }

        let completion: ChatCompletionResponse = response
            .json()
            .await
            .map_err(|e| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: format!("Failed to parse response: {}", e),
            })?;

        completion
            .choices
            .into_iter()
            .next()
            .map(|c| c.message.content)
            .ok_or_else(|| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: "Empty response from API".to_string(),
            })
    }

    pub async fn expand_query(&self, query: &str, n: usize) -> Result<Vec<String>> {
        let prompt = format!(
            "Rewrite the following query into {} different variations in the SAME language. \
             Each variation should capture the same intent but use different words or phrasing. \
             Return ONLY a JSON array of strings, no other text.\n\nQuery: {}",
            n, query
        );

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.7,
        };

        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: format!("HTTP {}: {}", status, body),
            });
        }

        let completion: ChatCompletionResponse = response.json().await
            .map_err(|e| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: format!("Failed to parse response: {}", e),
            })?;

        if completion.choices.is_empty() {
            return Err(BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: "Empty response from API".to_string(),
            });
        }

        let content = &completion.choices[0].message.content;

        let expansions: Vec<String> = serde_json::from_str(content.trim())
            .map_err(|e| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: format!("Failed to parse query expansions: {}", e),
            })?;

        Ok(expansions)
    }

    pub async fn classify_intent(&self, query: &str) -> Result<Intent> {
        let prompt = format!(
            "Classify the following query into one of: entity, temporal, event, general. \
             Return ONLY the category name.\n\nQuery: {}",
            query
        );

        let request = ChatCompletionRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "user".to_string(),
                content: prompt,
            }],
            temperature: 0.1,
        };

        let response = self.client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&request)
            .send()
            .await
            .map_err(|e| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: e.to_string(),
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: format!("HTTP {}: {}", status, body),
            });
        }

        let completion: ChatCompletionResponse = response.json().await
            .map_err(|e| BrainError::ApiUnreachable {
                provider: "deepseek".to_string(),
                message: format!("Failed to parse response: {}", e),
            })?;

        if completion.choices.is_empty() {
            return Ok(Intent::General);
        }

        let content = completion.choices[0].message.content.trim().to_lowercase();

        let intent = content.parse::<Intent>()
            .unwrap_or(Intent::General);

        Ok(intent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_from_str() {
        assert!(matches!("entity".parse::<Intent>().unwrap(), Intent::Entity));
        assert!(matches!("temporal".parse::<Intent>().unwrap(), Intent::Temporal));
        assert!(matches!("event".parse::<Intent>().unwrap(), Intent::Event));
        assert!(matches!("general".parse::<Intent>().unwrap(), Intent::General));
        assert!(matches!("unknown".parse::<Intent>().unwrap(), Intent::General));
    }

    #[test]
    fn test_intent_display() {
        assert_eq!(Intent::Entity.to_string(), "entity");
        assert_eq!(Intent::Temporal.to_string(), "temporal");
        assert_eq!(Intent::Event.to_string(), "event");
        assert_eq!(Intent::General.to_string(), "general");
    }
}
