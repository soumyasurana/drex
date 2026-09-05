use async_trait::async_trait;
use auth::AuthContext;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use common::pagination::Page;
use errors::ContextraError;
use gateway::{
    AppState, ChatExecutionRequest, ChatExecutionResponse, CollectionResource,
    ConversationResource, CreateCollectionRequest, CreateConversationRequest,
    CreateDocumentRequest, DocumentFilter, DocumentResource, GatewayService, MessageResource,
    Pagination, build_router,
};
use std::sync::Arc;
use tower::ServiceExt;
use types::{CollectionId, ConversationId, DocumentId, OrgId, UserId};

struct TestMockGatewayService {
    valid_api_key: String,
}

#[async_trait]
impl GatewayService for TestMockGatewayService {
    async fn authenticate(&self, token: &str) -> Result<AuthContext, ContextraError> {
        if token == self.valid_api_key {
            Ok(AuthContext::new(UserId::new(), OrgId::new(), vec![]))
        } else {
            Err(ContextraError::Unauthorized("Invalid API key".to_string()))
        }
    }

    async fn list_documents(
        &self,
        _pagination: Pagination,
        _filter: DocumentFilter,
    ) -> Result<Page<DocumentResource>, ContextraError> {
        let doc = DocumentResource {
            id: DocumentId::new().to_string(),
            collection_id: CollectionId::new().to_string(),
            content: "test doc content".to_string(),
            metadata: Default::default(),
        };
        Ok(Page::new(vec![doc], None, false, Some(1)))
    }

    async fn get_document(&self, id: DocumentId) -> Result<DocumentResource, ContextraError> {
        Ok(DocumentResource {
            id: id.to_string(),
            collection_id: CollectionId::new().to_string(),
            content: "single doc".to_string(),
            metadata: Default::default(),
        })
    }

    async fn create_document(
        &self,
        request: CreateDocumentRequest,
    ) -> Result<DocumentResource, ContextraError> {
        Ok(DocumentResource {
            id: DocumentId::new().to_string(),
            collection_id: CollectionId::new().to_string(),
            content: request.source_path,
            metadata: Default::default(),
        })
    }

    async fn list_collections(
        &self,
        _pagination: Pagination,
    ) -> Result<Page<CollectionResource>, ContextraError> {
        let col = CollectionResource {
            id: CollectionId::new().to_string(),
            name: "test-collection".to_string(),
            metadata: Default::default(),
        };
        Ok(Page::new(vec![col], None, false, Some(1)))
    }

    async fn get_collection(&self, id: CollectionId) -> Result<CollectionResource, ContextraError> {
        Ok(CollectionResource {
            id: id.to_string(),
            name: "found-collection".to_string(),
            metadata: Default::default(),
        })
    }

    async fn create_collection(
        &self,
        request: CreateCollectionRequest,
    ) -> Result<CollectionResource, ContextraError> {
        Ok(CollectionResource {
            id: CollectionId::new().to_string(),
            name: request.name,
            metadata: request.metadata,
        })
    }

    async fn list_conversations(
        &self,
        _pagination: Pagination,
    ) -> Result<Page<ConversationResource>, ContextraError> {
        let conv = ConversationResource {
            id: ConversationId::new().to_string(),
            title: Some("test conversation".to_string()),
            metadata: Default::default(),
        };
        Ok(Page::new(vec![conv], None, false, Some(1)))
    }

    async fn create_conversation(
        &self,
        request: CreateConversationRequest,
    ) -> Result<ConversationResource, ContextraError> {
        Ok(ConversationResource {
            id: ConversationId::new().to_string(),
            title: request.title,
            metadata: request.metadata,
        })
    }

    async fn list_messages(
        &self,
        conversation_id: ConversationId,
        _pagination: Pagination,
    ) -> Result<Page<MessageResource>, ContextraError> {
        let msg = MessageResource {
            id: uuid::Uuid::now_v7(),
            conversation_id: conversation_id.to_string(),
            role: "user".to_string(),
            content: "hello world".to_string(),
            metadata: Default::default(),
        };
        Ok(Page::new(vec![msg], None, false, Some(1)))
    }

    async fn execute_chat(
        &self,
        _conversation_id: ConversationId,
        _request: ChatExecutionRequest,
    ) -> Result<ChatExecutionResponse, ContextraError> {
        Err(ContextraError::ServiceUnavailable(
            "No LLM provider configured. Chat is unavailable.".to_string(),
        ))
    }
}

fn test_app() -> (axum::Router, String) {
    let valid_key = "key123.secret456".to_string();
    let service = TestMockGatewayService {
        valid_api_key: valid_key.clone(),
    };
    let app = build_router(AppState::new(Arc::new(service)));
    (app, valid_key)
}

#[tokio::test]
async fn test_auth_missing_header_returns_401() {
    let (app, _) = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/collections?limit=1")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_invalid_api_key_returns_401() {
    let (app, _) = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/collections?limit=1")
                .header("x-api-key", "invalid.key")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_auth_valid_api_key_succeeds() {
    let (app, key) = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/collections?limit=1")
                .header("x-api-key", key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_document_endpoint_delegation() {
    let (app, key) = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/documents")
                .header("x-api-key", key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_collection_endpoint_delegation() {
    let (app, key) = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/collections")
                .header("x-api-key", key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_conversation_endpoint_delegation() {
    let (app, key) = test_app();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/conversations")
                .header("x-api-key", key)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_chat_without_llm_key_returns_503() {
    let (app, key) = test_app();
    let conv_id = ConversationId::new();
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/conversations/{conv_id}/messages"))
                .header("x-api-key", key)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"message":"Hello"}"#))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}
