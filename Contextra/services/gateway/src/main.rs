use gateway::{
    AppState, build_router, service::NoopLLMProvider, service::ProductionGatewayService,
};
use providers::{LLMProvider, ProviderFactory};
use settings::Settings;
use std::net::SocketAddr;
use std::sync::Arc;
use storage::{db::PgPool, vector_store::QdrantVectorStore};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    // Initialize tracing subscriber
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    info!("Starting Contextra Gateway service...");

    // 1. Load configuration
    let settings = Settings::load()?;
    info!(
        "Loaded configuration for environment: {}",
        settings.server.env
    );

    // 2. Connect to PostgreSQL and run database migrations
    info!("Connecting to PostgreSQL at {}", settings.database.url);
    let db = PgPool::connect(&settings.database.url).await?;

    info!("Running database migrations...");
    db.run_migrations().await?;
    info!("Database migrations applied successfully.");

    // 3. Connect to Qdrant Vector Store
    info!(
        "Connecting to Qdrant vector store at {}",
        settings.vector_store.url
    );
    let vector_store = QdrantVectorStore::connect(
        &settings.vector_store.url,
        settings.vector_store.api_key.clone(),
    )?;

    // 4. Initialize LLM Provider
    let provider_factory = ProviderFactory::new(settings.providers.clone());
    let (llm_provider, has_real_llm): (Arc<dyn LLMProvider>, bool) = match provider_factory
        .create_configured_llm_provider()
    {
        Ok(provider) => {
            info!("Successfully initialized LLM provider");
            (provider, true)
        }
        Err(err) => {
            tracing::warn!(
                "No LLM provider key configured ({err}). Chat endpoint will return 503 Service Unavailable, but documents, collections, and conversation APIs remain functional."
            );
            (Arc::new(NoopLLMProvider), false)
        }
    };

    // 5. Construct Gateway service
    let gateway_service =
        ProductionGatewayService::new(db, vector_store, llm_provider, has_real_llm, &settings);

    let app = build_router(AppState::new(Arc::new(gateway_service)));

    let bind_addr = format!("{}:{}", settings.server.host, settings.server.port);
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    info!("Contextra Gateway listening on http://{}", bind_addr);

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
