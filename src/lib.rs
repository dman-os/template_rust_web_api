#![allow(clippy::single_component_path_imports, clippy::let_and_return)]

#[cfg(feature = "dylink")]
#[allow(unused_imports)]
use dylink;

use deps::*;

pub mod auth;
pub mod macros;
pub mod user;
pub mod utils;

use crate::utils::*;

use std::future::Future;

pub(crate) use axum::http::StatusCode;
use axum::{
    extract::*,
    response::{self, IntoResponse},
};
use utoipa::openapi;

pub fn setup_tracing() -> eyre::Result<()> {
    color_eyre::install()?;
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "info");
    }

    tracing_subscriber::fmt()
        // .pretty()
        .compact()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .try_init()
        .map_err(|err| eyre::eyre!(err))?;

    Ok(())
}

#[derive(Debug)]
pub struct Config {
    pub pass_salt_hash: Vec<u8>,
    pub argon2_conf: argon2::Config<'static>,
    pub auth_token_lifespan: time::Duration,
}

#[derive(Debug)]
pub struct Context {
    pub db_pool: sqlx::postgres::PgPool,
    pub config: Config,
}

pub type SharedContext = std::sync::Arc<Context>;

shadow_rs::shadow!(build);

pub struct ApiDoc;
impl utoipa::OpenApi for ApiDoc {
    fn openapi() -> openapi::OpenApi {
        let mut openapi = openapi::OpenApiBuilder::new()
            .info(
                openapi::InfoBuilder::new()
                    .title(build::PROJECT_NAME)
                    .version(build::PKG_VERSION)
                    .description(Some(format!(
                        r#"{}

Notes:
- Time values are integers despite the `string($date-time)` type shown here.
                        "#,
                        build::PKG_DESCRIPTION
                    )))
                    .build(),
            )
            .paths({
                let builder = openapi::path::PathsBuilder::new();
                let builder = user::paths(builder);
                let builder = auth::paths(builder);
                builder.build()
            })
            .components(Some({
                let builder = openapi::ComponentsBuilder::new();
                let builder = builder
                    .schema(
                        type_name_raw::<SortingOrder>(),
                        <SortingOrder as utoipa::ToSchema>::schema(),
                    )
                    .schema(
                        type_name_raw::<ValidationErrors>(),
                        <utils::ValidationErrors as utoipa::ToSchema>::schema(),
                    )
                    .schema(
                        type_name_raw::<ValidationErrorsKind>(),
                        <utils::ValidationErrorsKind as utoipa::ToSchema>::schema(),
                    )
                    .schema(
                        type_name_raw::<ValidationError>(),
                        <utils::ValidationError as utoipa::ToSchema>::schema(),
                    );
                let builder = user::components(builder);
                let builder = auth::components(builder);
                builder.build()
            }))
            .tags(Some([
                auth::TAG.into(),
                user::TAG.into(),
                DEFAULT_TAG.into(),
            ]))
            .build();
        if let Some(components) = openapi.components.as_mut() {
            use utoipa::openapi::security::*;
            components.add_security_scheme(
                "bearer",
                SecurityScheme::Http(openapi::security::Http::new(
                    openapi::security::HttpAuthScheme::Bearer,
                )),
            )
        }
        openapi
    }
}

#[async_trait::async_trait]
pub trait Endpoint: Send + Sync + 'static {
    type Request: Send + Sync + 'static;
    type Response;
    type Error;

    async fn handle(
        &self,
        ctx: &crate::Context,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error>;
}

#[async_trait::async_trait]
pub trait AuthenticatedEndpoint: Send + Sync + 'static {
    type Request: Send + Sync + 'static;
    type Response;
    type Error: From<auth::authorize::Error>;

    fn authorize_request(&self, request: &Self::Request) -> crate::auth::authorize::Request;

    async fn handle(
        &self,
        ctx: &crate::Context,
        accessing_user: uuid::Uuid,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error>;
}

// pub enum AuthenticatedEndpointError<E> {
//     AuthenticationError(E),
//     EndpointError(E)
// }

#[async_trait::async_trait]
impl<T> Endpoint for T
where
    T: AuthenticatedEndpoint,
{
    type Request = T::Request;
    type Response = T::Response;
    type Error = T::Error;

    async fn handle(
        &self,
        ctx: &crate::Context,
        request: Self::Request,
    ) -> Result<Self::Response, Self::Error> {
        let accessing_user = {
            let auth_args = self.authorize_request(&request);
            crate::auth::authorize::Authorize
                .handle(ctx, auth_args)
                .await?
        };
        self.handle(ctx, accessing_user, request).await
    }
}

pub trait HttpEndpoint: Endpoint + Clone
where
    Self::Error: serde::Serialize,
    for<'a> &'a Self::Error: Into<StatusCode>,
{
    const METHOD: Method;
    const PATH: &'static str;
    type HttpRequest: axum::extract::FromRequest<axum::body::Body> + Send + Sync + 'static;
    // FIXME: this is superflous and can be covered by the `response` call
    const SUCCESS_CODE: StatusCode = StatusCode::OK;
    // type HttpResponse: axum::response::IntoResponse;

    /// TODO: consider making this a `From` trait bound on `Self::Parameters`
    fn request(params: Self::HttpRequest) -> Result<Self::Request, Self::Error>;
    fn response(resp: Self::Response) -> axum::response::Response;

    /// This actally need not be a method but I guess it allows for easy behavior
    /// modification. We ought to probably move these to the `Handler` impl
    /// when they stabilize specialization
    fn http(
        &self,
        req: hyper::Request<hyper::Body>,
    ) -> std::pin::Pin<Box<dyn Future<Output = axum::response::Response> + Send>> {
        let this = self.clone();
        Box::pin(async move {
            let mut req_parts = axum::extract::RequestParts::new(req);
            let req = match Self::HttpRequest::from_request(&mut req_parts).await {
                Ok(val) => val,
                Err(err) => return err.into_response(),
            };
            let req = match Self::request(req) {
                Ok(val) => val,
                Err(err) => {
                    return (Into::<StatusCode>::into(&err), response::Json(err)).into_response()
                }
            };
            let Extension(ctx) =
                match Extension::<crate::SharedContext>::from_request(&mut req_parts).await {
                    Ok(val) => val,
                    Err(err) => return err.into_response(),
                };
            // we have to clone it or the borrow checker biches that &T is
            match this.handle(&ctx, req).await {
                // Ok(ok) => Into::<Self::HttpResponse>::into(ok).into_response(),
                Ok(ok) => {
                    let mut resp = Self::response(ok);
                    *resp.status_mut() = Self::SUCCESS_CODE;
                    resp
                }
                Err(err) => (Into::<StatusCode>::into(&err), response::Json(err)).into_response(),
            }
        })
    }
}
pub struct Tag {
    name: &'static str,
    desc: &'static str,
}

impl From<Tag> for openapi::Tag {
    fn from(tag: Tag) -> Self {
        openapi::tag::TagBuilder::new()
            .name(tag.name)
            .description(Some(tag.desc))
            .build()
    }
}

pub const DEFAULT_TAG: Tag = Tag {
    name: "api",
    desc: "This is the catch all tag.",
};

pub fn axum_path_str_to_openapi(path: &str) -> String {
    path.split('/')
        .filter(|s| !s.is_empty())
        .map(|s| {
            if &s[0..1] == ":" {
                format!("/{{{}}}", &s[1..])
            } else {
                format!("/{s}")
            }
        })
        .collect()
}

#[test]
fn test_axum_path_str_to_openapi() {
    for (expected, path) in [
        ("/users/{id}", "/users/:id"),
        ("/users/{id}/resource/{resID}", "/users/:id/resource/:resID"),
    ] {
        assert_eq!(
            expected,
            &axum_path_str_to_openapi(path)[..],
            "failed on {path}"
        );
    }
}

pub fn axum_path_parameter_list(path: &str) -> Vec<String> {
    path.split('/')
        .filter(|s| !s.is_empty())
        .filter(|s| &s[0..1] == ":")
        .map(|s| s[1..].to_string())
        .collect()
}

#[test]
fn test_axum_path_paramter_list() {
    for (expected, path) in [
        (vec!["id".to_string()], "/users/:id"),
        (
            vec!["id".to_string(), "resID".to_string()],
            "/users/:id/resource/:resID",
        ),
    ] {
        assert_eq!(
            expected,
            &axum_path_parameter_list(path)[..],
            "failed on {path}"
        );
    }
}

pub trait ToRefOrSchema {
    fn schema_name() -> &'static str;
    fn ref_or_schema() -> openapi::schema::RefOr<openapi::schema::Schema>;
}

impl<T> ToRefOrSchema for T
where
    T: utoipa::ToSchema,
{
    fn ref_or_schema() -> openapi::schema::RefOr<openapi::schema::Schema> {
        T::schema().into()
    }

    fn schema_name() -> &'static str {
        type_name_raw::<T>()
    }
}

pub struct NoContent;

impl From<()> for NoContent {
    fn from(_: ()) -> Self {
        Self
    }
}

impl ToRefOrSchema for NoContent {
    fn schema_name() -> &'static str {
        type_name_raw::<NoContent>()
    }

    fn ref_or_schema() -> openapi::schema::RefOr<openapi::schema::Schema> {
        panic!("this baby is special cased")
    }
}

#[derive(educe::Educe, serde::Serialize, serde::Deserialize)]
#[serde(crate = "serde")]
#[educe(Deref, DerefMut)]
pub struct Ref<T>(pub T);

impl<T> From<T> for Ref<T> {
    fn from(inner: T) -> Self {
        Self(inner)
    }
}

impl<T> ToRefOrSchema for Ref<T>
where
    T: utoipa::ToSchema + serde::Serialize,
{
    fn ref_or_schema() -> openapi::schema::RefOr<openapi::schema::Schema> {
        openapi::schema::Ref::from_schema_name(type_name_raw::<T>()).into()
        // utoipa::openapi::ObjectBuilder::new()
        //     .property(
        //         "$ref",
        //         utoipa::openapi::schema::Ref::from_schema_name(T::type_name_raw()),
        //     )
        //     .into()
    }

    fn schema_name() -> &'static str {
        T::schema_name()
    }
}

pub enum ParameterDoc {
    Param(Box<openapi::path::Parameter>),
    Body(Box<openapi::request_body::RequestBody>),
}

impl From<openapi::path::Parameter> for ParameterDoc {
    fn from(param: openapi::path::Parameter) -> Self {
        Self::Param(Box::new(param))
    }
}

impl From<openapi::request_body::RequestBody> for ParameterDoc {
    fn from(body: openapi::request_body::RequestBody) -> Self {
        Self::Body(Box::new(body))
    }
}

pub trait DocumentedParameter {
    // FIXME:: yikes
    const HAS_BEARER: bool = false;
    fn to_openapi(op_id: &str, path: &str) -> Vec<ParameterDoc>;
}

// impl<T> DocumentedParameter for axum::extract::Path<T> {
//     fn to_openapi(_op_id: &str, path: &str) -> Vec<ParameterDoc> {
//         axum_path_parameter_list(path)
//             .into_iter()
//             .map(|name| {
//                 openapi::path::ParameterBuilder::new()
//                     .name(name)
//                     .parameter_in(openapi::path::ParameterIn::Path)
//                     .required(openapi::Required::True)
//                     .build()
//                     .into()
//             })
//             .collect()
//     }
// }

impl DocumentedParameter for axum::extract::Path<uuid::Uuid> {
    fn to_openapi(_op_id: &str, path: &str) -> Vec<ParameterDoc> {
        axum_path_parameter_list(path)
            .into_iter()
            .map(|name| {
                openapi::path::ParameterBuilder::new()
                    .name(name)
                    .parameter_in(openapi::path::ParameterIn::Path)
                    .required(openapi::Required::True)
                    .schema(Some(
                        openapi::schema::ObjectBuilder::new()
                            .schema_type(openapi::SchemaType::String)
                            .format(Some(openapi::schema::SchemaFormat::Uuid)),
                    ))
                    .build()
                    .into()
            })
            .collect()
    }
}

impl<T> DocumentedParameter for axum::extract::Json<T>
where
    T: ToRefOrSchema,
{
    fn to_openapi(_op_id: &str, _path: &str) -> Vec<ParameterDoc> {
        vec![utoipa::openapi::request_body::RequestBodyBuilder::new()
            .content(
                "application/json",
                utoipa::openapi::ContentBuilder::new()
                    .schema(match T::ref_or_schema() {
                        utoipa::openapi::schema::RefOr::T(schema) => {
                            if T::schema_name() == "Request" {
                                schema.into()
                            } else {
                                utoipa::openapi::Ref::from_schema_name(T::schema_name().to_string())
                                    .into()
                            }
                        }
                        ref_or => ref_or,
                    })
                    .build(),
            )
            // .name("body")
            // .parameter_in(utoipa::openapi::path::ParameterIn::Path)
            // .required(utoipa::openapi::Required::True)
            .build()
            .into()]
    }
}

// impl<T> DocumentedParameter for axum::extract::Query<T>
// where
//     T: utoipa::ToSchema,
// {
//     fn to_openapi(_op_id: &str, _path: &str) -> Vec<ParameterDoc> {
//         match T::schema() {
//             utoipa::openapi::Schema::Object(obj) => {
//
//             },
//             utoipa::openapi::Schema::Array(_) => panic!("{} is an Array schema: not allowed as Query paramter", std::any::type_name::<T>()),
//             utoipa::openapi::Schema::OneOf(_) => panic!("{} is an OneOf schema: not allowed as Query paramter", std::any::type_name::<T>()),
//             _ => todo!(),
//         }
//         vec![utoipa::openapi::path::ParameterBuilder::new().schema({
//             .schema(match T::ref_or_schema() {
//                 utoipa::openapi::schema::RefOr::T(schema) => {
//                     if T::schema_name() == "Request" {
//                         schema.into()
//                     } else {
//                         utoipa::openapi::Ref::from_schema_name(T::schema_name().to_string())
//                             .into()
//                     }
//                 }
//                 ref_or => ref_or,
//             })
//         })
//             // .name("body")
//             // .parameter_in(utoipa::openapi::path::ParameterIn::Path)
//             // .required(utoipa::openapi::Required::True)
//             .build()
//             .into()]
//     }
// }

impl<T> DocumentedParameter for Option<T>
where
    T: DocumentedParameter,
{
    const HAS_BEARER: bool = T::HAS_BEARER;
    fn to_openapi(op_id: &str, path: &str) -> Vec<ParameterDoc> {
        let mut vec = T::to_openapi(op_id, path);
        for param in &mut vec {
            match param {
                ParameterDoc::Param(param) => {
                    param.required = openapi::Required::False;
                }
                ParameterDoc::Body(body) => {
                    body.required = Some(openapi::Required::False);
                }
            }
        }
        vec
    }
}
impl DocumentedParameter for () {
    fn to_openapi(_op_id: &str, _path: &str) -> Vec<ParameterDoc> {
        vec![]
    }
}

impl<T> DocumentedParameter for (T,)
where
    T: DocumentedParameter,
{
    const HAS_BEARER: bool = T::HAS_BEARER;
    fn to_openapi(op_id: &str, path: &str) -> Vec<ParameterDoc> {
        T::to_openapi(op_id, path)
    }
}

impl<T1, T2> DocumentedParameter for (T1, T2)
where
    T1: DocumentedParameter,
    T2: DocumentedParameter,
{
    const HAS_BEARER: bool = T1::HAS_BEARER | T2::HAS_BEARER;
    fn to_openapi(op_id: &str, path: &str) -> Vec<ParameterDoc> {
        let mut vec = T1::to_openapi(op_id, path);
        vec.append(&mut T2::to_openapi(op_id, path));
        vec
    }
}

impl<T1, T2, T3> DocumentedParameter for (T1, T2, T3)
where
    T1: DocumentedParameter,
    T2: DocumentedParameter,
    T3: DocumentedParameter,
{
    const HAS_BEARER: bool = T1::HAS_BEARER | T2::HAS_BEARER | T3::HAS_BEARER;
    fn to_openapi(op_id: &str, path: &str) -> Vec<ParameterDoc> {
        let mut vec = T1::to_openapi(op_id, path);
        vec.append(&mut T2::to_openapi(op_id, path));
        vec.append(&mut T3::to_openapi(op_id, path));
        vec
    }
}

/// (description, example)
pub type ErrorResponse<Err> = (&'static str, Err);

pub trait DocumentedEndpoint: HttpEndpoint + Sized
where
    Self::Response: ToRefOrSchema,
    Self::Error: ToRefOrSchema + serde::Serialize,
    for<'a> &'a Self::Error: Into<StatusCode>,
    Self::HttpRequest: DocumentedParameter,
{
    const TAG: &'static Tag = &DEFAULT_TAG;
    const SUMMARY: &'static str = "";
    const DESCRIPTION: &'static str = "";
    const SUCCESS_DESCRIPTION: &'static str = "";
    const DEPRECATED: bool = false;

    /// By default, this calls [`utils::type_name_raw`] on `Self`.
    fn id() -> &'static str {
        type_name_raw::<Self>()
    }

    /// Provide examples to be used for the error responses
    /// generated by [`error_responses`]. Each example will
    /// be treated as a separate response keyed under the result
    /// of calling [`Into`] [`StatusCode`] on it. This is optimized
    /// for the enum error design.
    fn errors() -> Vec<ErrorResponse<Self::Error>>;

    /// Provide examples to be used for the success response
    /// generated by [`success_responses`]. These examples will
    /// be used for a single Response object keyed under [`HttpEndpoint::SUCCESS_CODE`]
    fn success_examples() -> Vec<serde_json::Value> {
        vec![]
    }

    /// Read at `success_examples` for the default behavior.
    fn success_responses() -> Vec<(String, openapi::Response)> {
        vec![(Self::SUCCESS_CODE.as_u16().to_string(), {
            let builder = if Self::Response::schema_name() != type_name_raw::<NoContent>() {
                openapi::ResponseBuilder::new().content("application/json", {
                    let mut schema = match Self::Response::ref_or_schema() {
                        // if it's a `Ref`, use the `schema_name`
                        openapi::schema::RefOr::Ref(_) => openapi::ContentBuilder::new()
                            .schema(openapi::Ref::from_schema_name(Self::Response::schema_name())),
                        // else, assume generic name
                        openapi::schema::RefOr::T(schema) => {
                            openapi::ContentBuilder::new()
                                // .schema(utoipa::openapi::Ref::from_schema_name(
                                //     format!("{id}Response"),
                                // ))
                                .schema(schema)
                        }
                    };
                    for example in Self::success_examples() {
                        schema = schema.example(Some(serde_json::to_value(example).unwrap()))
                    }
                    schema.build()
                })
            } else {
                openapi::ResponseBuilder::new()
            };

            let builder = if !Self::SUCCESS_DESCRIPTION.is_empty() {
                builder.description(Self::SUCCESS_DESCRIPTION)
            } else {
                builder
            };
            builder.build()
        })]
    }

    /// Besides what's stated in the doc of [`errors`], the default impl assumes that
    /// the `Error` type schema is registered as a component under `EndpointIdError`
    /// endpoint id coming from [`DocumentedEndpoint::id`]
    fn error_responses() -> Vec<(String, openapi::Response)> {
        let id = Self::id();
        Self::errors()
            .into_iter()
            .map(|(desc, example)| {
                (
                    Into::<StatusCode>::into(&example).as_u16().to_string(),
                    openapi::ResponseBuilder::new()
                        .description(desc)
                        .content(
                            "application/json",
                            openapi::ContentBuilder::new()
                                .schema(utoipa::openapi::Ref::from_schema_name(format!(
                                    "{id}Error"
                                )))
                                // .schema(Self::Error::ref_or_schema())
                                .example(Some(serde_json::to_value(example).unwrap()))
                                .build(),
                        )
                        .build(),
                )
            })
            .collect()
    }

    /// Makes use of [`success_responses`] and [`default_responses`].
    fn responses() -> openapi::Responses {
        let builder = openapi::ResponsesBuilder::new();
        let builder = builder.responses_from_iter(Self::success_responses().into_iter());
        let builder = builder.responses_from_iter(Self::error_responses().into_iter());
        builder.build()
    }

    fn paramters() -> (
        Option<openapi::request_body::RequestBody>,
        Vec<openapi::path::Parameter>,
    ) {
        let id = Self::id();
        let (params, bodies) = Self::HttpRequest::to_openapi(id, Self::PATH)
            .into_iter()
            .fold((vec![], vec![]), |(mut params, mut bodies), doc| {
                match doc {
                    ParameterDoc::Param(param) => {
                        params.push(*param);
                    }
                    ParameterDoc::Body(body) => {
                        bodies.push(*body);
                    }
                }
                (params, bodies)
            });
        assert!(bodies.len() < 2, "{id} has more than one Body ParameterDoc");
        (bodies.into_iter().next(), params)
    }

    fn path_item() -> openapi::PathItem {
        let id = Self::id();
        let (body, params) = Self::paramters();
        openapi::PathItem::new(
            Self::METHOD,
            openapi::path::OperationBuilder::new()
                .operation_id(Some(id))
                .deprecated(Some(if Self::DEPRECATED {
                    openapi::Deprecated::True
                } else {
                    openapi::Deprecated::False
                }))
                .summary(if !Self::SUMMARY.is_empty() {
                    Some(Self::SUMMARY)
                } else {
                    None
                })
                .description(if !Self::DESCRIPTION.is_empty() {
                    Some(Self::DESCRIPTION)
                } else {
                    None
                })
                .tag(Self::TAG.name)
                .securities(if Self::HttpRequest::HAS_BEARER {
                    Some([openapi::security::SecurityRequirement::new::<
                        &str,
                        [&str; 1usize],
                        &str,
                    >("bearer", [""])])
                } else {
                    None
                })
                .request_body(body)
                .parameters(Some(params.into_iter()))
                .responses(Self::responses()),
        )
    }

    /// Registers the [`Error`] type schema under `EndpointIdError` name using the
    /// id provided at [`DocumentedEndpoint::id`]
    fn default_components(builder: openapi::ComponentsBuilder) -> openapi::ComponentsBuilder {
        let id = Self::id();
        // let (_, bodies) = Self::Parameters::to_openapi(id, Self::PATH)
        //     .into_iter()
        //     .fold((vec![], vec![]), |(mut params, mut bodies), doc| {
        //         match doc {
        //             ParameterDoc::Param(param) => {
        //                 params.push(param);
        //             }
        //             ParameterDoc::Body(body) => {
        //                 bodies.push(body);
        //             }
        //         }
        //         (params, bodies)
        //     });
        [
            // (
            //     format!("{id}Response"),
            //     <Self::Response as ToRefOrSchema>::ref_or_schema(),
            // ),
            (
                format!("{id}Error"),
                <Self::Error as ToRefOrSchema>::ref_or_schema(),
            ),
        ]
        .into_iter()
        .fold(builder, |builder, (name, ref_or)| match ref_or {
            // assume the component has been added elsewhere
            utoipa::openapi::schema::RefOr::Ref(_) => builder,
            utoipa::openapi::schema::RefOr::T(schema) => builder.schema(name, schema),
        })
    }

    fn components(builder: openapi::ComponentsBuilder) -> openapi::ComponentsBuilder {
        Self::default_components(builder)
    }
}

pub type Method = openapi::PathItemType;

// pub struct DocParameterBuilder {
//     inner: utoipa::openapi::path::ParameterBuilder,
// }
// pub enum ParamExample<T> {
//     Query(T),
//     Path(T),
//     Header(T),
//     Cookie(T),
// }
// impl DocParameterBuilder {
//     pub fn new<T>(name: &'static str, example: ) -> Self {
//         Self {
//             inner: utoipa::openapi::path::ParameterBuilder::new().name(name)
//         }
//     }
//     pub fn build(self: Self) -> Parameter {
//         todo!()
//     }
// }

/// This is used to get around Rust orphaning rules. This allow us
/// to implement any foreign traits lik `axum::handler::Handler` for any `T`
/// that implements `Endpoint`
#[derive(educe::Educe)]
#[educe(Deref, DerefMut)]
pub struct EndpointWrapper<T> {
    inner: T,
}

impl<T> EndpointWrapper<T>
where
    T: HttpEndpoint + Clone + Send + Sized + 'static,
    T::Error: serde::Serialize,
    for<'a> &'a T::Error: Into<StatusCode>,
{
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

impl<T> Clone for EndpointWrapper<T>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl<T> axum::handler::Handler<T::Request> for EndpointWrapper<T>
where
    T: HttpEndpoint + Clone,
    T::Error: serde::Serialize,
    for<'a> &'a T::Error: Into<StatusCode>,
{
    type Future = std::pin::Pin<Box<dyn Future<Output = axum::response::Response> + Send>>;

    fn call(self, req: hyper::Request<hyper::Body>) -> Self::Future {
        self.http(req)
    }
}

impl<T> From<EndpointWrapper<T>> for axum::Router
where
    T: HttpEndpoint + Clone,
    T::Error: serde::Serialize,
    for<'a> &'a T::Error: Into<StatusCode>,
{
    fn from(wrapper: EndpointWrapper<T>) -> Self {
        use utoipa::openapi::PathItemType;
        let method = match T::METHOD {
            PathItemType::Get => axum::routing::get(wrapper),
            PathItemType::Post => axum::routing::post(wrapper),
            PathItemType::Put => axum::routing::put(wrapper),
            PathItemType::Delete => axum::routing::delete(wrapper),
            PathItemType::Options => axum::routing::options(wrapper),
            PathItemType::Head => axum::routing::head(wrapper),
            PathItemType::Patch => axum::routing::patch(wrapper),
            PathItemType::Trace => axum::routing::trace(wrapper),
            PathItemType::Connect => todo!(),
        };
        axum::Router::new().route(T::PATH, method)
    }
}

impl<T> utoipa::Path for EndpointWrapper<T>
where
    T: DocumentedEndpoint,
    T::Request: axum::extract::FromRequest<axum::body::Body>,
    T::Response: utoipa::ToSchema,
    T::Error: utoipa::ToSchema + serde::Serialize,
    for<'a> &'a T::Error: Into<StatusCode>,
    T::HttpRequest: DocumentedParameter,
{
    fn path() -> &'static str {
        <T as HttpEndpoint>::PATH
    }

    fn path_item(_: Option<&str>) -> openapi::path::PathItem {
        <T as DocumentedEndpoint>::path_item()
    }
}

pub struct BearerToken(pub std::sync::Arc<str>);

#[async_trait::async_trait]
impl<B> axum::extract::FromRequest<B> for BearerToken
where
    B: Send,
{
    type Rejection = axum::response::Response;

    async fn from_request(req: &mut RequestParts<B>) -> Result<Self, Self::Rejection> {
        let header = req
            .headers()
            .get(axum::http::header::AUTHORIZATION)
            .ok_or_else(|| {
                (StatusCode::UNAUTHORIZED, "Authorization header not set").into_response()
            })?;
        if &header.as_bytes()[0..7] != b"Bearer " {
            return Err((
                StatusCode::UNAUTHORIZED,
                "Authorization header is not Bearer token",
            )
                .into_response());
        }
        let token = &header.as_bytes()[7..];
        let token = std::str::from_utf8(token).map_err(|_| {
            (StatusCode::UNAUTHORIZED, "Bearer token not valid utf-8").into_response()
        })?;
        Ok(Self(std::sync::Arc::from(token)))
    }
}

impl DocumentedParameter for BearerToken {
    const HAS_BEARER: bool = true;
    fn to_openapi(_op_id: &str, _path: &str) -> Vec<ParameterDoc> {
        vec![]
    }
}
