pub mod service;

use async_trait::async_trait;
use auth::AuthContext;
use axum::body::Body;
use axum::extract::{ConnectInfo, FromRef, Path, Query, State};
use axum::http::{HeaderName, HeaderValue, Request, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use common::pagination::Page;
use errors::ContextraError;
use futures_util::stream;
use providers::ChatResponse;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use types::{CollectionId, ConversationId, DocumentId, Metadata};
use utoipa::{IntoParams, OpenApi, ToSchema};
use utoipa_swagger_ui::SwaggerUi;
use uuid::Uuid;

const REQUEST_ID_HEADER: &str = "x-request-id";
const DEFAULT_LIMIT: usize = 25;
const MAX_LIMIT: usize = 100;

#[derive(Clone)]
pub struct AppState {
    service: Arc<dyn GatewayService>,
    rate_limiter: Arc<TokenBucketRateLimiter>,
}

impl AppState {
    pub fn new(service: Arc<dyn GatewayService>) -> Self {
        Self {
            service,
            rate_limiter: Arc::new(TokenBucketRateLimiter::new(
                120,
                120,
                Duration::from_secs(60),
            )),
        }
    }

    pub fn with_rate_limiter(mut self, rate_limiter: TokenBucketRateLimiter) -> Self {
        self.rate_limiter = Arc::new(rate_limiter);
        self
    }
}

impl FromRef<AppState> for Arc<dyn GatewayService> {
    fn from_ref(state: &AppState) -> Self {
        state.service.clone()
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .nest(
            "/api/v1",
            Router::new()
                .route("/documents", get(list_documents).post(create_document))
                .route("/documents/:document_id", get(get_document))
                .route(
                    "/collections",
                    get(list_collections).post(create_collection),
                )
                .route("/collections/:collection_id", get(get_collection))
                .route(
                    "/conversations",
                    get(list_conversations).post(create_conversation),
                )
                .route(
                    "/conversations/:conversation_id/messages",
                    get(list_messages).post(execute_chat),
                )
                .route(
                    "/conversations/:conversation_id/messages/stream",
                    post(stream_chat),
                ),
        )
        .merge(SwaggerUi::new("/docs").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(middleware::from_fn(request_id_middleware))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            rate_limit_middleware,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state)
}

#[async_trait]
pub trait GatewayService: Send + Sync {
    async fn authenticate(&self, token: &str) -> Result<AuthContext, ContextraError>;

    async fn list_documents(
        &self,
        pagination: Pagination,
        filter: DocumentFilter,
    ) -> Result<Page<DocumentResource>, ContextraError>;

    async fn get_document(&self, id: DocumentId) -> Result<DocumentResource, ContextraError>;

    async fn create_document(
        &self,
        request: CreateDocumentRequest,
    ) -> Result<DocumentResource, ContextraError>;

    async fn list_collections(
        &self,
        pagination: Pagination,
    ) -> Result<Page<CollectionResource>, ContextraError>;

    async fn get_collection(&self, id: CollectionId) -> Result<CollectionResource, ContextraError>;

    async fn create_collection(
        &self,
        request: CreateCollectionRequest,
    ) -> Result<CollectionResource, ContextraError>;

    async fn list_conversations(
        &self,
        pagination: Pagination,
    ) -> Result<Page<ConversationResource>, ContextraError>;

    async fn create_conversation(
        &self,
        request: CreateConversationRequest,
    ) -> Result<ConversationResource, ContextraError>;

    async fn list_messages(
        &self,
        conversation_id: ConversationId,
        pagination: Pagination,
    ) -> Result<Page<MessageResource>, ContextraError>;

    async fn execute_chat(
        &self,
        conversation_id: ConversationId,
        request: ChatExecutionRequest,
    ) -> Result<ChatExecutionResponse, ContextraError>;
}

#[derive(Debug, Clone, Copy)]
pub struct Pagination {
    pub cursor: Option<usize>,
    pub limit: usize,
}

impl Pagination {
    fn from_query(query: PaginationQuery) -> Result<Self, ContextraError> {
        let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
        if limit == 0 || limit > MAX_LIMIT {
            return Err(ContextraError::Validation(format!(
                "limit must be between 1 and {MAX_LIMIT}"
            )));
        }

        let cursor = match query.cursor {
            Some(cursor) => Some(cursor.parse::<usize>().map_err(|_| {
                ContextraError::Validation("cursor must be an opaque pagination token".to_string())
            })?),
            None => None,
        };

        Ok(Self { cursor, limit })
    }
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams, ToSchema)]
pub struct PaginationQuery {
    pub cursor: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Default, Deserialize, IntoParams, ToSchema)]
pub struct DocumentFilter {
    pub collection_id: Option<String>,
    pub tag: Option<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct DocumentResource {
    pub id: String,
    pub collection_id: String,
    pub content: String,
    #[schema(value_type = Object)]
    pub metadata: Metadata,
}

impl From<types::Document> for DocumentResource {
    fn from(document: types::Document) -> Self {
        Self {
            id: document.id.to_string(),
            collection_id: document.collection_id.to_string(),
            content: document.content,
            metadata: document.metadata,
        }
    }
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateDocumentRequest {
    pub source_path: String,
}

impl CreateDocumentRequest {
    fn validate(&self) -> Result<(), ContextraError> {
        if self.source_path.trim().is_empty() {
            return Err(ContextraError::Validation(
                "source_path is required".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct CollectionResource {
    pub id: String,
    pub name: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateCollectionRequest {
    pub name: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub metadata: Metadata,
}

impl CreateCollectionRequest {
    fn validate(&self) -> Result<(), ContextraError> {
        if self.name.trim().is_empty() {
            return Err(ContextraError::Validation("name is required".to_string()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ConversationResource {
    pub id: String,
    pub title: Option<String>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct CreateConversationRequest {
    pub title: Option<String>,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct MessageResource {
    pub id: Uuid,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub metadata: Metadata,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ChatExecutionRequest {
    pub message: String,
}

impl ChatExecutionRequest {
    fn validate(&self) -> Result<(), ContextraError> {
        if self.message.trim().is_empty() {
            return Err(ContextraError::Validation(
                "message is required".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, ToSchema)]
pub struct ChatExecutionResponse {
    pub id: String,
    pub model: String,
    pub message: String,
    pub finish_reason: Option<String>,
}

impl From<ChatResponse> for ChatExecutionResponse {
    fn from(response: ChatResponse) -> Self {
        Self {
            id: response.id,
            model: response.model,
            message: response.message.content.unwrap_or_default(),
            finish_reason: response.finish_reason,
        }
    }
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct PageResponse<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
    pub total_count: Option<u64>,
}

impl<T> From<Page<T>> for PageResponse<T> {
    fn from(page: Page<T>) -> Self {
        Self {
            items: page.items,
            next_cursor: page.next_cursor.map(|cursor| cursor.0),
            has_more: page.has_more,
            total_count: page.total_count,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorEnvelope {
    pub error: ErrorBody,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ErrorBody {
    pub code: String,
    pub message: String,
    pub request_id: String,
}

pub struct ApiError {
    error: ContextraError,
    request_id: Option<String>,
}

impl ApiError {
    fn new(error: ContextraError, request_id: Option<RequestId>) -> Self {
        Self {
            error,
            request_id: request_id.map(|request_id| request_id.0),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.error.http_status();
        let request_id = self.request_id.unwrap_or_else(|| "unknown".to_string());
        let body = ErrorEnvelope {
            error: ErrorBody {
                code: self.error.code().to_string(),
                message: self.error.to_string(),
                request_id: request_id.clone(),
            },
        };
        let mut response = (status, Json(body)).into_response();
        if let Ok(header_value) = HeaderValue::from_str(&request_id) {
            response
                .headers_mut()
                .insert(HeaderName::from_static(REQUEST_ID_HEADER), header_value);
        }
        response
    }
}

type ApiResult<T> = Result<T, ApiError>;

#[utoipa::path(
    get,
    path = "/api/v1/documents",
    params(PaginationQuery, DocumentFilter),
    responses((status = 200, body = PageResponse<DocumentResource>), (status = 401, body = ErrorEnvelope), (status = 429, body = ErrorEnvelope)),
    tag = "documents"
)]
async fn list_documents(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Query(pagination): Query<PaginationQuery>,
    Query(filter): Query<DocumentFilter>,
) -> ApiResult<Json<PageResponse<DocumentResource>>> {
    let pagination = Pagination::from_query(pagination)
        .map_err(|error| ApiError::new(error, Some(request_id.clone())))?;
    service
        .list_documents(pagination, filter)
        .await
        .map(PageResponse::from)
        .map(Json)
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/documents/{document_id}",
    params(("document_id" = String, Path, description = "Document id")),
    responses((status = 200, body = DocumentResource), (status = 404, body = ErrorEnvelope)),
    tag = "documents"
)]
async fn get_document(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Path(document_id): Path<DocumentId>,
) -> ApiResult<Json<DocumentResource>> {
    service
        .get_document(document_id)
        .await
        .map(Json)
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    post,
    path = "/api/v1/documents",
    request_body = CreateDocumentRequest,
    responses((status = 201, body = DocumentResource), (status = 400, body = ErrorEnvelope)),
    tag = "documents"
)]
async fn create_document(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Json(payload): Json<CreateDocumentRequest>,
) -> ApiResult<(StatusCode, Json<DocumentResource>)> {
    payload
        .validate()
        .map_err(|error| ApiError::new(error, Some(request_id.clone())))?;
    service
        .create_document(payload)
        .await
        .map(|document| (StatusCode::CREATED, Json(document)))
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections",
    params(PaginationQuery),
    responses((status = 200, body = PageResponse<CollectionResource>)),
    tag = "collections"
)]
async fn list_collections(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Query(query): Query<PaginationQuery>,
) -> ApiResult<Json<PageResponse<CollectionResource>>> {
    let pagination = Pagination::from_query(query)
        .map_err(|error| ApiError::new(error, Some(request_id.clone())))?;
    service
        .list_collections(pagination)
        .await
        .map(PageResponse::from)
        .map(Json)
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{collection_id}",
    params(("collection_id" = String, Path, description = "Collection id")),
    responses((status = 200, body = CollectionResource), (status = 404, body = ErrorEnvelope)),
    tag = "collections"
)]
async fn get_collection(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Path(collection_id): Path<CollectionId>,
) -> ApiResult<Json<CollectionResource>> {
    service
        .get_collection(collection_id)
        .await
        .map(Json)
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections",
    request_body = CreateCollectionRequest,
    responses((status = 201, body = CollectionResource), (status = 400, body = ErrorEnvelope)),
    tag = "collections"
)]
async fn create_collection(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Json(payload): Json<CreateCollectionRequest>,
) -> ApiResult<(StatusCode, Json<CollectionResource>)> {
    payload
        .validate()
        .map_err(|error| ApiError::new(error, Some(request_id.clone())))?;
    service
        .create_collection(payload)
        .await
        .map(|collection| (StatusCode::CREATED, Json(collection)))
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/conversations",
    params(PaginationQuery),
    responses((status = 200, body = PageResponse<ConversationResource>)),
    tag = "conversations"
)]
async fn list_conversations(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Query(query): Query<PaginationQuery>,
) -> ApiResult<Json<PageResponse<ConversationResource>>> {
    let pagination = Pagination::from_query(query)
        .map_err(|error| ApiError::new(error, Some(request_id.clone())))?;
    service
        .list_conversations(pagination)
        .await
        .map(PageResponse::from)
        .map(Json)
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    post,
    path = "/api/v1/conversations",
    request_body = CreateConversationRequest,
    responses((status = 201, body = ConversationResource), (status = 400, body = ErrorEnvelope)),
    tag = "conversations"
)]
async fn create_conversation(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Json(payload): Json<CreateConversationRequest>,
) -> ApiResult<(StatusCode, Json<ConversationResource>)> {
    service
        .create_conversation(payload)
        .await
        .map(|conversation| (StatusCode::CREATED, Json(conversation)))
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    get,
    path = "/api/v1/conversations/{conversation_id}/messages",
    params(("conversation_id" = String, Path, description = "Conversation id"), PaginationQuery),
    responses((status = 200, body = PageResponse<MessageResource>)),
    tag = "messages"
)]
async fn list_messages(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Path(conversation_id): Path<ConversationId>,
    Query(query): Query<PaginationQuery>,
) -> ApiResult<Json<PageResponse<MessageResource>>> {
    let pagination = Pagination::from_query(query)
        .map_err(|error| ApiError::new(error, Some(request_id.clone())))?;
    service
        .list_messages(conversation_id, pagination)
        .await
        .map(PageResponse::from)
        .map(Json)
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    post,
    path = "/api/v1/conversations/{conversation_id}/messages",
    params(("conversation_id" = String, Path, description = "Conversation id")),
    request_body = ChatExecutionRequest,
    responses((status = 200, body = ChatExecutionResponse), (status = 400, body = ErrorEnvelope)),
    tag = "messages"
)]
async fn execute_chat(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Path(conversation_id): Path<ConversationId>,
    Json(payload): Json<ChatExecutionRequest>,
) -> ApiResult<Json<ChatExecutionResponse>> {
    payload
        .validate()
        .map_err(|error| ApiError::new(error, Some(request_id.clone())))?;
    service
        .execute_chat(conversation_id, payload)
        .await
        .map(Json)
        .map_err(|error| ApiError::new(error, Some(request_id)))
}

#[utoipa::path(
    post,
    path = "/api/v1/conversations/{conversation_id}/messages/stream",
    params(("conversation_id" = String, Path, description = "Conversation id")),
    request_body = ChatExecutionRequest,
    responses((status = 200, description = "Server-sent event stream")),
    tag = "messages"
)]
async fn stream_chat(
    Extension(request_id): Extension<RequestId>,
    State(service): State<Arc<dyn GatewayService>>,
    Path(conversation_id): Path<ConversationId>,
    Json(payload): Json<ChatExecutionRequest>,
) -> ApiResult<Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>>> {
    payload
        .validate()
        .map_err(|error| ApiError::new(error, Some(request_id.clone())))?;
    let response = service
        .execute_chat(conversation_id, payload)
        .await
        .map_err(|error| ApiError::new(error, Some(request_id)))?;
    let event = Event::default()
        .event("message")
        .json_data(response)
        .unwrap_or_else(|_| Event::default().event("error").data("serialization failed"));
    Ok(Sse::new(stream::once(async move { Ok(event) })))
}

#[derive(Debug, Clone)]
pub struct RequestId(pub String);

async fn request_id_middleware(mut request: Request<Body>, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
        .map(ToString::to_string)
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    let mut response = next.run(request).await;
    if let Ok(header_value) = HeaderValue::from_str(&request_id) {
        response
            .headers_mut()
            .insert(HeaderName::from_static(REQUEST_ID_HEADER), header_value);
    }
    response
}

async fn auth_middleware(
    State(state): State<AppState>,
    mut request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = request_id_from_request(&request);
    let token = bearer_token(request.headers())
        .or_else(|| {
            request
                .headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
        })
        .map(str::to_string);

    let Some(token) = token else {
        return ApiError::new(
            ContextraError::Unauthorized("missing authorization token".to_string()),
            Some(request_id),
        )
        .into_response();
    };

    match state.service.authenticate(&token).await {
        Ok(context) => {
            request.extensions_mut().insert(context);
            next.run(request).await
        }
        Err(error) => ApiError::new(error, Some(request_id)).into_response(),
    }
}

async fn rate_limit_middleware(
    State(state): State<AppState>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let request_id = request_id_from_request(&request);
    let key = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
        .or_else(|| {
            request
                .extensions()
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ConnectInfo(addr)| addr.ip().to_string())
        })
        .unwrap_or_else(|| "anonymous".to_string());

    if state.rate_limiter.allow(&key) {
        next.run(request).await
    } else {
        ApiError::new(
            ContextraError::RateLimited("rate limit exceeded".to_string()),
            Some(request_id),
        )
        .into_response()
    }
}

fn request_id_from_request(request: &Request<Body>) -> RequestId {
    request
        .extensions()
        .get::<RequestId>()
        .cloned()
        .unwrap_or_else(|| RequestId(Uuid::now_v7().to_string()))
}

fn bearer_token(headers: &http::HeaderMap) -> Option<&str> {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
}

#[derive(Debug)]
pub struct TokenBucketRateLimiter {
    capacity: u32,
    refill_tokens: u32,
    refill_every: Duration,
    buckets: Mutex<HashMap<String, Bucket>>,
}

#[derive(Debug, Clone)]
struct Bucket {
    tokens: u32,
    last_refill: Instant,
}

impl TokenBucketRateLimiter {
    pub fn new(capacity: u32, refill_tokens: u32, refill_every: Duration) -> Self {
        Self {
            capacity: capacity.max(1),
            refill_tokens: refill_tokens.max(1),
            refill_every,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn allow(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets.entry(key.to_string()).or_insert(Bucket {
            tokens: self.capacity,
            last_refill: now,
        });

        let elapsed_periods = now
            .duration_since(bucket.last_refill)
            .as_nanos()
            .checked_div(self.refill_every.as_nanos().max(1))
            .unwrap_or(0);
        if elapsed_periods > 0 {
            let refill = self
                .refill_tokens
                .saturating_mul(u32::try_from(elapsed_periods).unwrap_or(u32::MAX));
            bucket.tokens = bucket.tokens.saturating_add(refill).min(self.capacity);
            bucket.last_refill = now;
        }

        if bucket.tokens == 0 {
            return false;
        }
        bucket.tokens -= 1;
        true
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        list_documents,
        get_document,
        create_document,
        list_collections,
        get_collection,
        create_collection,
        list_conversations,
        create_conversation,
        list_messages,
        execute_chat,
        stream_chat
    ),
    components(schemas(
        PaginationQuery,
        DocumentFilter,
        DocumentResource,
        CreateDocumentRequest,
        CollectionResource,
        CreateCollectionRequest,
        ConversationResource,
        CreateConversationRequest,
        MessageResource,
        ChatExecutionRequest,
        ChatExecutionResponse,
        ErrorEnvelope,
        ErrorBody,
        PageResponse<DocumentResource>,
        PageResponse<CollectionResource>,
        PageResponse<ConversationResource>,
        PageResponse<MessageResource>
    )),
    tags(
        (name = "documents"),
        (name = "collections"),
        (name = "conversations"),
        (name = "messages")
    )
)]
pub struct ApiDoc;

pub struct UnconfiguredGatewayService;

#[async_trait]
impl GatewayService for UnconfiguredGatewayService {
    async fn authenticate(&self, _token: &str) -> Result<AuthContext, ContextraError> {
        Err(ContextraError::Unauthorized(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn list_documents(
        &self,
        _pagination: Pagination,
        _filter: DocumentFilter,
    ) -> Result<Page<DocumentResource>, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn get_document(&self, _id: DocumentId) -> Result<DocumentResource, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn create_document(
        &self,
        _request: CreateDocumentRequest,
    ) -> Result<DocumentResource, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn list_collections(
        &self,
        _pagination: Pagination,
    ) -> Result<Page<CollectionResource>, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn get_collection(
        &self,
        _id: CollectionId,
    ) -> Result<CollectionResource, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn create_collection(
        &self,
        _request: CreateCollectionRequest,
    ) -> Result<CollectionResource, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn list_conversations(
        &self,
        _pagination: Pagination,
    ) -> Result<Page<ConversationResource>, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn create_conversation(
        &self,
        _request: CreateConversationRequest,
    ) -> Result<ConversationResource, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn list_messages(
        &self,
        _conversation_id: ConversationId,
        _pagination: Pagination,
    ) -> Result<Page<MessageResource>, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }

    async fn execute_chat(
        &self,
        _conversation_id: ConversationId,
        _request: ChatExecutionRequest,
    ) -> Result<ChatExecutionResponse, ContextraError> {
        Err(ContextraError::Internal(
            "gateway service is not configured".to_string(),
        ))
    }
}
