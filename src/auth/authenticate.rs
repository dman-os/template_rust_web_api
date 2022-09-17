use deps::*;

use crate::*;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub struct Authenticate;

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct Request {
    pub identifier: String,
    pub password: String,
}

/// `token` currently appears to be a UUID but don't rely one this as this may
/// change in the future.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct Response {
    pub user_id: uuid::Uuid,
    pub token: String,
    #[schema(example = 1234567)]
    #[serde(with = "time::serde::timestamp")]
    pub expires_at: time::OffsetDateTime,
}

#[derive(Debug, Serialize, thiserror::Error, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase", tag = "error")]
pub enum Error {
    #[error("credentials rejected")]
    CredentialsRejected,
    #[error("internal server error: {message:?}")]
    Internal { message: String },
}

#[async_trait::async_trait]
impl Endpoint for Authenticate {
    type Request = Request;
    type Response = Response;
    type Error = Error;

    async fn handle(
        &self,
        ctx: &crate::Context,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        let result = sqlx::query!(
            r#"
SELECT user_id, pass_hash
FROM credentials
WHERE user_id = (
    SELECT id
    FROM users
    WHERE email = $1::TEXT::CITEXT OR username = $1::TEXT::CITEXT
)
        "#,
            &request.identifier,
        )
        .fetch_one(&ctx.db_pool)
        .await
        .map_err(|err| match &err {
            sqlx::Error::RowNotFound => Error::CredentialsRejected,
            _ => Error::Internal {
                message: format!("db error: {err}"),
            },
        })?;
        let pass_valid =
            argon2::verify_encoded(&result.pass_hash[..], request.password.as_bytes()).unwrap();
        if !pass_valid {
            return Err(Error::CredentialsRejected);
        }

        let user_id = result.user_id;
        let expires_at =
            time::OffsetDateTime::now_utc().saturating_add(ctx.config.auth_token_lifespan);
        let token = uuid::Uuid::new_v4().to_string();
        sqlx::query!(
            r#"
INSERT INTO sessions (token, user_id, expires_at)
VALUES (
    $1,
    $2,
    $3
)
        "#,
            &token,
            &user_id,
            &expires_at
        )
        .execute(&ctx.db_pool)
        .await
        .map_err(|err| Error::Internal {
            message: format!("db error: {err}"),
        })?;

        Ok(Response {
            user_id,
            expires_at,
            token,
        })
    }
}

impl HttpEndpoint for Authenticate {
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/authenticate";

    type Parameters = (Json<Request>,);

    fn request((Json(req),): Self::Parameters) -> Result<Self::Request, Self::Error> {
        Ok(req)
    }
}

impl DocumentedEndpoint for Authenticate {
    const TAG: &'static Tag = &super::TAG;
    const SUMMARY: &'static str = "Create a new User resource.";

    fn successs() -> SuccessResponse<Self::Response> {
        (
            "Success authenticating.",
            Self::Response {
                user_id: Default::default(),
                token: "mcpqwen8y3489nc8y2pf".into(),
                expires_at: time::OffsetDateTime::now_utc(),
            },
        )
    }

    fn errors() -> Vec<ErrorResponse<Self::Error>> {
        vec![
            ("Credentials rejected", Error::CredentialsRejected),
            (
                "Internal server error",
                Error::Internal {
                    message: "internal server error".to_string(),
                },
            ),
        ]
    }
}

impl From<&Error> for axum::http::StatusCode {
    fn from(err: &Error) -> Self {
        use Error::*;
        match err {
            CredentialsRejected { .. } => Self::BAD_REQUEST,
            Internal { .. } => Self::INTERNAL_SERVER_ERROR,
        }
    }
}

#[cfg(test)]
mod tests {
    use deps::*;

    use crate::user::testing::*;
    use crate::utils::testing::*;

    use axum::http;
    use tower::ServiceExt;

    #[tokio::test]
    async fn authenticate_works_with_username() {
        let ctx = TestContext::new(crate::function!()).await;
        {
            let app = crate::auth::router().layer(axum::Extension(ctx.ctx()));

            let body_json = serde_json::json!({
                "identifier": USER_01_USERNAME,
                "password": "password",
            });
            let resp = app
                .oneshot(
                    http::Request::builder()
                        .method("POST")
                        .uri("/authenticate")
                        .header(axum::http::header::CONTENT_TYPE, "application/json")
                        .body(serde_json::to_vec(&body_json).unwrap().into())
                        .unwrap_or_log(),
                )
                .await
                .unwrap_or_log();
            assert_eq!(resp.status(), http::StatusCode::OK);
            let body = resp.into_body();
            let body = hyper::body::to_bytes(body).await.unwrap_or_log();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_log();
            assert!(body["expiresAt"].is_number());
            assert!(body["token"].is_string());
            assert_eq!(USER_01_ID.to_string(), body["userId"].as_str().unwrap());

            let app = crate::user::router().layer(axum::Extension(ctx.ctx()));
            let resp = app
                .oneshot(
                    http::Request::builder()
                        .method("GET")
                        .uri(format!("/users/{}", body["userId"].as_str().unwrap()))
                        .header(
                            axum::http::header::AUTHORIZATION,
                            format!("Bearer {}", body["token"].as_str().unwrap()),
                        )
                        .body(Default::default())
                        .unwrap_or_log(),
                )
                .await
                .unwrap_or_log();
            assert_eq!(resp.status(), http::StatusCode::OK);
        }
        ctx.close().await;
    }

    #[tokio::test]
    async fn authenticate_works_with_email() {
        let ctx = TestContext::new(crate::function!()).await;
        {
            let app = crate::auth::router().layer(axum::Extension(ctx.ctx()));

            let body_json = serde_json::json!({
                "identifier": USER_01_EMAIL,
                "password": "password",
            });
            let resp = app
                .oneshot(
                    http::Request::builder()
                        .method("POST")
                        .uri("/authenticate")
                        .header(axum::http::header::CONTENT_TYPE, "application/json")
                        .body(serde_json::to_vec(&body_json).unwrap().into())
                        .unwrap_or_log(),
                )
                .await
                .unwrap_or_log();
            assert_eq!(resp.status(), http::StatusCode::OK);
            let body = resp.into_body();
            let body = hyper::body::to_bytes(body).await.unwrap_or_log();
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_log();
            assert!(body["expiresAt"].is_number());
            assert!(body["token"].is_string());
            assert_eq!(USER_01_ID.to_string(), body["userId"].as_str().unwrap());

            let app = crate::user::router().layer(axum::Extension(ctx.ctx()));
            let resp = app
                .oneshot(
                    http::Request::builder()
                        .method("GET")
                        .uri(format!("/users/{}", body["userId"].as_str().unwrap()))
                        .header(
                            axum::http::header::AUTHORIZATION,
                            format!("Bearer {}", body["token"].as_str().unwrap()),
                        )
                        .body(Default::default())
                        .unwrap_or_log(),
                )
                .await
                .unwrap_or_log();
            assert_eq!(resp.status(), http::StatusCode::OK);
        }
        ctx.close().await;
    }

    #[tokio::test]
    async fn authenticate_fails_if_username_not_found() {
        let ctx = TestContext::new(crate::function!()).await;
        {
            let app = crate::auth::router().layer(axum::Extension(ctx.ctx()));

            let body_json = serde_json::json!({
                "identifier": "golden_eel",
                "password": "password",
            });
            let resp = app
                .oneshot(
                    http::Request::builder()
                        .method("POST")
                        .uri("/authenticate")
                        .header("Content-Type", "application/json")
                        .body(serde_json::to_vec(&body_json).unwrap().into())
                        .unwrap_or_log(),
                )
                .await
                .unwrap_or_log();
            assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);
            let body = resp.into_body();
            let body = hyper::body::to_bytes(body).await.unwrap_or_log();
            let body = serde_json::from_slice(&body).unwrap_or_log();
            check_json(
                (
                    "expected",
                    &serde_json::json!({
                        "error": "credentialsRejected"
                    }),
                ),
                ("response", &body),
            );
        }
        ctx.close().await;
    }

    #[tokio::test]
    async fn authenticate_fails_if_email_not_found() {
        let ctx = TestContext::new(crate::function!()).await;
        {
            let app = crate::auth::router().layer(axum::Extension(ctx.ctx()));

            let body_json = serde_json::json!({
                "identifier": "xan@da.man",
                "password": "password",
            });
            let resp = app
                .oneshot(
                    http::Request::builder()
                        .method("POST")
                        .uri("/authenticate")
                        .header("Content-Type", "application/json")
                        .body(serde_json::to_vec(&body_json).unwrap().into())
                        .unwrap_or_log(),
                )
                .await
                .unwrap_or_log();
            assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);
            let body = resp.into_body();
            let body = hyper::body::to_bytes(body).await.unwrap_or_log();
            let body = serde_json::from_slice(&body).unwrap_or_log();
            check_json(
                (
                    "expected",
                    &serde_json::json!({
                        "error": "credentialsRejected"
                    }),
                ),
                ("response", &body),
            );
        }
        ctx.close().await;
    }

    #[tokio::test]
    async fn authenticate_fails_if_password_is_wrong() {
        let ctx = TestContext::new(crate::function!()).await;
        {
            let app = crate::auth::router().layer(axum::Extension(ctx.ctx()));

            let body_json = serde_json::json!({
                "identifier": USER_01_EMAIL,
                "password": "apeshit",
            });
            let resp = app
                .oneshot(
                    http::Request::builder()
                        .method("POST")
                        .uri("/authenticate")
                        .header("Content-Type", "application/json")
                        .body(serde_json::to_vec(&body_json).unwrap().into())
                        .unwrap_or_log(),
                )
                .await
                .unwrap_or_log();
            assert_eq!(resp.status(), http::StatusCode::BAD_REQUEST);
            let body = resp.into_body();
            let body = hyper::body::to_bytes(body).await.unwrap_or_log();
            let body = serde_json::from_slice(&body).unwrap_or_log();
            check_json(
                (
                    "expected",
                    &serde_json::json!({
                        "error": "credentialsRejected"
                    }),
                ),
                ("response", &body),
            );
        }
        ctx.close().await;
    }
}
