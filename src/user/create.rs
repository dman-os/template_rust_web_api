use deps::*;

use crate::*;

use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone)]
pub struct CreateUser;

#[async_trait::async_trait]
impl Endpoint for CreateUser {
    type Request = Request;

    type Response = super::User;

    type Error = Error;

    async fn handle(
        &self,
        ctx: &crate::Context,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        validator::Validate::validate(&request)?;
        let pass_hash = argon2::hash_encoded(
            request.password.as_bytes(),
            &ctx.config.pass_salt_hash,
            &ctx.config.argon2_conf,
        )
        .unwrap_or_log();
        let user = sqlx::query_as!(
            super::User,
            r#"
SELECT
    id as "id!",
    created_at as "created_at!",
    updated_at as "updated_at!",
    email::TEXT as "email!",
    username::TEXT as "username!",
    pic_url
FROM create_user($1::TEXT::CITEXT, $2::TEXT::CITEXT, $3)
                "#,
            &request.username,
            &request.email,
            &pass_hash
        )
        .fetch_one(&ctx.db_pool)
        .await
        .map_err(|err| match &err {
            sqlx::Error::Database(boxed) if boxed.constraint().is_some() => {
                match boxed.constraint().unwrap() {
                    "unique_users_username" => Error::UsernameOccupied {
                        username: request.username,
                    },
                    "unique_users_email" => Error::EmailOccupied {
                        email: request.email,
                    },
                    _ => Error::Internal {
                        message: format!("{err}"),
                    },
                }
            }
            _ => Error::Internal {
                message: format!("{err}"),
            },
        })?;
        // TODO: email notification, account activation
        Ok(user)
    }
}

impl HttpEndpoint for CreateUser {
    const METHOD: Method = Method::Post;
    const PATH: &'static str = "/users";
    const SUCCESS_CODE: StatusCode = StatusCode::CREATED;

    type Parameters = (Json<Request>,);

    fn request((Json(req),): Self::Parameters) -> Result<Self::Request, Self::Error> {
        Ok(req)
    }
}

impl DocumentedEndpoint for CreateUser {
    const TAG: &'static Tag = &super::TAG;

    const SUMMARY: &'static str = "Create a new User resource.";

    fn successs() -> Vec<SuccessResponse<Self::Response>> {
        use crate::user::testing::*;
        vec![(
            StatusCode::CREATED,
            "Success creating a User object",
            Self::Response {
                id: Default::default(),
                created_at: time::OffsetDateTime::now_utc(),
                updated_at: time::OffsetDateTime::now_utc(),
                email: USER_01_EMAIL.into(),
                username: USER_01_USERNAME.into(),
                pic_url: Some("https:://example.com/picture.jpg".into()),
            },
        )]
    }

    fn errors() -> Vec<ErrorResponse<Self::Error>> {
        use crate::user::testing::*;
        vec![
            (
                "Username occupied",
                Error::UsernameOccupied {
                    username: USER_01_USERNAME.into(),
                },
            ),
            (
                "Email occupied",
                Error::EmailOccupied {
                    email: USER_01_EMAIL.into(),
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

#[derive(Debug, Deserialize, Validate)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct Request {
    #[validate(length(min = 5))]
    pub username: String,
    #[validate(email)]
    pub email: String,
    #[validate(length(min = 8))]
    pub password: String,
}

#[derive(Debug, Serialize, thiserror::Error, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase", tag = "error")]
pub enum Error {
    #[error("username occupied: {username:?}")]
    UsernameOccupied { username: String },
    #[error("email occupied: {email:?}")]
    EmailOccupied { email: String },
    #[error("invalid input: {issues:?}")]
    InvalidInput {
        #[from]
        issues: validator::ValidationErrors,
    },
    #[error("internal server error: {message:?}")]
    Internal { message: String },
}

impl From<&Error> for axum::http::StatusCode {
    fn from(err: &Error) -> Self {
        use Error::*;
        match err {
            UsernameOccupied { .. } | EmailOccupied { .. } | InvalidInput { .. } => {
                Self::BAD_REQUEST
            }
            Internal { .. } => Self::INTERNAL_SERVER_ERROR,
        }
    }
}

#[cfg(test)]
mod tests {
    use deps::*;

    use super::Request;

    // use crate::user::testing::*;
    use crate::utils::testing::*;

    use axum::http;
    use tower::ServiceExt;

    fn fixture_request() -> Request {
        serde_json::from_value(fixture_request_json()).unwrap()
    }

    fn fixture_request_json() -> serde_json::Value {
        serde_json::json!({
            "username": "whish_box",
            "email": "multis@cream.mux",
            "password": "lovebite",
        })
    }

    crate::table_tests! {
        create_user_validate,
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

    create_user_validate! {
        rejects_too_short_usernames: (
            Request {
                username: "shrt".into(),
                ..fixture_request()
            },
            Some("username"),
        ),
        rejects_too_short_passwords: (
            Request {
                password: "short".into(),
                ..fixture_request()
            },
            Some("password"),
        ),
        rejects_invalid_emails: (
            Request {
                email: "invalid".into(),
                ..fixture_request()
            },
            Some("email"),
        ),
    }

    #[tokio::test]
    async fn create_user_works() {
        let ctx = TestContext::new(crate::function!()).await;
        {
            let app = crate::user::router().layer(axum::Extension(ctx.ctx()));

            let body_json = fixture_request_json();
            let resp = app
                .oneshot(
                    http::Request::builder()
                        .method("POST")
                        .uri("/users")
                        .header("Content-Type", "application/json")
                        .body(serde_json::to_vec(&body_json).unwrap().into())
                        .unwrap_or_log(),
                )
                .await
                .unwrap_or_log();
            assert_eq!(resp.status(), http::StatusCode::CREATED);
            let body = resp.into_body();
            let body = hyper::body::to_bytes(body).await.unwrap_or_log();
            let body = serde_json::from_slice(&body).unwrap_or_log();
            check_json(
                (
                    "expected",
                    &body_json.clone().remove_keys_from_obj(&["password"]),
                ),
                ("response", &body),
            );

            let app = crate::user::router().layer(axum::Extension(ctx.ctx()));
            let resp = app
                .oneshot(
                    http::Request::builder()
                        .method("GET")
                        .uri(format!("/users/{}", body["id"].as_str().unwrap()))
                        .body(Default::default())
                        .unwrap_or_log(),
                )
                .await
                .unwrap_or_log();
            assert_eq!(resp.status(), http::StatusCode::OK);
            let body = resp.into_body();
            let body = hyper::body::to_bytes(body).await.unwrap_or_log();
            let body = serde_json::from_slice(&body).unwrap_or_log();
            check_json(
                ("expected", &body_json.remove_keys_from_obj(&["password"])),
                ("response", &body),
            );
        }
        ctx.close().await;
    }
}

