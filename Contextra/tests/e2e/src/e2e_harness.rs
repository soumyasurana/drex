#![allow(clippy::expect_used)]

use gateway::{AppState, GatewayService, build_router};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;

pub async fn spawn_test_gateway(service: Arc<dyn GatewayService>) -> String {
    let app = build_router(AppState::new(service));
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind test TCP listener");
    let addr = listener.local_addr().expect("failed to get local addr");

    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .await
        .expect("test gateway server crashed");
    });

    format!("http://{addr}")
}
