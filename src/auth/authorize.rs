//! Check if the auth policy allows the provded [`User`] is allowed
//! to perform the provided [`Action`] on the provided [`Resource`]

use deps::*;

use crate::auth::{Action, Resource};
use crate::*;

#[derive(Clone, Copy, Debug)]
pub struct Authorize;

#[async_trait::async_trait]
impl crate::Endpoint for Authorize {
    type Request = Request;
    type Response = bool;
    type Error = Error;

    #[tracing::instrument(skip(ctx))]
    async fn handle(
        &self,
        ctx: &crate::Context,
        id: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        return todo!();
        //                 sqlx::query_as!(
        //                     User,
        //                     r#"
        // SELECT id,
        //     created_at,
        //     updated_at,
        //     email::TEXT as "email!",
        //     username::TEXT as "username!",
        //     pic_url
        // FROM users
        // WHERE id = $1::uuid
        //             "#,
        //                     &id
        //                 )
        //                 .fetch_one(&ctx.db_pool)
        //                 .await
        //                 .map_err(|err| match err {
        //                     sqlx::Error::RowNotFound => Error::NotFound { id },
        //                     _ => Error::Internal {
        //                         message: format!("{err}"),
        //                     },
        //                 })
    }
}

#[derive(Debug)]
pub struct Request {
    pub user_id: uuid::Uuid,
    pub resource: Resource,
    pub action: Action,
}

#[derive(Debug, thiserror::Error, serde::Serialize, utoipa::ToSchema)]
#[serde(crate = "serde", tag = "error", rename_all = "camelCase")]
pub enum Error {
    #[error("internal server error: {message:?}")]
    Internal { message: String },
}

#[cfg(test)]
mod tests {
    use deps::*;

    use crate::user::testing::*;
    use crate::utils::testing::*;

    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn check_policy_works() {
        let ctx = TestContext::new(crate::function!()).await;
        {}
        ctx.close().await;
    }
}

