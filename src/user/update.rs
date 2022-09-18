use deps::*;

use crate::utils::*;
use crate::*;

use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone)]
pub struct UpdateUser;

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct Request {
    #[serde(skip)]
    auth_token: Option<std::sync::Arc<str>>,
    #[serde(skip)]
    user_id: Option<uuid::Uuid>,
    #[validate(length(min = 5))]
    pub username: Option<String>,
    #[validate(email)]
    pub email: Option<String>,
    #[validate(url)]
    pub pic_url: Option<String>,
    #[validate(length(min = 8))]
    pub password: Option<String>,
}
impl Request {
    fn is_empty(&self) -> bool {
        self.username.is_none()
            && self.email.is_none()
            && self.pic_url.is_none()
            && self.password.is_none()
    }
}

#[derive(Debug, Serialize, thiserror::Error, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase", tag = "error")]
pub enum Error {
    #[error("not found at id: {id:?}")]
    NotFound { id: uuid::Uuid },
    #[error("acess denied")]
    AccessDenied,
    #[error("username occupied: {username:?}")]
    UsernameOccupied { username: String },
    #[error("email occupied: {email:?}")]
    EmailOccupied { email: String },
    #[error("invalid input: {issues:?}")]
    InvalidInput {
        #[from]
        issues: ValidationErrors,
    },
    #[error("internal server error: {message:?}")]
    Internal { message: String },
}

crate::impl_from_auth_err!(Error);

pub type Response = Ref<super::User>;

#[async_trait::async_trait]
impl AuthenticatedEndpoint for UpdateUser {
    type Request = Request;
    type Response = Response;
    type Error = Error;

    fn authorize_request(&self, request: &Self::Request) -> crate::auth::authorize::Request {
        crate::auth::authorize::Request {
            auth_token: request.auth_token.clone().unwrap(),
            resource: crate::auth::Resource::User {
                id: request.user_id.unwrap(),
            },
            action: crate::auth::Action::Write,
        }
    }

    #[tracing::instrument(skip(ctx))]
    async fn handle(
        &self,
        ctx: &crate::Context,
        accessing_user: uuid::Uuid,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        validator::Validate::validate(&request).map_err(utils::ValidationErrors::from)?;
        if request.is_empty() {
            return AuthenticatedEndpoint::handle(
                &crate::user::get::GetUser,
                ctx,
                accessing_user,
                crate::user::get::Request {
                    auth_token: request.auth_token.unwrap(),
                    id: request.user_id.unwrap(),
                },
            )
            .await
            .map_err(|err| match err {
                user::get::Error::NotFound { id } => Error::NotFound { id },
                user::get::Error::AccessDenied => Error::AccessDenied,
                user::get::Error::Internal { message } => Error::Internal { message },
            });
        }
        let pass_hash = request.password.map(|pass| {
            argon2::hash_encoded(
                pass.as_bytes(),
                &ctx.config.pass_salt_hash,
                &ctx.config.argon2_conf,
            )
            .unwrap_or_log()
        });
        let null_str = "NULL".into();
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
FROM update_user(
    $1,
    $2::TEXT::CITEXT, 
    $3::TEXT::CITEXT, 
    $4,
    $5
)
                "#,
            &request.user_id.unwrap(),
            &request.username.as_ref().unwrap_or(&null_str),
            &request.email.as_ref().unwrap_or(&null_str),
            &request.pic_url.as_ref().unwrap_or(&null_str),
            &pass_hash.as_ref().unwrap_or(&null_str)
        )
        .fetch_one(&ctx.db_pool)
        .await
        .map_err(|err| match &err {
            sqlx::Error::RowNotFound => Error::NotFound {
                id: request.user_id.unwrap(),
            },
            sqlx::Error::Database(boxed) if boxed.constraint().is_some() => {
                match boxed.constraint().unwrap() {
                    "unique_users_username" => Error::UsernameOccupied {
                        username: request.username.unwrap(),
                    },
                    "unique_users_email" => Error::EmailOccupied {
                        email: request.email.unwrap(),
                    },
                    _ => Error::Internal {
                        message: format!("db error: {err}"),
                    },
                }
            }
            _ => Error::Internal {
                message: format!("db error: {err}"),
            },
        })?;
        // TODO: email notification, account activation
        Ok(user.into())
    }
}

impl From<&Error> for axum::http::StatusCode {
    fn from(err: &Error) -> Self {
        use Error::*;
        match err {
            NotFound { .. } => Self::NOT_FOUND,
            AccessDenied => Self::UNAUTHORIZED,
            UsernameOccupied { .. } | EmailOccupied { .. } | InvalidInput { .. } => {
                Self::BAD_REQUEST
            }
            Internal { .. } => Self::INTERNAL_SERVER_ERROR,
        }
    }
}

impl HttpEndpoint for UpdateUser {
    const METHOD: Method = Method::Patch;
    const PATH: &'static str = "/users/:id";

    type Parameters = (BearerToken, Path<uuid::Uuid>, Json<Request>);

    fn request(
        (BearerToken(token), Path(user_id), Json(req)): Self::Parameters,
    ) -> Result<Self::Request, Self::Error> {
        Ok(Request {
            auth_token: Some(token),
            user_id: Some(user_id),
            ..req
        })
    }
}

impl DocumentedEndpoint for UpdateUser {
    const TAG: &'static Tag = &super::TAG;

    const SUMMARY: &'static str = "Create a new User resource.";

    fn successs() -> SuccessResponse<Self::Response> {
        use crate::user::testing::*;
        (
            "Success updating User",
            super::User {
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

    fn errors() -> Vec<ErrorResponse<Self::Error>> {
        use crate::user::testing::*;
        vec![
            ("Access denied", Error::AccessDenied),
            (
                "Not found",
                Error::NotFound {
                    id: Default::default(),
                },
            ),
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
                        issues.into()
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

#[cfg(test)]
mod tests {
    use deps::*;

    use super::Request;

    use crate::user::testing::*;
    use crate::utils::testing::*;

    // fn fixture_request() -> Request {
    //     serde_json::from_value(fixture_request_json()).unwrap()
    // }

    fn fixture_request_empty() -> Request {
        Request {
            auth_token: None,
            user_id: None,
            username: None,
            email: None,
            password: None,
            pic_url: None,
        }
    }

    fn fixture_request_json() -> serde_json::Value {
        serde_json::json!({
            "username": "whish_box",
            "email": "multis@cream.mux",
            "password": "lovebite",
            "picUrl": "http://i.will.neve.eva/eva.leave.im.80ies.soul.babe",
        })
    }

    crate::table_tests! {
        update_user_validate,
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

    update_user_validate! {
        rejects_too_short_usernames: (
            Request {
                username: Some("shrt".into()),
                ..fixture_request_empty()
            },
            Some("username"),
        ),
        rejects_too_short_passwords: (
            Request {
                password: Some("short".into()),
                ..fixture_request_empty()
            },
            Some("password"),
        ),
        rejects_invalid_emails: (
            Request {
                email: Some("invalid".into()),
                ..fixture_request_empty()
            },
            Some("email"),
        ),
        rejects_bad_pic_urls: (
            Request {
                pic_url: Some("invalid".into()),
                ..fixture_request_empty()
            },
            Some("pic_url"),
        ),
    }

    macro_rules! update_user_integ {
        ($(
            $name:ident: {
                uri: $uri:expr,
                auth_token: $auth_token:expr,
                status: $status:expr,
                body: $json_body:expr,
                $(check_json: $check_json:expr,)?
                $(extra_assertions: $extra_fn:expr,)?
                $(print_response: $print_res:expr,)?
            },
        )*) => {
            mod integ {
                use super::*;
                crate::integration_table_tests! {
                    $(
                        $name: {
                            uri: $uri,
                            method: "PATCH",
                            status: $status,
                            router: crate::user::router(),
                            body: $json_body,
                            $(check_json: $check_json,)?
                            auth_token: $auth_token,
                            $(extra_assertions: $extra_fn,)?
                            $(print_response: $print_res,)?
                        },
                    )*
                }
            }
        };
    }

    update_user_integ! {
        works: {
            uri: format!("/users/{USER_01_ID}"),
            auth_token: USER_01_SESSION.into(),
            status: http::StatusCode::OK,
            body: fixture_request_json(),
            check_json: fixture_request_json().remove_keys_from_obj(&["password"]),
            extra_assertions: &|EAArgs { ctx, response_json, .. }| {
                Box::pin(async move {
                    let req_body_json = fixture_request_json();
                    let resp_body_json = response_json.unwrap();
                    tracing::info!(?resp_body_json);
                    assert!(
                        resp_body_json["updatedAt"].as_i64().unwrap() >
                        resp_body_json["createdAt"].as_i64().unwrap()
                    );
                    let app = crate::user::router().layer(axum::Extension(ctx.ctx()));
                    let resp = app
                        .oneshot(
                            http::Request::builder()
                                .method("GET")
                                .uri(format!("/users/{USER_01_ID}"))
                                .header(
                                    axum::http::header::AUTHORIZATION,
                                    format!("Bearer {USER_01_SESSION}"),
                                )
                                .body(Default::default())
                                .unwrap_or_log(),
                        )
                        .await
                        .unwrap_or_log();
                    assert_eq!(resp.status(), http::StatusCode::OK);
                    let body = resp.into_body();
                    let body = hyper::body::to_bytes(body).await.unwrap_or_log();
                    let body = serde_json::from_slice(&body).unwrap_or_log();
                    tracing::info!(?body);
                    check_json(
                        ("expected", &req_body_json.remove_keys_from_obj(&["password"])),
                        ("response", &body),
                    );
                })
            },
            print_response: true,
        },
        fails_if_username_occupied: {
            uri: format!("/users/{USER_01_ID}"),
            auth_token: USER_01_SESSION.into(),
            status: http::StatusCode::BAD_REQUEST,
            body: fixture_request_json().destructure_into_self(
                serde_json::json!({ "username": USER_02_USERNAME })
            ),
            check_json: serde_json::json!({
                "error": "usernameOccupied"
            }),
        },
        fails_if_email_occupied: {
            uri: format!("/users/{USER_01_ID}"),
            auth_token: USER_01_SESSION.into(),
            status: http::StatusCode::BAD_REQUEST,
            body: fixture_request_json().destructure_into_self(
                serde_json::json!({ "email": USER_02_EMAIL })
            ),
            check_json: serde_json::json!({
                "error": "emailOccupied"
            }),
        },
        fails_if_not_found: {
            uri: format!("/users/{}", uuid::Uuid::new_v4()),
            auth_token: USER_01_SESSION.into(), // FIXME: use super user session
            status: StatusCode::NOT_FOUND,
            body: fixture_request_json(),
            check_json: serde_json::json!({
                "error": "notFound",
            }),
        },
    }
}
