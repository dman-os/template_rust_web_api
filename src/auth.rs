use deps::*;

use crate::{DocumentedEndpoint, EndpointWrapper, HttpEndpoint};

#[derive(Debug)]
pub enum Resource {
    User,
    Users,
}

#[derive(Debug)]
pub enum Action {
    Read,
    Write,
    Delete,
}

#[derive(Debug)]
pub enum Role {
    SuperAdmin,
}

pub const TAG: crate::Tag = crate::Tag {
    name: "auth",
    desc: "The authentication and authorization services.",
};

pub mod authenticate;
pub mod authorize;

pub fn router() -> axum::Router {
    axum::Router::new()
        .merge(EndpointWrapper::new(authenticate::Authenticate))
}

pub fn components(
    builder: utoipa::openapi::ComponentsBuilder,
) -> utoipa::openapi::ComponentsBuilder {
    let builder = authenticate::Authenticate::components(builder);
    builder
}

pub fn paths(builder: utoipa::openapi::PathsBuilder) -> utoipa::openapi::PathsBuilder {
    builder.path(
        crate::axum_path_str_to_openapi(authenticate::Authenticate::PATH),
        authenticate::Authenticate::path_item(),
    )
}
