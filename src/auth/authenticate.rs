use deps::*;

use crate::*;

use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone)]
pub struct Authenticate;

#[derive(Debug, Deserialize, Validate)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct Request {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: String,
}

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
    #[error("invalid input: {issues:?}")]
    InvalidInput {
        #[from]
        issues: validator::ValidationErrors,
    },
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
        let null_str = "NULL".to_string();

        let result = sqlx::query!(
            r#"
SELECT user_id, pass_hash
FROM credentials
WHERE user_id = (
    SELECT id
    FROM users
    WHERE email = $1::TEXT::CITEXT OR username = $2::TEXT::CITEXT
)
        "#,
            request.email.as_ref().unwrap_or(&null_str),
            request.username.as_ref().unwrap_or(&null_str),
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
            "Success creating a User object",
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
                "Invalid input",
                Error::InvalidInput {
                    issues: {
                        let mut issues = validator::ValidationErrors::new();
                        issues.add(
                            "email",
                            validator::ValidationError {
                                code: std::borrow::Cow::from("email"),
                                message: None,
                                params: [(
                                    std::borrow::Cow::from("value"),
                                    serde_json::json!("bad.email.com"),
                                )]
                                .into_iter()
                                .collect(),
                            },
                        );
                        issues
                    },
                },
            ),
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
            CredentialsRejected { .. } | InvalidInput { .. } => Self::BAD_REQUEST,
            Internal { .. } => Self::INTERNAL_SERVER_ERROR,
        }
    }
}

#[cfg(test)]
mod tests {
    use deps::*;

    use super::Request;

    use crate::user::testing::*;
    use crate::utils::testing::*;

    use axum::http;
    use tower::ServiceExt;

    fn fixture_request() -> Request {
        serde_json::from_value(fixture_request_json()).unwrap()
    }

    fn fixture_request_json() -> serde_json::Value {
        serde_json::json!({
            "username": USER_01_USERNAME,
            "email": USER_01_EMAIL,
            "password": "password",
        })
    }

    crate::table_tests! {
        authenticate_validate,
        (request, err_field),
        {
            match validator::Validate::validate(&request) {
                Ok(()) => {
                    if let Some(err_field) = err_field {
                        panic!("validation succeeded, was expecting err on field: {err_field}");
                    }
                }
                Err(err) => {
                    let err_field = err_field.expect("unexpected validation failure");
                    if !err.field_errors().contains_key(&err_field) {
                        panic!("validation didn't fail on expected field: {err_field}, {err:?}");
                    }
                }
            }
        }
    }

    authenticate_validate! {
        rejects_if_neither_email_or_username_found: (
            Request {
                username: None,
                email: None,
                ..fixture_request()
            },
            Some("email"),
        ),
    }

    #[tokio::test]
    async fn authenticate_works_with_username() {
        let ctx = TestContext::new(crate::function!()).await;
        {
            let app = crate::auth::router().layer(axum::Extension(ctx.ctx()));

            let body_json = fixture_request_json().remove_keys_from_obj(&["email"]);
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
            assert_eq!(USER_01_ID, body["userId"].as_str().unwrap());

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

            let body_json = fixture_request_json().remove_keys_from_obj(&["username"]);
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
            assert_eq!(USER_01_ID, body["userId"].as_str().unwrap());

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

            let body_json = fixture_request_json()
                .remove_keys_from_obj(&["email"])
                .destructure_into_self(serde_json::json!(
                    {
                        "username": "NEWSTUFF"
                    }
                ));
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

            let body_json = fixture_request_json()
                .remove_keys_from_obj(&["username"])
                .destructure_into_self(serde_json::json!(
                    {
                        "email": "xan@da.man"
                    }
                ));
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

            let body_json = fixture_request_json()
                .remove_keys_from_obj(&["username"])
                .destructure_into_self(serde_json::json!(
                    {
                        "password": "apeshitapeshit"
                    }
                ));
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
