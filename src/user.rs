use deps::*;

use crate::{DocumentedEndpoint, EndpointWrapper, HttpEndpoint};

#[derive(serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct User {
    pub id: uuid::Uuid,
    /// In seconds since unix epoch in UTC.
    #[schema(example = 1234567)]
    #[serde(with = "time::serde::timestamp")]
    pub created_at: time::OffsetDateTime,
    /// In seconds since unix epoch in UTC.
    #[schema(example = 1234567)]
    #[serde(with = "time::serde::timestamp")]
    pub updated_at: time::OffsetDateTime,
    #[schema(example = "alice@example.com")]
    pub email: String,
    #[schema(example = "hunter2")]
    pub username: String,
    pub pic_url: Option<String>,
}

pub const TAG: crate::Tag = crate::Tag {
    name: "user",
    desc: "Manipulate User objects.",
};

mod get;
mod create;

pub fn router() -> axum::Router {
    axum::Router::new()
        .merge(EndpointWrapper::new(get::GetUser))
        .merge(EndpointWrapper::new(create::CreateUser))
}

pub fn components(
    builder: utoipa::openapi::ComponentsBuilder,
) -> utoipa::openapi::ComponentsBuilder {
    let builder = get::GetUser::components(builder);
    create::CreateUser::components(builder)
}

pub fn paths(builder: utoipa::openapi::PathsBuilder) -> utoipa::openapi::PathsBuilder {
    builder.path(
        crate::axum_path_str_to_openapi(get::GetUser::PATH),
        get::GetUser::path_item(),
    )
}

// #[cfg(test)]
pub mod testing {
    pub const USER_01_ID: &str = "add83cdf-2ab3-443f-84dd-476d7984cf75";
    pub const USER_02_ID: &str = "ce4fe993-04d6-462e-af1d-d734fcc9639d";
    pub const USER_03_ID: &str = "d437e73f-4610-462c-ab22-f94b76bba83a";
    pub const USER_04_ID: &str = "68cf4d43-62d2-4202-8c50-c79a5f4dd1cc";

    pub const USER_01_USERNAME: &str = "sabrina";
    pub const USER_02_USERNAME: &str = "archie";
    pub const USER_03_USERNAME: &str = "betty";
    pub const USER_04_USERNAME: &str = "veronica";

    pub const USER_01_EMAIL: &str = "hex.queen@teen.dj";
    pub const USER_02_EMAIL: &str = "archie1941@poetry.ybn";
    pub const USER_03_EMAIL: &str = "pInXy@melt.shake";
    pub const USER_04_EMAIL: &str = "trekkiegirl@ln.pi";
}
