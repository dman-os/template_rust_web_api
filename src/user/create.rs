use deps::*;

use crate::utils::*;
use crate::*;

use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Clone)]
pub struct CreateUser;

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema)]
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
        issues: ValidationErrors,
    },
    #[error("internal server error: {message:?}")]
    Internal { message: String },
}

pub type Response = Ref<super::User>;

#[async_trait::async_trait]
impl Endpoint for CreateUser {
    type Request = Request;
    type Response = Response;
    type Error = Error;

    async fn handle(
        &self,
        ctx: &crate::Context,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        validator::Validate::validate(&request).map_err(utils::ValidationErrors::from)?;
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
            UsernameOccupied { .. } | EmailOccupied { .. } | InvalidInput { .. } => {
                Self::BAD_REQUEST
            }
            Internal { .. } => Self::INTERNAL_SERVER_ERROR,
        }
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

    fn successs() -> SuccessResponse<Self::Response> {
        use crate::user::testing::*;
        (
            "Success creating a User object",
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
    use crate::{auth::*, Endpoint};

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

    macro_rules! create_user_integ {
        ($(
            $name:ident: {
                status: $status:expr,
                body: $json_body:expr,
                $(check_json: $check_json:expr,)?
                $(extra_assertions: $extra_fn:expr,)?
            },
        )*) => {
            mod integ {
                use super::*;
                crate::integration_table_tests! {
                    $(
                        $name: {
                            uri: "/users",
                            method: "POST",
                            status: $status,
                            router: crate::user::router(),
                            body: $json_body,
                            $(check_json: $check_json,)?
                            $(extra_assertions: $extra_fn,)?
                        },
                    )*
                }
            }
        };
    }

    create_user_integ! {
        works: {
            status: http::StatusCode::CREATED,
            body: fixture_request_json(),
            check_json: fixture_request_json().remove_keys_from_obj(&["password"]),
            extra_assertions: &|EAArgs { ctx, response_json, .. }| {
                Box::pin(async move {
                    let req_body_json = fixture_request_json();
                    let resp_body_json = response_json.unwrap();
                    // TODO: use super user token
                    let token = authenticate::Authenticate.handle(&ctx.ctx(), authenticate::Request{
                        identifier: req_body_json["username"].as_str().unwrap().into(),
                        password: req_body_json["password"].as_str().unwrap().into()
                    }).await.unwrap_or_log().token;

                    let app = crate::user::router().layer(axum::Extension(ctx.ctx()));
                    let resp = app
                        .oneshot(
                            http::Request::builder()
                                .method("GET")
                                .uri(format!("/users/{}", resp_body_json["id"].as_str().unwrap()))
                                .header(
                                    axum::http::header::AUTHORIZATION,
                                    format!("Bearer {token}"),
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
                    check_json(
                        ("expected", &req_body_json.remove_keys_from_obj(&["password"])),
                        ("response", &body),
                    );
                })
            },
        },
        fails_if_username_occupied: {
            status: http::StatusCode::BAD_REQUEST,
            body: fixture_request_json().destructure_into_self(
                serde_json::json!({ "username": USER_01_USERNAME })
            ),
            check_json: serde_json::json!({
                "error": "usernameOccupied"
            }),
        },
        fails_if_email_occupied: {
            status: http::StatusCode::BAD_REQUEST,
            body: fixture_request_json().destructure_into_self(
                serde_json::json!({ "email": USER_01_EMAIL })
            ),
            check_json: serde_json::json!({
                "error": "emailOccupied"
            }),
        },
    }
}
