use reqwest::Client as HttpClient;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use types::Metadata;

#[derive(Debug, Clone)]
pub struct GatewayClient {
    base_url: String,
    auth_token: Option<String>,
    client: HttpClient,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionResource {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentResource {
    pub id: String,
    pub collection_id: String,
    pub content: String,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationResource {
    pub id: String,
    pub title: Option<String>,
    #[serde(default)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatExecutionResponse {
    pub id: String,
    pub model: String,
    pub message: String,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageResponse<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

impl GatewayClient {
    pub fn new(base_url: impl Into<String>, auth_token: Option<String>) -> Self {
        let client = HttpClient::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            auth_token,
            client,
        }
    }

    fn apply_auth(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        if let Some(token) = &self.auth_token {
            req.header("Authorization", format!("Bearer {token}"))
        } else {
            req
        }
    }

    pub async fn list_collections(&self) -> Result<Vec<CollectionResource>, String> {
        let url = format!("{}/api/v1/collections", self.base_url);
        let req = self.apply_auth(self.client.get(&url));

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Failed to connect to Gateway at '{url}': {e}"))?;

        if !resp.status().is_success() {
            return Err(format!(
                "Gateway error ({}): {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            ));
        }

        let page: PageResponse<CollectionResource> = resp
            .json()
            .await
            .map_err(|e| format!("Failed to parse collection list response: {e}"))?;

        Ok(page.items)
    }

    pub async fn ingest_document(&self, source_path: &str) -> Result<DocumentResource, String> {
        let url = format!("{}/api/v1/documents", self.base_url);
        let body = serde_json::json!({ "source_path": source_path });
        let req = self.apply_auth(self.client.post(&url)).json(&body);

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Failed to connect to Gateway at '{url}': {e}"))?;

        if !resp.status().is_success() {
            return Err(format!(
                "Gateway error ({}): {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            ));
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse document ingestion response: {e}"))
    }

    pub async fn create_conversation(
        &self,
        title: Option<String>,
    ) -> Result<ConversationResource, String> {
        let url = format!("{}/api/v1/conversations", self.base_url);
        let body = serde_json::json!({ "title": title });
        let req = self.apply_auth(self.client.post(&url)).json(&body);

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Failed to connect to Gateway at '{url}': {e}"))?;

        if !resp.status().is_success() {
            return Err(format!(
                "Gateway error ({}): {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            ));
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse conversation response: {e}"))
    }

    pub async fn chat(
        &self,
        conversation_id: &str,
        message: &str,
    ) -> Result<ChatExecutionResponse, String> {
        let url = format!(
            "{}/api/v1/conversations/{}/messages",
            self.base_url, conversation_id
        );
        let body = serde_json::json!({ "message": message });
        let req = self.apply_auth(self.client.post(&url)).json(&body);

        let resp = req
            .send()
            .await
            .map_err(|e| format!("Failed to connect to Gateway at '{url}': {e}"))?;

        if !resp.status().is_success() {
            return Err(format!(
                "Gateway error ({}): {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            ));
        }

        resp.json()
            .await
            .map_err(|e| format!("Failed to parse chat response: {e}"))
    }
}
