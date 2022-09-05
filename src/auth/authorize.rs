//! Check if the auth policy allows the provded [`User`] is allowed
//! to perform the provided [`Action`] on the provided [`Resource`]

use deps::*;

use crate::auth::{Action, Resource};

#[derive(Clone, Copy, Debug)]
pub struct Authorize;

#[derive(Debug)]
pub struct Request {
    pub token: std::sync::Arc<str>,
    pub resource: Resource,
    pub action: Action,
}

#[derive(Debug, thiserror::Error, serde::Serialize, utoipa::ToSchema)]
#[serde(crate = "serde", tag = "error", rename_all = "camelCase")]
pub enum Error {
    #[error("unauthorized")]
    Unauthorized,
    #[error("invalid token")]
    InvalidToken,
    #[error("internal server error: {message:?}")]
    Internal { message: String },
}

#[async_trait::async_trait]
impl crate::Endpoint for Authorize {
    type Request = Request;
    type Response = uuid::Uuid;
    type Error = Error;

    #[tracing::instrument(skip(ctx))]
    async fn handle(
        &self,
        ctx: &crate::Context,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        let session = sqlx::query_as!(
            super::Session,
            r#"
SELECT * 
FROM sessions
WHERE token = $1
            "#,
            &request.token[..]
        )
        .fetch_one(&ctx.db_pool)
        .await
        .map_err(|err| match err {
            sqlx::Error::RowNotFound => Error::InvalidToken,
            _ => Error::Internal {
                message: format!("{err}"),
            },
        })?;
        if session.expires_at < time::OffsetDateTime::now_utc() {
            return Err(Error::InvalidToken);
        }
        Ok(session.user_id)
    }
}

#[cfg(test)]
mod tests {
    // use deps::*;

    use crate::auth::*;
    use crate::utils::testing::*;
    use crate::{user::testing::*, Endpoint};

    crate::table_tests! {
        authorize_policy tokio,
        (username, id, resource_actions),
        {
            let ctx = TestContext::new(crate::function!()).await;
            {
                let res = authenticate::Authenticate.handle(&ctx.ctx(), authenticate::Request{
                    username: Some(username.into()),
                    email: None,
                    password: "password".into()
                }).await.unwrap_or_log();
                for (resource, action) in resource_actions {
                    let user_id = authorize::Authorize.handle(&ctx.ctx(), authorize::Request {
                        token: res.token.clone().into(),
                        resource,
                        action
                    }).await.unwrap_or_log();
                    assert_eq!(id, user_id.to_string());
                }
            }
            ctx.close().await;
        },
    }

    authorize_policy! {
        allows_any_action_on_own_account: (
            USER_01_USERNAME,
            USER_01_ID,
            {
                [
                    Resource::User { id: USER_01_ID.parse().unwrap() }
                ]
                .into_iter()
                .flat_map(|res| {
                    let res = res.clone();
                    [
                        Action::Read,
                        Action::Write,
                        Action::Delete
                    ]
                    .into_iter()
                    .map(move |act| (res.clone(), act))
                }).collect::<Vec<_>>()
            }
        ),
    }
}
