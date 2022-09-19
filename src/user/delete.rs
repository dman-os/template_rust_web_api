use deps::*;

use axum::extract::*;

use crate::*;

#[derive(Clone, Copy, Debug)]
pub struct DeleteUser;

#[derive(Debug)]
pub struct Request {
    pub auth_token: std::sync::Arc<str>,
    pub id: uuid::Uuid,
}

#[derive(Debug, thiserror::Error, serde::Serialize, utoipa::ToSchema)]
#[serde(crate = "serde", tag = "error", rename_all = "camelCase")]
pub enum Error {
    #[error("acess denied")]
    AccessDenied,
    #[error("internal server error: {message:?}")]
    Internal { message: String },
}

crate::impl_from_auth_err!(Error);

pub type Response = NoContent;

#[async_trait::async_trait]
impl crate::AuthenticatedEndpoint for DeleteUser {
    type Request = Request;
    type Response = Response;
    type Error = Error;

    fn authorize_request(&self, request: &Self::Request) -> crate::auth::authorize::Request {
        crate::auth::authorize::Request {
            auth_token: request.auth_token.clone(),
            resource: crate::auth::Resource::User { id: request.id },
            action: crate::auth::Action::Delete,
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

        let was_deleted = sqlx::query!(
            r#"
SELECT delete_user($1)
            "#,
            &id
        )
        .fetch_one(&ctx.db_pool)
        .await
        .map_err(|err| Error::Internal {
            message: format!("db error: {err}"),
        })?;
        tracing::trace!(?was_deleted);
        Ok(NoContent)
    }
}

impl From<&Error> for axum::http::StatusCode {
    fn from(err: &Error) -> Self {
        use Error::*;
        match err {
            AccessDenied => Self::UNAUTHORIZED,
            Internal { .. } => Self::INTERNAL_SERVER_ERROR,
        }
    }
}

impl HttpEndpoint for DeleteUser {
    const METHOD: Method = Method::Delete;
    const PATH: &'static str = "/users/:id";
    const SUCCESS_CODE: StatusCode = StatusCode::NO_CONTENT;

    type HttpRequest = (BearerToken, Path<uuid::Uuid>);

    fn request(
        (BearerToken(token), Path(id)): Self::HttpRequest,
    ) -> Result<Self::Request, Self::Error> {
        Ok(self::Request {
            auth_token: token,
            id,
        })
    }

    fn response(_: Self::Response) -> axum::response::Response {
        Default::default()
    }
}

impl DocumentedEndpoint for DeleteUser {
    const TAG: &'static crate::Tag = &super::TAG;

    fn errors() -> Vec<ErrorResponse<Error>> {
        vec![
            ("Access denied", Error::AccessDenied),
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

    macro_rules! get_user_integ {
        ($(
            $name:ident: {
                uri: $uri:expr,
                auth_token: $auth_token:expr,
                status: $status:expr,
                $(extra_assertions: $extra_fn:expr,)?
            },
        )*) => {
            mod integ {
                use super::*;
                crate::integration_table_tests! {
                    $(
                        $name: {
                            uri: $uri,
                            method: "DELETE",
                            status: $status,
                            router: crate::user::router(),
                            auth_token: $auth_token,
                            $(extra_assertions: $extra_fn,)?
                        },
                    )*
                }
            }
        };
    }

    get_user_integ! {
        works: {
            uri: format!("/users/{USER_01_ID}"),
            auth_token: USER_01_SESSION.into(),
            status: StatusCode::NO_CONTENT,
            extra_assertions: &|EAArgs { ctx, response_json, .. }| {
                Box::pin(async move {
                    let app = crate::user::router().layer(axum::Extension(ctx.ctx()));
                    let resp = app
                        .oneshot(
                            http::Request::builder()
                                .method("GET")
                                .uri(format!("/users/{USER_01_ID}"))
                                .header(
                                    axum::http::header::AUTHORIZATION,
                                    format!("Bearer {USER_04_SESSION}"),
                                )
                                .body(Default::default())
                                .unwrap_or_log(),
                        )
                        .await
                        .unwrap_or_log();
                    assert_eq!(resp.status(), http::StatusCode::NOT_FOUND);
                })
            },
        },
        is_idempotent: {
            uri: format!("/users/{}", uuid::Uuid::new_v4()),
            auth_token: USER_01_SESSION.into(), // FIXME: use super user session
            status: StatusCode::NO_CONTENT,
        },
    }
}
