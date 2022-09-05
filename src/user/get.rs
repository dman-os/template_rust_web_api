use deps::*;

use axum::extract::*;

use crate::*;

use super::User;

#[derive(Clone, Copy, Debug)]
pub struct GetUser;

#[derive(Debug)]
pub struct Request {
    token: std::sync::Arc<str>,
    id: uuid::Uuid,
}

#[derive(Debug, thiserror::Error, serde::Serialize, utoipa::ToSchema)]
#[serde(crate = "serde", tag = "error", rename_all = "camelCase")]
pub enum Error {
    #[error("not found at id: {id:?}")]
    NotFound { id: uuid::Uuid },
    #[error("acess denied")]
    AccessDenied,
    #[error("internal server error: {message:?}")]
    Internal { message: String },
}

impl From<auth::authorize::Error> for Error {
    fn from(err: auth::authorize::Error) -> Self {
        use auth::authorize::Error;
        match err {
            Error::Unauthorized | Error::InvalidToken => Self::AccessDenied,
            Error::Internal { message } => Self::Internal { message },
        }
    }
}

#[async_trait::async_trait]
impl crate::AuthenticatedEndpoint for GetUser {
    type Request = Request;
    type Response = User;
    type Error = Error;

    fn authorize_request(&self, request: &Self::Request) -> crate::auth::authorize::Request {
        crate::auth::authorize::Request {
            token: request.token.clone(),
            resource: crate::auth::Resource::User { id: request.id },
            action: crate::auth::Action::Read,
        }
    }

    #[tracing::instrument(skip(ctx))]
    async fn handle(
        &self,
        ctx: &crate::Context,
        _accessing_user: uuid::Uuid,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        let id = request.id;

        sqlx::query_as!(
            User,
            r#"
SELECT 
    id,
    created_at,
    updated_at,
    email::TEXT as "email!",
    username::TEXT as "username!",
    pic_url
FROM users
WHERE id = $1::uuid
            "#,
            &id
        )
        .fetch_one(&ctx.db_pool)
        .await
        .map_err(|err| match err {
            sqlx::Error::RowNotFound => Error::NotFound { id },
            _ => Error::Internal {
                message: format!("db error: {err}"),
            },
        })
    }
}

impl From<&Error> for axum::http::StatusCode {
    fn from(err: &Error) -> Self {
        use Error::*;
        match err {
            NotFound { .. } => Self::NOT_FOUND,
            AccessDenied => Self::UNAUTHORIZED,
            Internal { .. } => Self::INTERNAL_SERVER_ERROR,
        }
    }
}

impl HttpEndpoint for GetUser {
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/users/:id";

    type Parameters = (BearerToken, Path<uuid::Uuid>);

    fn request(
        (BearerToken(token), Path(id)): Self::Parameters,
    ) -> Result<Self::Request, Self::Error> {
        Ok(self::Request { token, id })
    }
}

impl DocumentedEndpoint for GetUser {
    const TAG: &'static crate::Tag = &super::TAG;
    const SUMMARY: &'static str = "Get the User at the given id";

    fn successs() -> SuccessResponse<Self::Response> {
        use crate::user::testing::*;
        (
            "Success getting User",
            Self::Response {
                id: Default::default(),
                created_at: time::OffsetDateTime::now_utc(),
                updated_at: time::OffsetDateTime::now_utc(),
                email: USER_01_EMAIL.into(),
                username: USER_01_USERNAME.into(),
                pic_url: Some("https:://example.com/picture.jpg".into()),
            },
        )
    }

    fn errors() -> Vec<ErrorResponse<Error>> {
        vec![
            ("Access denied", Error::AccessDenied),
            (
                "Not found",
                Error::NotFound {
                    id: Default::default(),
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

#[cfg(test)]
mod tests {
    use deps::*;

    use crate::user::testing::*;
    use crate::utils::testing::*;
    use crate::{auth::*, Endpoint};

    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn get_user_works() {
        let ctx = TestContext::new(crate::function!()).await;
        {
            let app = crate::user::router().layer(axum::Extension(ctx.ctx()));

            let token = authenticate::Authenticate
                .handle(
                    &ctx.ctx(),
                    authenticate::Request {
                        username: Some(USER_01_USERNAME.into()),
                        email: None,
                        password: "password".into(),
                    },
                )
                .await
                .unwrap_or_log()
                .token;

            let resp = app
                .oneshot(
                    Request::builder()
                        .uri(format!("/users/{USER_01_ID}"))
                        .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"))
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
