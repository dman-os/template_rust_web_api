#![allow(clippy::single_component_path_imports)]

#[cfg(feature = "dylink")]
#[allow(unused_imports)]
use dylink;

use deps::*;

pub mod macros;

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
            let db_pool = sqlx::PgPool::connect(&std::env::var("DATABASE_URL").unwrap_or_log())
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

pub fn setup_tracing() -> eyre::Result<()> {
    color_eyre::install()?;
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    tracing_subscriber::fmt()
        // .pretty()
        .compact()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .try_init()
        .map_err(|err| eyre::eyre!(err))?;

    Ok(())
}

#[derive(Debug)]
pub struct Config {}

#[derive(Debug)]
pub struct Context {
    pub db_pool: sqlx::postgres::PgPool,
    pub config: Config,
}

pub type SharedContext = std::sync::Arc<Context>;

mod user {
    use deps::*;

    #[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
    #[serde(crate = "serde", rename_all = "camelCase")]
    pub struct User {
        pub id: uuid::Uuid,
        /// In seconds since unix epoch in UTC.
        #[schema(example = 1234567)]
        #[serde(with = "time::serde::timestamp")]
        pub created_at: time::OffsetDateTime,
        /// In seconds since unix epoch in UTC.
        #[schema(example = 1234567)]
        #[serde(with = "time::serde::timestamp")]
        pub updated_at: time::OffsetDateTime,
        #[schema(example = "alice@example.com")]
        pub email: String,
        #[schema(example = "hunter2")]
        pub username: String,
        pub pic_url: Option<String>,
    }

    pub fn router() -> axum::Router {
        axum::Router::new().route("/users/:id", axum::routing::get(get::http))
    }

    #[cfg(test)]
    pub mod testing {
        pub const USER_01_ID: &'static str = "add83cdf-2ab3-443f-84dd-476d7984cf75";
        pub const USER_02_ID: &'static str = "ce4fe993-04d6-462e-af1d-d734fcc9639d";
        pub const USER_03_ID: &'static str = "d437e73f-4610-462c-ab22-f94b76bba83a";
        pub const USER_04_ID: &'static str = "68cf4d43-62d2-4202-8c50-c79a5f4dd1cc";

        pub const USER_01_USERNAME: &'static str = "sabrina";
        pub const USER_02_USERNAME: &'static str = "archie";
        pub const USER_03_USERNAME: &'static str = "betty";
        pub const USER_04_USERNAME: &'static str = "veronica";

        pub const USER_01_EMAIL: &'static str = "hex.queen@teen.dj";
        pub const USER_02_EMAIL: &'static str = "archie1941@poetry.ybn";
        pub const USER_03_EMAIL: &'static str = "pInXy@melt.shake";
        pub const USER_04_EMAIL: &'static str = "trekkiegirl@ln.pi";
    }

    pub mod get {
        use deps::*;

        use axum::response::{self, IntoResponse};

        use super::User;

        pub struct Request {
            pub id: uuid::Uuid,
        }

        pub type Response = User;

        #[derive(Debug, thiserror::Error, serde::Serialize, utoipa::ToSchema)]
        #[serde(crate = "serde", tag = "error", rename_all = "camelCase")]
        pub enum Error {
            #[error("not found at id: {id:?}")]
            NotFound { id: uuid::Uuid },
            #[error("acess denied")]
            AccessDenied,
            #[error("server error: {message:?}")]
            ServerError { message: String },
        }

        pub async fn handle(ctx: &crate::Context, request: Request) -> Result<Response, Error> {
            sqlx::query_as!(
                User,
                r#"
SELECT id,
    created_at,
    updated_at,
    email::TEXT as "email!",
    username::TEXT as "username!",
    pic_url
FROM users
WHERE id = $1::uuid
            "#,
                &request.id
            )
            .fetch_one(&ctx.db_pool)
            .await
            .map_err(|err| match err {
                sqlx::Error::RowNotFound => Error::NotFound { id: request.id },
                _ => Error::ServerError {
                    message: format!("{err}"),
                },
            })
        }

        /// Get the user under the givem id.
        #[utoipa::path(
            get,
            path = "/users/{id}",
            operation_id = "get_user",
            tag = "user",
            params(
                (
                    "id" = uuid::Uuid,
                    Path,
                )
            ),
            security(
                ("api_key" = [""])
            ),
            responses(
                (
                    status = 200,
                    description = "Success",
                    body = Response,
                ),
                (
                    status = 401,
                    description = "Access denied",
                    body = Error,
                    example = json!(Error::AccessDenied)
                ),
                (
                    status = 404,
                    description = "Not found",
                    body = Error,
                    example = json!(
                        Error::NotFound { id: Default::default() }
                    )
                ),
                (
                    status = 500,
                    description = "Server error",
                    body = Error,
                    example = json!(
                        Error::ServerError { message: "internal server error".to_string() }
                    )
                ),
            ),
        )]
        #[tracing::instrument(skip(ctx))]
        pub async fn http(
            id: axum::extract::Path<uuid::Uuid>,
            ctx: axum::Extension<crate::SharedContext>,
        ) -> response::Response {
            match handle(&ctx, Request { id: *id }).await {
                Ok(ok) => response::Json(ok).into_response(),
                Err(err) => {
                    use axum::http::StatusCode;
                    use Error::*;
                    (
                        match &err {
                            NotFound { .. } => StatusCode::NOT_FOUND,
                            AccessDenied => StatusCode::UNAUTHORIZED,
                            ServerError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
                        },
                        response::Json(err),
                    )
                        .into_response()
                }
            }
        }

        #[cfg(test)]
        mod tests {
            use deps::*;

            use crate::user::testing::*;
            use crate::utils::testing::*;

            use axum::http::{Request, StatusCode};
            use tower::ServiceExt;

            #[tokio::test]
            async fn get_user_works() {
                let ctx = TestContext::new(crate::function!()).await;
                {
                    let app = crate::user::router().layer(axum::Extension(ctx.ctx()));

                    let resp = app
                        .oneshot(
                            Request::builder()
                                .uri(format!("/users/{USER_01_ID}"))
                                .body(Default::default())
                                .unwrap_or_log(),
                        )
                        .await
                        .unwrap_or_log();
                    assert_eq!(resp.status(), StatusCode::OK);
                    let body = resp.into_body();
                    let body = hyper::body::to_bytes(body).await.unwrap_or_log();
                    let body = serde_json::from_slice(&body).unwrap_or_log();
                    check_json(
                        (
                            "expected",
                            &serde_json::json!({
                                "id": USER_01_ID,
                                "username": USER_01_USERNAME,
                                "email": USER_01_EMAIL,
                            }),
                        ),
                        ("response", &body),
                    );
                }
                ctx.close().await;
            }
        }
    }
}

mod utils {
    use deps::*;

    /* #[derive(serde::Serialize, utoipa::ToSchema)]
    #[serde(crate = "serde", rename_all = "camelCase")]
    pub struct ErrorResponse<T> {
        pub code: u16,
        #[serde(flatten)]
        pub err: T,
    }

    impl<T> ErrorResponse<T> {
        pub const fn new(code: u16, err: T) -> Self {
            Self { code, err }
        }
    } */

    #[cfg(test)]
    pub mod testing {
        use deps::*;

        use crate::{Context, SharedContext};

        pub fn setup_tracing() -> eyre::Result<()> {
            color_eyre::install()?;
            if std::env::var("RUST_LOG_TEST").is_err() {
                std::env::set_var("RUST_LOG_TEST", "info");
            }

            tracing_subscriber::fmt()
                // .pretty()
                .compact()
                .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
                .with_timer(tracing_subscriber::fmt::time::uptime())
                .try_init()
                .map_err(|err| eyre::eyre!(err))?;

            Ok(())
        }

        // Ensure that the `tracing` stack is only initialised once using `once_cell`
        // isn't required in cargo-nextest since each test runs in a new process
        pub fn setup_tracing_once() {
            use once_cell::sync::Lazy;
            static TRACING: Lazy<()> = Lazy::new(|| {
                dotenvy::dotenv().ok();
                setup_tracing().unwrap();
            });
            Lazy::force(&TRACING);
        }

        pub struct TestContext {
            pub test_name: &'static str,
            ctx: Option<SharedContext>,
            // clean_up_closure: Option<Box<dyn FnOnce(Context) -> ()>>,
            clean_up_closure:
                Option<Box<dyn FnOnce(Context) -> futures::future::BoxFuture<'static, ()>>>,
        }

        impl TestContext {
            pub async fn new(test_name: &'static str) -> Self {
                setup_tracing_once();

                let config = crate::Config {};

                use sqlx::prelude::*;
                let opts = sqlx::postgres::PgConnectOptions::default()
                    .host(
                        std::env::var("TEST_DB_HOST")
                            .expect("TEST_DB_HOST wasn't found in enviroment")
                            .as_str(),
                    )
                    .port(
                        std::env::var("TEST_DB_PORT")
                            .expect("TEST_DB_PORT wasn't found in enviroment")
                            .parse()
                            .expect("TEST_DB_PORT is not a valid number"),
                    )
                    .username(
                        std::env::var("TEST_DB_USER")
                            .expect("TEST_DB_USER wasn't found in enviroment")
                            .as_str(),
                    );

                let mut opts = if let Some(pword) = std::env::var("TEST_DB_PASS").ok() {
                    opts.password(pword.as_str())
                } else {
                    opts
                };
                opts.log_statements("DEBUG".parse().unwrap());

                let mut connection = opts
                    .connect()
                    .await
                    .expect("Failed to connect to Postgres without db");

                connection
                    .execute(&format!(r###"DROP DATABASE IF EXISTS {}"###, test_name)[..])
                    .await
                    .expect("Failed to drop old database.");

                connection
                    .execute(&format!(r###"CREATE DATABASE {}"###, test_name)[..])
                    .await
                    .expect("Failed to create database.");

                let opts = opts.database(test_name);

                // migrate database
                let db_pool = sqlx::PgPool::connect_with(opts)
                    .await
                    .expect("Failed to connect to Postgres as test db.");

                sqlx::migrate!("./migrations")
                    .run(&db_pool)
                    .await
                    .expect("Failed to migrate the database");

                sqlx::migrate!("./fixtures")
                    .set_ignore_missing(true) // don't inspect migrations store
                    .run(&db_pool)
                    .await
                    .expect("Failed to add test data");

                let ctx = Context { db_pool, config };
                Self {
                    test_name,
                    ctx: Some(std::sync::Arc::new(ctx)),
                    /* clean_up_closure: Some(Box::new(move |ctx| {
                        futures::executor::block_on(async move {
                            ctx.db_pool.close().await;

                            tracing::info!("got here: {}", ctx.db_pool.size());
                            tokio::time::timeout(
                                std::time::Duration::from_secs(1),
                                connection
                                    .execute(&format!(r###"DROP DATABASE {test_name}"###)[..]),
                            )
                            .await
                            .expect("Timeout when dropping test database.")
                            .expect("Failed to drop test database.");
                            tracing::info!("got here");
                        })
                    })), */
                    clean_up_closure: Some(Box::new(move |ctx| {
                        Box::pin(async move {
                            ctx.db_pool.close().await;
                            connection
                                .execute(&format!(r###"DROP DATABASE {test_name}"###)[..])
                                .await
                                .expect("Failed to drop test database.");
                        })
                    })),
                }
            }

            pub fn ctx(&self) -> SharedContext {
                self.ctx.clone().unwrap_or_log()
            }

            /// Call this after all holders of the [`SharedContext`] have been dropped.
            pub async fn close(mut self) {
                let ctx = self.ctx.take().unwrap_or_log();
                let ctx = std::sync::Arc::<_>::try_unwrap(ctx).unwrap_or_log();
                (self.clean_up_closure.take().unwrap())(ctx);
            }
            /* pub async fn new_with_service<F>(
                test_name: &'static str,
                cfg_callback: F,
            ) -> (
                Self,
                // TestServer,
                impl FnOnce(Self) -> futures::future::BoxFuture<'static, ()>,
            )
            where
                F:Fn(& Context) -> axum::Router + 'static + Send + Sync + Clone + Copy
            {
                let router = cfg_callback
                Self {
                    test_name,
                }
            } */
        }

        impl Drop for TestContext {
            fn drop(&mut self) {
                if self.ctx.is_some() {
                    tracing::warn!(
                        "test context dropped without cleaning up for: {}",
                        self.test_name
                    )
                }
            }
        }

        pub fn check_json(
            (check_name, check): (&str, &serde_json::Value),
            (json_name, json): (&str, &serde_json::Value),
        ) {
            use serde_json::Value::*;
            match (check, json) {
                (Array(r_arr), Array(arr)) => {
                    for ii in 0..arr.len() {
                        check_json(
                            (&format!("{check_name}[{ii}]"), &r_arr[ii]),
                            (&format!("{json_name}[{ii}]"), &arr[ii]),
                        );
                    }
                }
                (Object(check), Object(response)) => {
                    for (key, val) in check {
                        check_json(
                            (&format!("{check_name}.{key}"), val),
                            (&format!("{json_name}.{key}"), &response[key]),
                        );
                    }
                }
                (check, json) => assert_eq!(check, json, "{check_name} != {json_name}"),
            }
        }
    }
}
