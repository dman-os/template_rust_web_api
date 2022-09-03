use deps::*;

use template_rust_web_api::*;

fn main() {
    dotenvy::dotenv().ok();
    setup_tracing().unwrap();
    #[cfg(feature = "dylink")]
    tracing::warn!("dylink enabled");

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap_or_log()
        .block_on(async {
            let config = Config {};
            let db_url = std::env::var("DATABASE_URL").unwrap_or_log();
            let db_pool = sqlx::PgPool::connect(&db_url).await.unwrap_or_log();
            let ctx = Context { db_pool, config };
            let ctx = std::sync::Arc::new(ctx);
            let app = axum::Router::new()
                .merge(utoipa_swagger_ui::SwaggerUi::new("/swagger-ui/*tail").url(
                    "/api-doc/openapi.json",
                    <ApiDoc as utoipa::OpenApi>::openapi(),
                ))
                .merge(user::router())
                .layer(axum::Extension(ctx))
                .layer(
                    tower_http::trace::TraceLayer::new_for_http()
                        .on_response(
                            tower_http::trace::DefaultOnResponse::new()
                                .level(tracing::Level::INFO)
                                .latency_unit(tower_http::LatencyUnit::Micros),
                        )
                        .on_failure(
                            tower_http::trace::DefaultOnFailure::new()
                                .level(tracing::Level::ERROR)
                                .latency_unit(tower_http::LatencyUnit::Micros),
                        ),
                );

            let address = std::net::SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, 8080));
            tracing::info!("Server listening at {address:?}");
            axum::Server::bind(&address)
                .serve(app.into_make_service())
                .await
        })
        .unwrap_or_log()
}
