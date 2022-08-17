use deps::*;

use template_web_api_rust::*;

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
            use utoipa::OpenApi;

            #[derive(utoipa::OpenApi)]
            #[openapi(
                paths(
                    user::get::http,
                ),
                components(
                    schemas(
                        user::get::Response as GetUserResponse,
                        user::get::Error as GetUserError,
                    )
                ),
                modifiers(&SecurityAddon),
                tags(
                    (name = "user", description = "Manipulate User objects")
                )
            )]
            struct ApiDoc;

            struct SecurityAddon;

            impl utoipa::Modify for SecurityAddon {
                fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
                    use utoipa::openapi::security::*;
                    if let Some(components) = openapi.components.as_mut() {
                        components.add_security_scheme(
                            "api_key",
                            SecurityScheme::ApiKey(ApiKey::Header(ApiKeyValue::new("todo_apikey"))),
                        )
                    }
                }
            }

            let config = Config {};
            let db_url = std::env::var("DATABASE_URL").unwrap_or_log();
            // tracing::info!("db_url: {db_url}");
            let db_pool = sqlx::PgPool::connect(&db_url)
                .await
                .unwrap_or_log();
            let ctx = Context { db_pool, config };
            let ctx = std::sync::Arc::new(ctx);
            let app = axum::Router::new()
                .merge(
                    utoipa_swagger_ui::SwaggerUi::new("/swagger-ui/*tail")
                        .url("/api-doc/openapi.json", ApiDoc::openapi()),
                )
                .merge(user::router())
                .layer(axum::Extension(ctx))
                .layer(tower_http::trace::TraceLayer::new_for_http());

            let address = std::net::SocketAddr::from((std::net::Ipv4Addr::UNSPECIFIED, 8080));
            tracing::info!("Server listening at {address:?}");
            axum::Server::bind(&address)
                .serve(app.into_make_service())
                .await
        })
        .unwrap_or_log()
}

