use deps::*;

use axum::{
    extract::*,
    response::{self, IntoResponse},
};

use crate::*;

use super::User;

#[derive(Clone, Copy, Debug)]
pub struct GetUser;

#[async_trait::async_trait]
impl crate::Endpoint for GetUser {
    type Request = Request;
    type Response = Response;
    type Error = Error;
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/users/:id";

    #[tracing::instrument(skip(ctx))]
    async fn handle(self, ctx: &crate::Context, request: Self::Request) -> Result<Response, Error> {
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
            _ => Error::Internal {
                message: format!("{err}"),
            },
        })
    }
}

impl DocumentedEndpoint<Request, Response, Error> for GetUser {
    const TAG: &'static crate::Tag = &super::TAG;
    const SUMMARY: &'static str = "Get the User at the given id";

    fn successs() -> Vec<SuccessResponse<Response>> {
        use crate::user::testing::*;
        vec![(
            axum::http::StatusCode::OK,
            "Success getting User",
            Response {
                id: Default::default(),
                created_at: time::OffsetDateTime::now_utc(),
                updated_at: time::OffsetDateTime::now_utc(),
                email: USER_01_EMAIL.into(),
                username: USER_01_USERNAME.into(),
                pic_url: Some("https:://example.com/picture.jpg".into()),
            },
        )]
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

#[derive(Debug, utoipa::IntoParams)]
pub struct Request {
    pub id: uuid::Uuid,
}

#[async_trait::async_trait]
impl<B> FromRequest<B> for Request
where
    B: Send,
{
    type Rejection = response::Response;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<uuid::Uuid>::from_request(req)
            .await
            .map_err(|err| err.into_response())?;

        Ok(Self { id })
    }
}

pub type Response = User;

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

// impl Into<axum::http::StatusCode> for &Error {
//     fn into(self) -> axum::http::StatusCode {
//         use Error::*;
//         match self {
//             NotFound { .. } => StatusCode::NOT_FOUND,
//             AccessDenied => StatusCode::UNAUTHORIZED,
//             Internal { .. } => StatusCode::INTERNAL_SERVER_ERROR,
//         }
//     }
// }

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
