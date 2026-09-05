#![allow(clippy::unwrap_used)]

use super::e2e_harness::spawn_test_gateway;
use async_trait::async_trait;
use auth::AuthContext;
use cli::client::GatewayClient;
use cli::local::LocalEngine;
use common::pagination::Page;
use errors::ContextraError;
use gateway::{
    ChatExecutionRequest, ChatExecutionResponse, CollectionResource, CreateCollectionRequest,
    CreateConversationRequest, CreateDocumentRequest, DocumentFilter, DocumentResource,
    GatewayService, MessageResource, Pagination,
};
use std::io::Write;
use std::sync::{Arc, Mutex};
use tempfile::NamedTempFile;
use types::{CollectionId, ConversationId, DocumentId};
use uuid::Uuid;

#[derive(Clone, Default)]
struct MockE2eGatewayService {
    collections: Arc<Mutex<Vec<CollectionResource>>>,
    documents: Arc<Mutex<Vec<DocumentResource>>>,
}

#[async_trait]
impl GatewayService for MockE2eGatewayService {
    async fn authenticate(&self, _token: &str) -> Result<AuthContext, ContextraError> {
        Ok(AuthContext::new(
            types::UserId::new(),
            types::OrgId::new(),
            vec!["*:*".into()],
        ))
    }

    async fn list_documents(
        &self,
        _pagination: Pagination,
        _filter: DocumentFilter,
    ) -> Result<Page<DocumentResource>, ContextraError> {
        let docs = self.documents.lock().unwrap().clone();
        Ok(Page::new(docs, None, false, None))
    }

    async fn get_document(&self, id: DocumentId) -> Result<DocumentResource, ContextraError> {
        self.documents
            .lock()
            .unwrap()
            .iter()
            .find(|d| d.id == id.to_string())
            .cloned()
            .ok_or_else(|| ContextraError::NotFound(id.to_string()))
    }

    async fn create_document(
        &self,
        request: CreateDocumentRequest,
    ) -> Result<DocumentResource, ContextraError> {
        let doc = DocumentResource {
            id: Uuid::now_v7().to_string(),
            collection_id: Uuid::now_v7().to_string(),
            content: format!("Content from {}", request.source_path),
            metadata: types::Metadata::new(),
        };
        self.documents.lock().unwrap().push(doc.clone());
        Ok(doc)
    }

    async fn list_collections(
        &self,
        _pagination: Pagination,
    ) -> Result<Page<CollectionResource>, ContextraError> {
        let cols = self.collections.lock().unwrap().clone();
        Ok(Page::new(cols, None, false, None))
    }

    async fn get_collection(&self, id: CollectionId) -> Result<CollectionResource, ContextraError> {
        self.collections
            .lock()
            .unwrap()
            .iter()
            .find(|c| c.id == id.to_string())
            .cloned()
            .ok_or_else(|| ContextraError::NotFound(id.to_string()))
    }

    async fn create_collection(
        &self,
        request: CreateCollectionRequest,
    ) -> Result<CollectionResource, ContextraError> {
        let col = CollectionResource {
            id: Uuid::now_v7().to_string(),
            name: request.name,
            metadata: types::Metadata::new(),
        };
        self.collections.lock().unwrap().push(col.clone());
        Ok(col)
    }

    async fn list_conversations(
        &self,
        _pagination: Pagination,
    ) -> Result<Page<gateway::ConversationResource>, ContextraError> {
        Ok(Page::new(vec![], None, false, None))
    }

    async fn create_conversation(
        &self,
        request: CreateConversationRequest,
    ) -> Result<gateway::ConversationResource, ContextraError> {
        Ok(gateway::ConversationResource {
            id: Uuid::now_v7().to_string(),
            title: request.title,
            metadata: types::Metadata::new(),
        })
    }

    async fn list_messages(
        &self,
        _conversation_id: ConversationId,
        _pagination: Pagination,
    ) -> Result<Page<MessageResource>, ContextraError> {
        Ok(Page::new(vec![], None, false, None))
    }

    async fn execute_chat(
        &self,
        _conversation_id: ConversationId,
        request: ChatExecutionRequest,
    ) -> Result<ChatExecutionResponse, ContextraError> {
        Ok(ChatExecutionResponse {
            id: Uuid::now_v7().to_string(),
            model: "gpt-4.1-mini".into(),
            message: format!("Processed message: {}", request.message),
            finish_reason: Some("stop".into()),
        })
    }
}

#[tokio::test]
async fn test_cli_local_ingest_and_chat_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let mut tmp = NamedTempFile::new()?;
    writeln!(
        tmp,
        "# Test Context Engineering Document\nThis document describes Contextra's architecture."
    )?;

    let engine = LocalEngine::new();
    let ingest_res = engine.ingest(tmp.path().to_str().unwrap()).await?;
    assert!(ingest_res.contains("Document ID:"));
    assert!(ingest_res.contains("Chunks ingested:"));

    let chat_res = engine.chat("Explain Contextra's architecture").await?;
    assert!(chat_res.contains("Contextra Local Engine response"));

    Ok(())
}

#[tokio::test]
async fn test_cli_local_eval_run() -> Result<(), Box<dyn std::error::Error>> {
    let engine = LocalEngine::new();
    let report = engine.run_eval(None, 3).await?;
    assert!(report.contains("--- Evaluation Report ---"));
    assert!(report.contains("Retrieval Precision@3:"));
    assert!(report.contains("Generation Overall Score:"));

    Ok(())
}

#[tokio::test]
async fn test_gateway_rest_ingest_and_chat_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    let service = Arc::new(MockE2eGatewayService::default());
    let base_url = spawn_test_gateway(service).await;
    let client = GatewayClient::new(base_url, Some("test-token".to_string()));

    // 1. Ingest document via REST
    let doc = client.ingest_document("/tmp/sample_doc.txt").await?;
    assert!(!doc.id.is_empty());
    assert!(doc.content.contains("/tmp/sample_doc.txt"));

    // 2. Create conversation via REST
    let conv = client
        .create_conversation(Some("E2E Test Session".into()))
        .await?;
    assert!(!conv.id.is_empty());

    // 3. Execute chat via REST
    let chat_resp = client.chat(&conv.id, "Hello Contextra").await?;
    assert_eq!(chat_resp.model, "gpt-4.1-mini");
    assert!(
        chat_resp
            .message
            .contains("Processed message: Hello Contextra")
    );

    Ok(())
}
