use deps::*;

use axum::extract::*;

use crate::utils::*;
use crate::*;

use super::User;

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub enum UserSortingField {
    Username,
    Email,
    CreatedAt,
    UpdatedAt,
}

impl SortingField for UserSortingField {
    #[inline]
    fn sql_field_name(&self) -> String {
        match self {
            Self::Username => "username",
            Self::Email => "email",
            Self::CreatedAt => "created_at",
            Self::UpdatedAt => "updated_at",
        }
        .into()
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ListUsers;

crate::alias_and_ref!(ListRequest<UserSortingField>, ListUsersRequest, Request, de);

#[derive(Debug, thiserror::Error, serde::Serialize, utoipa::ToSchema)]
#[serde(crate = "serde", tag = "error", rename_all = "camelCase")]
pub enum Error {
    #[error("acess denied")]
    AccessDenied,
    #[error("invalid input: {issues:?}")]
    InvalidInput {
        #[from]
        issues: ValidationErrors,
    },
    #[error("internal server error: {message:?}")]
    Internal { message: String },
}

crate::impl_from_auth_err!(Error);

crate::alias_and_ref!(ListResponse<super::User>, ListUsersResponse, Response, ser);

#[async_trait::async_trait]
impl crate::AuthenticatedEndpoint for ListUsers {
    type Request = Request;
    type Response = Response;
    type Error = Error;

    fn authorize_request(&self, request: &Self::Request) -> crate::auth::authorize::Request {
        crate::auth::authorize::Request {
            auth_token: request.auth_token.clone().unwrap(),
            resource: crate::auth::Resource::Users,
            action: crate::auth::Action::Read,
        }
    }

    // #[tracing::instrument(skip(ctx))]
    async fn handle(
        &self,
        ctx: &crate::Context,
        _accessing_user: uuid::Uuid,
        Request(request): Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        validator::Validate::validate(&request).map_err(utils::ValidationErrors::from)?;
        let (cursor_clause, sorting_field, sorting_order, filter) = request
            .after_cursor
            .map(|cursor| (true, cursor))
            .or_else(|| request.before_cursor.map(|cursor| (false, cursor)))
            .map(|(is_after, cursor)| {
                let invalid_cursor_err = |msg| Error::InvalidInput {
                    issues: {
                        let mut issues = validator::ValidationErrors::new();
                        let cursor_field = if is_after {
                            "afterCursor"
                        } else {
                            "beforeCursor"
                        };
                        issues.add(
                            cursor_field,
                            validator::ValidationError {
                                code: "invalid_cursor".into(),
                                message: Some(msg),
                                params: [(
                                    std::borrow::Cow::from("value"),
                                    serde_json::json!(cursor),
                                )]
                                .into_iter()
                                .collect(),
                            },
                        );
                        issues.into()
                    },
                };
                let cursor: Cursor<serde_json::Value, UserSortingField> = cursor
                    .parse()
                    .map_err(|_| invalid_cursor_err("unable to decode cursor".into()))?;
                // let op = match (cursor.order, is_after) {
                //     (SortingOrder::Ascending, true) | (SortingOrder::Descending, false) => ">",
                //     (SortingOrder::Ascending, false) | (SortingOrder::Descending, true) => "<",
                // };
                let op = if is_after { ">" } else { "<" };
                // FIXME: sql injection
                let clause = match cursor.field {
                    UserSortingField::Username | UserSortingField::Email => {
                        let value = cursor
                            .value
                            .as_str()
                            .ok_or_else(|| invalid_cursor_err("nonsensical cursor".into()))?;
                        let column = cursor.field.sql_field_name();
                        format!("WHERE {column} {op} '{value}'")
                    }
                    UserSortingField::CreatedAt | UserSortingField::UpdatedAt => {
                        let value = cursor
                            .value
                            .as_i64()
                            .ok_or_else(|| invalid_cursor_err("nonsensical cursor".into()))?;
                        let column = cursor.field.sql_field_name();
                        format!("WHERE {column} {op} (TO_TIMESTAMP({value}) AT TIME ZONE 'UTC')")
                    }
                };
                Ok::<_, Error>((clause.into(), cursor.field, cursor.order, cursor.filter))
            })
            .unwrap_or_else(|| {
                Ok((
                    std::borrow::Cow::from(""),
                    request.sorting_field.unwrap_or(UserSortingField::CreatedAt),
                    request.sorting_order.unwrap_or(SortingOrder::Descending),
                    request.filter,
                ))
            })?;
        let (sorting_field_str, sorting_order_str) =
            (sorting_field.sql_field_name(), sorting_order.sql_key_word());
        let limit = request.limit.unwrap_or(DEFAULT_LIST_LIMIT);
        let results = sqlx::query(
            format!(
                r#"
SELECT 
    id,
    created_at,
    updated_at,
    email::TEXT as "email!",
    username::TEXT as "username!",
    pic_url
FROM (
    SELECT *
    FROM users
    WHERE cast($1 as text) IS NULL OR (
        username ILIKE '%%' || $1 || '%%'
        OR email ILIKE '%%' || $1 || '%%'
    )
    ORDER BY {sorting_field_str}, id {sorting_order_str}
    NULLS LAST
) as f
{cursor_clause}
-- fetch one more to check if we have more data 
-- (counts are expensive or something)
LIMIT $2 + 1 
        "#
            )
            .as_str(),
        )
        .bind(filter.as_ref())
        .bind(limit as i64)
        .fetch_all(&ctx.db_pool)
        .await;
        match results {
            Ok(results) => {
                let more_rows_pending = results.len() == limit + 1;
                let items = results
                    .into_iter()
                    .take(limit as _)
                    .map(|row| {
                        use sqlx::Row;
                        Ok::<_, sqlx::Error>(User {
                            id: row.try_get("id")?,
                            created_at: row.try_get("created_at")?,
                            updated_at: row.try_get("updated_at")?,
                            username: row.try_get("username!")?,
                            email: row.try_get("email!")?,
                            pic_url: row.try_get("pic_url")?,
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|err| Error::Internal {
                        message: format!("row mapping err: {err}"),
                    })?;
                let cursor = if more_rows_pending {
                    Some(
                        Cursor {
                            value: {
                                let last = items.last().unwrap();
                                match sorting_field {
                                    UserSortingField::Username => serde_json::json!(last.username),
                                    UserSortingField::Email => serde_json::json!(last.email),
                                    UserSortingField::CreatedAt => {
                                        serde_json::json!(last.created_at.unix_timestamp())
                                    }
                                    UserSortingField::UpdatedAt => {
                                        serde_json::json!(last.updated_at.unix_timestamp())
                                    }
                                }
                            },
                            field: sorting_field,
                            order: sorting_order,
                            filter,
                        }
                        .to_encoded_str(),
                    )
                } else {
                    None
                };
                Ok(ListUsersResponse { cursor, items }.into())
            }
            Err(sqlx::Error::RowNotFound) => Ok(ListUsersResponse {
                cursor: None,
                items: vec![],
            }
            .into()),
            Err(err) => Err(Error::Internal {
                message: format!("db err: {err}"),
            }),
        }
    }
}

impl From<&Error> for axum::http::StatusCode {
    fn from(err: &Error) -> Self {
        use Error::*;
        match err {
            InvalidInput { .. } => Self::BAD_REQUEST,
            AccessDenied => Self::UNAUTHORIZED,
            Internal { .. } => Self::INTERNAL_SERVER_ERROR,
        }
    }
}

impl HttpEndpoint for ListUsers {
    const METHOD: Method = Method::Get;
    const PATH: &'static str = "/users";

    type HttpRequest = (BearerToken, Json<Request>);

    fn request(
        (BearerToken(token), Json(Request(request))): Self::HttpRequest,
    ) -> Result<Self::Request, Self::Error> {
        Ok(ListUsersRequest {
            auth_token: Some(token),
            ..request
        }
        .into())
    }

    fn response(Response(resp): Self::Response) -> axum::response::Response {
        Json(resp).into_response()
    }
}

impl DocumentedEndpoint for ListUsers {
    const TAG: &'static crate::Tag = &super::TAG;

    fn success_examples() -> Vec<serde_json::Value> {
        use crate::user::testing::*;
        [ListUsersResponse {
            cursor: None,
            items: vec![
                User {
                    id: Default::default(),
                    created_at: time::OffsetDateTime::now_utc(),
                    updated_at: time::OffsetDateTime::now_utc(),
                    email: USER_01_EMAIL.into(),
                    username: USER_01_USERNAME.into(),
                    pic_url: Some("https:://example.com/picture.jpg".into()),
                },
                User {
                    id: Default::default(),
                    created_at: time::OffsetDateTime::now_utc(),
                    updated_at: time::OffsetDateTime::now_utc(),
                    email: USER_02_EMAIL.into(),
                    username: USER_02_USERNAME.into(),
                    pic_url: None,
                },
            ],
        }]
        .into_iter()
        .map(serde_json::to_value)
        .collect::<Result<_, _>>()
        .unwrap()
    }

    fn errors() -> Vec<ErrorResponse<Error>> {
        vec![
            ("Access denied", Error::AccessDenied),
            (
                "Invalid input",
                Error::InvalidInput {
                    issues: {
                        let mut issues = validator::ValidationErrors::new();
                        issues.add(
                            "limit",
                            validator::ValidationError {
                                code: std::borrow::Cow::from("range"),
                                message: None,
                                params: [(std::borrow::Cow::from("value"), serde_json::json!(0))]
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
    // TODO: rigourous test suite, this module's not the most type safe

    use deps::*;

    use crate::user::list::*;
    use crate::user::testing::*;
    use crate::utils::testing::*;

    fn fixture_request() -> ListUsersRequest {
        serde_json::from_value(fixture_request_json()).unwrap()
    }

    fn fixture_request_json() -> serde_json::Value {
        serde_json::json!({
            "limit": 25,
            "filter": USER_01_USERNAME,
            "sortingField": "username",
            "sortingOrder": "descending"
        })
    }

    crate::table_tests! {
        list_users_validate,
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

    list_users_validate! {
        rejects_too_large_limits: (
            ListUsersRequest {
                limit: Some(99999),
                ..fixture_request()
            },
            Some("limit"),
        ),
        rejects_both_cursors_at_once: (
            ListUsersRequest {
                before_cursor: Some("cursorstr".into()),
                after_cursor: Some("cursorstr".into()),
                auth_token: None,
                limit: None,
                filter: None,
                sorting_field: None,
                sorting_order: None,
            },
            Some("__all__"),
        ),
        rejects_cursors_with_filter: (
            ListUsersRequest {
                after_cursor: Some("cursorstr".into()),
                ..fixture_request()
            },
            Some("__all__"),
        ),
    }

    macro_rules! list_users_integ {
        ($(
            $name:ident: {
                auth_token: $auth_token:expr,
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
                            method: "GET",
                            status: $status,
                            router: crate::user::router(),
                            body: $json_body,
                            $(check_json: $check_json,)?
                            auth_token: $auth_token,
                            $(extra_assertions: $extra_fn,)?
                        },
                    )*
                }
            }
        };
    }

    list_users_integ! {
        works: {
            auth_token: USER_01_SESSION.into(),
            status: StatusCode::OK,
            body: fixture_request_json().destructure_into_self(serde_json::json!({
                "limit": 2,
                "filter": null,
            })),
            extra_assertions: &|EAArgs { ctx, response_json, .. }| {
                Box::pin(async move {
                    let resp_body_json = response_json.unwrap();
                    assert_eq!(resp_body_json["items"].as_array().unwrap().len(), 2);
                    assert!(resp_body_json["cursor"].as_str().is_some());
                    let app = crate::user::router().layer(axum::Extension(ctx.ctx()));
                    let resp = app
                        .oneshot(
                            http::Request::builder()
                                .method("GET")
                                .uri("/users")
                                .header(
                                    http::header::AUTHORIZATION,
                                    format!("Bearer {USER_01_SESSION}"),
                                )
                                .header(axum::http::header::CONTENT_TYPE, "application/json")
                                .body(
                                    serde_json::to_vec(
                                        &serde_json::json!({
                                            "afterCursor": resp_body_json["cursor"]
                                                                .as_str()
                                                                .unwrap()
                                        })
                                    ).unwrap().into()
                                )
                                .unwrap_or_log(),
                        )
                        .await
                        .unwrap_or_log();
                    let (head, body) = resp.into_parts();
                    let body = hyper::body::to_bytes(body).await.unwrap_or_log();
                    let body: serde_json::Value = serde_json::from_slice(&body).unwrap_or_log();
                    assert_eq!(head.status, StatusCode::OK, "{head:?} {body:?}");
                    assert_eq!(
                        body["items"].as_array().unwrap().len(),
                        2,
                        "{resp_body_json:?}\n{body:?}"
                    );
                    assert!(
                        body["cursor"].is_null(),
                        "{resp_body_json:?}\n{body:?}"
                    );
                })
            },
        },
    }
}
