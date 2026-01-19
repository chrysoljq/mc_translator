use crate::log_warn;
use anyhow::{Result, anyhow};
use reqwest::{Client, RequestBuilder, Response, StatusCode};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::select;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use crate::config::AppConfig;

#[derive(Clone)]
pub struct OpenAIClient {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
    prompt: String,
    max_retries: u32,
    retry_delay: u64,
}

impl OpenAIClient {
    pub fn new(config: AppConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .build()
            .unwrap_or_default();

        Self {
            client,
            api_key: config.api_key,
            base_url: config.base_url.trim_end_matches('/').to_string(),
            model: config.model,
            prompt: config.prompt,
            max_retries: config.max_retries,
            retry_delay: config.retry_delay,
        }
    }

    async fn send_with_retry(
        &self,
        builder_fn: impl Fn() -> RequestBuilder,
        token: &CancellationToken,
    ) -> Result<Response> {
        let mut attempt = 0;

        loop {
            if token.is_cancelled() {
                return Err(anyhow!("任务已被用户取消"));
            }
            let request = builder_fn();

            let result = select! {
                res = request.send() => res,
                _ = token.cancelled() => {
                    return Err(anyhow!("任务被用户取消"));
                }
            };

            match result {
                Ok(resp) => {
                    let status = resp.status();

                    if status.is_success() {
                        return Ok(resp);
                    }

                    if status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST {
                        let text = resp.text().await.unwrap_or_default();
                        return Err(anyhow!("API 错误 (HTTP {}): {}", status, text));
                    }

                    if attempt >= self.max_retries {
                        let text = resp.text().await.unwrap_or_default();
                        return Err(anyhow!("重试耗尽 (HTTP {}): {}", status, text));
                    }

                    let wait_time = if status == StatusCode::TOO_MANY_REQUESTS {
                        if let Some(retry_after) = resp.headers().get("Retry-After") {
                            retry_after
                                .to_str()
                                .ok()
                                .and_then(|s| s.parse::<u64>().ok())
                                .map(Duration::from_secs)
                                .unwrap_or(Duration::from_secs(self.retry_delay * 2_u64.pow(attempt))) // 解析失败则回退
                        } else {
                            Duration::from_secs(self.retry_delay * 2_u64.pow(attempt)) // 指数回退
                        }
                    } else if status.is_server_error() {
                        Duration::from_secs(self.retry_delay)
                    } else {
                        let text = resp.text().await.unwrap_or_default();
                        return Err(anyhow!("请求失败 (HTTP {}): {}", status, text));
                    };

                    log_warn!(
                        "请求遇到 {}, 等待 {:?} 后重试 (第 {}/{} 次)...",
                        status,
                        wait_time,
                        attempt + 1,
                        self.max_retries
                    );
                    sleep(wait_time).await;
                }
                Err(e) => {
                    if attempt >= self.max_retries {
                        return Err(anyhow!("网络重试耗尽: {}", e));
                    }

                    let wait_time = Duration::from_secs(2_u64.pow(attempt));
                    log_warn!(
                        "网络错误: {}, 等待 {:?} 后重试 (第 {}/{} 次)...",
                        e,
                        wait_time,
                        attempt + 1,
                        self.max_retries
                    );
                    sleep(wait_time).await;
                }
            }

            attempt += 1;
        }
    }

    pub async fn fetch_models(&self, token: &CancellationToken) -> Result<Vec<String>> {
        let url = format!("{}/models", self.base_url);

        let resp = self
            .send_with_retry(
                || {
                    self.client
                        .get(&url)
                        .header("Authorization", format!("Bearer {}", self.api_key))
                },
                token,
            )
            .await?;

        let json: Value = resp.json().await?;
        let mut models = Vec::new();
        if let Some(data) = json["data"].as_array() {
            for item in data {
                if let Some(id) = item["id"].as_str() {
                    models.push(id.to_string());
                }
            }
        }
        models.sort();
        Ok(models)
    }
    /*
    pub async fn translate_batch(
        &self,
        data: Map<String, Value>,
        mod_id: &str,
        token: &CancellationToken,
    ) -> Result<Map<String, Value>> {
        let system_prompt = format!(
            "你是一个《我的世界》(Minecraft) 模组本地化专家。当前模组 ID: 【{}】。\n\
            请将传入的 JSON Value (英文) 翻译为简体中文，Key 必须保持不变。\n\
            请严格保留格式代码（如 §a, %s, {{0}} 等）。\n\
            只返回纯净的 JSON 字符串，不要包含 Markdown 代码块标记。",
            mod_id
        );

        let request_body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": serde_json::to_string(&data)?}
            ],
            "temperature": 0.1
        });

        let resp = self
            .send_with_retry(
                || {
                    self.client
                        .post(format!("{}/chat/completions", self.base_url))
                        .header("Authorization", format!("Bearer {}", self.api_key))
                        .header("Content-Type", "application/json")
                        .json(&request_body) // reqwest 会自动处理 json 的克隆
                },
                token,
            )
            .await?;

        let resp_json: Value = resp.json().await?;
        let content = resp_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or(anyhow!("API 返回内容为空"))?;

        let clean_content = self.clean_json_string(content);
        let parsed: Map<String, Value> = serde_json::from_str(&clean_content)?;
        Ok(parsed)
    }
    */
    pub async fn translate_text_list(
        &self,
        texts: Vec<String>,
        mod_id: &str,
        token: &CancellationToken,
    ) -> Result<Vec<String>> {
        let system_prompt = self.prompt.replace("{MOD_ID}", &mod_id);

        let request_body = json!({
            "model": self.model,
            "messages": [
                {"role": "system", "content": system_prompt},
                {"role": "user", "content": serde_json::to_string(&texts)?}
            ],
            "temperature": 0.1
        });

        let resp = self
            .send_with_retry(
                || {
                    self.client
                        .post(format!("{}/chat/completions", self.base_url))
                        .header("Authorization", format!("Bearer {}", self.api_key))
                        .header("Content-Type", "application/json")
                        .json(&request_body)
                },
                token,
            )
            .await?;

        let resp_json: Value = resp.json().await?;
        let content = resp_json["choices"][0]["message"]["content"]
            .as_str()
            .ok_or(anyhow!("API 返回内容为空"))?;

        let clean_content = self.clean_json_string(content);
        let parsed: Vec<String> = serde_json::from_str(&clean_content)?;
        Ok(parsed)
    }

    fn clean_json_string(&self, s: &str) -> String {
        s.trim()
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
            .to_string()
    }
}
