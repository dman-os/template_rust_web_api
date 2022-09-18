use deps::*;

use axum::extract::*;

use crate::*;

use super::User;

#[derive(Clone, Copy, Debug)]
pub struct GetUser;

#[derive(Debug)]
pub struct Request {
    pub auth_token: std::sync::Arc<str>,
    pub id: uuid::Uuid,
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

crate::impl_from_auth_err!(Error);

pub type Response = Ref<super::User>;

#[async_trait::async_trait]
impl crate::AuthenticatedEndpoint for GetUser {
    type Request = Request;
    type Response = Response;
    type Error = Error;

    fn authorize_request(&self, request: &Self::Request) -> crate::auth::authorize::Request {
        crate::auth::authorize::Request {
            auth_token: request.auth_token.clone(),
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
        .map(|val| val.into())
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
        Ok(self::Request {
            auth_token: token,
            id,
        })
    }
}

impl DocumentedEndpoint for GetUser {
    const TAG: &'static crate::Tag = &super::TAG;
    const SUMMARY: &'static str = "Get the User at the given id";

    fn successs() -> SuccessResponse<Self::Response> {
        use crate::user::testing::*;
        (
            "Success getting User",
            User {
                id: Default::default(),
                created_at: time::OffsetDateTime::now_utc(),
                updated_at: time::OffsetDateTime::now_utc(),
                email: USER_01_EMAIL.into(),
                username: USER_01_USERNAME.into(),
                pic_url: Some("https:://example.com/picture.jpg".into()),
            }
            .into(),
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

    macro_rules! get_user_integ {
        ($(
            $name:ident: {
                uri: $uri:expr,
                auth_token: $auth_token:expr,
                status: $status:expr,
                $(check_json: $check_json:expr,)?
                $(extra_assertions: $extra_fn:expr,)?
            },
        )*) => {
            mod integ {
                use super::*;
                crate::integration_table_tests! {
                    $(
                        $name: {
                            uri: $uri,
                            method: "GET",
                            status: $status,
                            router: crate::user::router(),
                            $(check_json: $check_json,)?
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
            status: StatusCode::OK,
            check_json: serde_json::json!({
                "id": USER_01_ID,
                "username": USER_01_USERNAME,
                "email": USER_01_EMAIL,
            }),
        },
        fails_if_not_found: {
            uri: format!("/users/{}", uuid::Uuid::new_v4()),
            auth_token: USER_01_SESSION.into(), // FIXME: use super user session
            status: StatusCode::NOT_FOUND,
            check_json: serde_json::json!({
                "error": "notFound",
            }),
        },
    }
}
