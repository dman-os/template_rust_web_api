/// Implement [`From`] [`crate::auth::authorize::Error`] for the provided type
/// This expects the standard unit `AccessDenied` and the struct `Internal`
/// variant on the `Error` enum
#[macro_export]
macro_rules! impl_from_auth_err {
    ($errty:ident) => {
        impl From<$crate::auth::authorize::Error> for $errty {
            fn from(err: $crate::auth::authorize::Error) -> Self {
                use $crate::auth::authorize::Error;
                match err {
                    Error::Unauthorized | Error::InvalidToken => Self::AccessDenied,
                    Error::Internal { message } => Self::Internal { message },
                }
            }
        }
    };
}

/// Name of currently execution function
/// Resolves to first found in current function path that isn't a closure.
#[macro_export]
macro_rules! function {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        static FNAME: deps::once_cell::sync::Lazy<&'static str> =
            deps::once_cell::sync::Lazy::new(|| {
                let name = type_name_of(f);
                // cut out the `::f`
                let name = &name[..name.len() - 3];
                // eleimante closure name
                let name = name.trim_end_matches("::{{closure}}");

                // Find and cut the rest of the path
                let name = match &name.rfind(':') {
                    Some(pos) => &name[pos + 1..name.len()],
                    None => name,
                };
                name
            });
        *FNAME
    }};
}

/// Resolves to function path
#[macro_export]
macro_rules! function_full {
    () => {{
        fn f() {}
        fn type_name_of<T>(_: T) -> &'static str {
            std::any::type_name::<T>()
        }
        static FNAME: deps::once_cell::sync::Lazy<&'static str> =
            deps::once_cell::sync::Lazy::new(|| {
                let name = type_name_of(f);
                // cut out the `::f`
                let name = &name[..name.len() - 3];
                // eleimante closure name
                let name = name.trim_end_matches("::{{closure}}");

                // // Find and cut the rest of the path
                // let name = match &name.rfind(':') {
                //     Some(pos) => &name[pos + 1..name.len()],
                //     None => name,
                // };
                name
            });
        *FNAME
    }};
}

#[test]
fn test_function_macro() {
    assert_eq!("test_function_macro", function!())
}

/// Gives you a identifier that is equivalent to an escaped dollar_sign.
/// Without this helper, nested using the dollar operator in nested macros would be impossible.
/// ```rust,ignore
/// macro_rules! top_macro {
///     ($nested_macro_name:ident, $top_param:ident) => {
///         $crate::__with_dollar_sign! {
///             // $d parameter here is equivalent to dollar sign for nested macro. You can use any variable name
///             ($d:tt) => {
///                 macro_rules! $nested_macro_name {
///                     // Notice the extra space between $d and
///                     ($d($d nested_param:ident)*) => {
///                         // this very contrived example assumes $top_param is pointing to a Fn ofc.
///                         $d(
///                             $top_param.call($d nested_param);
///                         )*
///                     }
///                 }
///             }
///         }
///     };
/// fn print<T: std::format::Debug>(value: T) {
///     println("{?}", value);
/// }
/// top_macro!(curry, print);
/// curry!(11, 12, 13, 14); // all four values will be printed
/// ```
/// Lifted from: https://stackoverflow.com/a/56663823
#[doc(hidden)]
#[macro_export]
macro_rules! __with_dollar_sign {
    ($($body:tt)*) => {
        macro_rules! __with_dollar_sign_ { $($body)* }
        __with_dollar_sign_!($);
    }
}

#[macro_export]
macro_rules! optional_expr {
    ($exp:expr) => {
        Some($exp)
    };
    () => {
        None
    };
}

#[macro_export]
macro_rules! optional_ident {
    () => {};
    ($token1:ident) => {
        $token1
    };
    ($token1:ident, $($token2:ident,)+) => {
        $token1
    };
}

#[macro_export]
macro_rules! optional_token {
    () => {};
    ($token1:tt) => {
        $token1
    };
    ($token1:tt, $($token2:tt,)+) => {
        $token1
    };
}

/// Lifted from: https://stackoverflow.com/a/56663823
/// TODO: refactor along the lines of [C-EVOCATIVE](https://rust-lang.github.io/api-guidelines/macros.html#input-syntax-is-evocative-of-the-output-c-evocative)
#[macro_export]
macro_rules! table_tests {
    ($name:ident, $args:pat, $body:tt) => {
        $crate::__with_dollar_sign! {
            ($d:tt) => {
                macro_rules! $name {
                    (
                        $d($d pname:ident  $d([$d attrs:meta])*: $d values:expr,)*
                    ) => {
                        mod $name {
                            #![ allow( unused_imports ) ]
                            use super::*;
                            $d(
                                #[test]
                                $d(#[ $d attrs ])*
                                fn $d pname() {
                                    let $args = $d values;
                                    $body
                                }
                            )*
                        }
                    }
                }
            }
        }
    };
    (
        $name:ident tokio,
        $args:pat,
        $body:tt,
        $(enable_tracing: $enable_tracing:expr,)?
        $(multi_thread: $multi_thread:expr,)?
    ) => {
        $crate::__with_dollar_sign! {
            ($d:tt) => {
                macro_rules! $name {
                    (
                        $d($d pname:ident  $d([$d attrs:meta])*: $d values:expr,)*
                    ) => {
                        mod $name {
                            #![ allow( unused_imports ) ]
                            use super::*;
                            // FIXME: should we not assume deps?
                            use deps::{eyre, tokio, ResultExt};
                            $d(
                                #[test]
                                $d(#[ $d attrs ])*
                                fn $d pname() {
                                    let enable_tracing: Option<bool> = $crate::optional_expr!($($enable_tracing)?);
                                    // TODO: consider disabling tracing by default?
                                    let enable_tracing = enable_tracing.unwrap_or(true);
                                    if enable_tracing {
                                        $crate::utils::testing::setup_tracing_once();
                                    }
                                    let multi_thread: Option<bool> = $crate::optional_expr!($($multi_thread)?);
                                    let multi_thread = multi_thread.unwrap_or(false);
                                    let mut builder = if multi_thread{
                                        tokio::runtime::Builder::new_multi_thread()
                                    }else{
                                        tokio::runtime::Builder::new_current_thread()
                                    };
                                    let result = builder
                                        .enable_all()
                                        .build()
                                        .unwrap()
                                        .block_on(async {
                                            let $args = $d values;
                                            $body
                                            Ok::<_, eyre::Report>(())
                                        });
                                    if enable_tracing{
                                        result.unwrap_or_log();
                                    } else{
                                        result.unwrap();
                                    }
                                }
                            )*
                        }
                    }
                }
            }
        }
    };
    ($name:ident async_double, $args:pat, $init_body:tt, $cleanup_body:tt) => {
        $crate::__with_dollar_sign! {
            ($d:tt) => {
                macro_rules! $name {
                    ($d($d pname:ident: { $d values:expr, $d extra:tt, },)*) => {
                        mod $name {
                            #![ allow( unused_imports ) ]
                            use super::*;
                            $d(
                                #[actix_rt::test]
                                async fn $d pname() {
                                    let $args = $d values;

                                    $init_body

                                    $d extra

                                    $cleanup_body
                                }
                            )*
                        }
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! integration_table_tests {
    ($(
        $name:ident: {
            uri: $uri:expr,
            method: $method:expr,
            status: $status:expr,
            router: $router:expr,
            $(body: $json_body:expr,)?
            $(check_json: $check_json:expr,)?
            $(auth_token: $auth_token:expr,)?
            $(extra_assertions: $extra_fn:expr,)?
            $(print_response: $print_res:expr,)?
        },
    )*) => {
        $(
            #[allow(unused_variables)]
            #[tokio::test]
            async fn $name() {
                let mut ctx = $crate::utils::testing::TestContext::new($crate::function_full!()).await;
                {
                    let mut request = axum::http::Request::builder()
                                        .method($method)
                                        .uri($uri);

                    // let token = authenticate::Authenticate
                    //     .handle(
                    //         &ctx.ctx(),
                    //         authenticate::Request {
                    //             username: Some(USER_01_USERNAME.into()),
                    //             email: None,
                    //             password: "password".into(),
                    //         },
                    //     )
                    //     .await
                    //     .unwrap_or_log()
                    //     .token;
                    let token: Option<String> = $crate::optional_expr!($($auth_token)?);
                    if let Some(token) = token.as_ref() {
                        request = request
                                .header(axum::http::header::AUTHORIZATION, format!("Bearer {token}"));
                    }

                    let json: Option<serde_json::Value> = $crate::optional_expr!($($json_body)?);
                    let request = if let Some(json_body) = json {
                        request
                            .header(axum::http::header::CONTENT_TYPE, "application/json")
                            .body(serde_json::to_vec(&json_body).unwrap().into())
                            .unwrap_or_log()
                    } else {
                        request
                            .body(Default::default()).unwrap_or_log()
                    };

                    let app = $router.layer(axum::Extension(ctx.ctx()));
                    use tower::ServiceExt;
                    let res = app
                        .oneshot(request)
                        .await
                        .unwrap_or_log();

                    let (head, body) = res.into_parts();
                    let response_json: Option<serde_json::Value> = hyper::body::to_bytes(body)
                        .await
                        .ok()
                        .and_then(|body|
                            serde_json::from_slice(&body).ok()
                        );

                    let status_code = $status;
                    assert_eq!(
                        head.status,
                        status_code,
                        "response: {head:?}\n{response_json:?}"
                    );

                    let check_json: Option<serde_json::Value> = $crate::optional_expr!($($check_json)?);
                    if let Some(check_json) = check_json {
                        let response_json = response_json.as_ref().unwrap();
                        $crate::utils::testing::check_json(
                            ("check", &check_json),
                            ("response", &response_json)
                        );
                    }

                    let print_response: Option<bool> = $crate::optional_expr!($($print_res)?);
                    if let Some(true) = print_response {
                        tracing::info!(head = ?head, "reponse_json: {:#?}", response_json);
                    }

                    use $crate::utils::testing::{ExtraAssertions, EAArgs};
                    let extra_assertions: Option<&ExtraAssertions> = $crate::optional_expr!($($extra_fn)?);
                    if let Some(extra_assertions) = extra_assertions {
                        extra_assertions(EAArgs{
                            ctx: &mut ctx,
                            auth_token: token,
                            response_json,
                            response_head: head
                        }).await;
                    }
                }
                ctx.close().await;
            }
        )*
    }
}

// FIXME:
#[macro_export]
macro_rules! integration_table_tests_shorthand {
    (
        $sname:ident,
        $(uri: $suri:expr,)?
        $(method: $smethod:expr,)?
        $(status: $sstatus:expr,)?
        $(router: $srouter:expr,)?
        $(body: $sjson_body:expr,)?
        $(check_json: $scheck_json:expr,)?
        $(extra_assertions: $sextra_fn:expr,)?
    ) => {
        $crate::__with_dollar_sign! {
            ($d:tt) => {
                macro_rules! $sname {
                    ($d (
                        $d name:ident: {
                            $d (uri: $d uri:expr,)?
                            $d (method: $d method:expr,)?
                            $d (status: $d status:expr,)?
                            $d (router: $d router:expr,)?
                            $d (body: $d json_body:expr,)?
                            $d (check_json: $d check_json:expr,)?
                            $d (extra_assertions: $d extra_fn:expr,)?
                        },
                    )*) => {
                        mod $sname {
                            #![ allow( unused_imports ) ]
                            use super::*;
                            $crate::integration_table_tests!{
                                $d(
                                    $d name: {
                                        optional_ident!(
                                            $(uri: $suri,)?
                                            $d(uri: $d uri,)?
                                        );
                                        optional_token!(
                                            $(method: $smethod,)?
                                            $d(check_json: $d method,)?
                                        );
                                        optional_token!(
                                            $(status: $sstatus,)?
                                            $d(check_json: $d status,)?
                                        );
                                        optional_token!(
                                            $(router: $srouter,)?
                                            $d(router: $d router,)?
                                        );
                                        optional_token!(
                                            $(body: $sjson_body,)?
                                            $d(body: $d json_body,)?
                                        );
                                        optional_token!(
                                            $(check_json: $scheck_json,)?
                                            $d(check_json: $d check_json,)?
                                        );
                                        optional_token!(
                                            $(extra_assertions: $sextra_fn,)?
                                            $d(extra_assertions: $d extra_fn,)?
                                        );
                                    },
                                )*
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    crate::table_tests! {
        test_sum,
        (arg1, arg2, expected, msg),
        {
            let sum = arg1 + arg2;
            println!("arg1: {arg1}, arg2: {arg2}, sum: {sum}");
            assert_eq!(arg1 + arg2, expected, "{msg}");
        }
    }

    // use cargo-expand to examine the tests after macro expansion
    // or `cargo rustc --profile=check -- -Zunpretty=expanded` if in hurry ig
    test_sum! {
        works: (
            1, 2, 3, "impossible"
        ), // NOTICE: don't forget the comma at the end
        doesnt_work [should_panic]: (
            1, 2, 4, "expected panic"
        ),
    }

    crate::table_tests! {
        test_sum_async tokio,
        (arg1, arg2, expected, msg),
        {
            // NOTICE: dependencies are searched from super, if tokio was in scope
            // we wouldn't have to go through on deps
            deps::tokio::time::sleep(std::time::Duration::from_nanos(0)).await;
            let sum = arg1 + arg2;
            println!("arg1: {arg1}, arg2: {arg2}, sum: {sum}");
            assert_eq!(arg1 + arg2, expected, "{}", msg);
        },
    }

    test_sum_async! {
        works: (
            1, 2, 3, "doesn't work"
        ),
    }

    crate::table_tests! {
        test_sum_async_multi tokio,
        (arg1, arg2, expected),
        {
            deps::tokio::task::block_in_place(||{
                let sum = arg1 + arg2;
                println!("arg1: {arg1}, arg2: {arg2}, sum: {sum}");
                assert_eq!(arg1 + arg2, expected);
            });
        },
        multi_thread: true,
    }

    test_sum_async_multi! {
        works: (
            1, 2, 3
        ),
    }

    use deps::*;
    fn sum_router() -> axum::Router {
        #[derive(serde::Deserialize)]
        #[serde(crate = "serde")]
        struct Args {
            a: u32,
            b: u32,
        }
        use axum::Json;
        axum::Router::new().route(
            "/sum",
            axum::routing::post(|Json(args): Json<Args>| async move {
                Json(serde_json::json!({ "c": (args.a + args.b) }))
            }),
        )
    }

    macro_rules! integ_table_test_sum {
        ($(
            $name:ident: {
                status: $status:expr,
                body: $json_body:expr,
                $(check_json: $check_json:expr,)?
                $(extra_assertions: $extra_fn:expr,)?
            },
        )*) => {
            mod integ_table_test_sum {
                use super::*;
                crate::integration_table_tests! {
                    $(
                        $name: {
                            uri: "/sum",
                            method: "POST",
                            status: $status,
                            router: sum_router(),
                            body: $json_body,
                            $(check_json: $check_json,)?
                            $(extra_assertions: $extra_fn,)?
                        },
                    )*
                }
            }
        };
    }

    integ_table_test_sum! {
        succeeds: {
            status: axum::http::StatusCode::OK,
            body: serde_json::json!({ "a": 1, "b": 2 }),
            check_json: serde_json::json!({ "c": 3  }),
            extra_assertions: &|crate::utils::testing::EAArgs { .. }| {
                Box::pin(async move {
                    // do stutff
                })
            },
        },
    }

    // crate::integration_table_tests_shorthand!{
    //     integ_table_test_sum_short,
    //     uri: "/sum",
    //     method: "POST",
    //     router: sum_router(),
    // }
    //
    // integ_table_test_sum_short! {
    //     succeeds: {
    //         status: axum::http::StatusCode::OK,
    //         body: serde_json::json!({ "a": 1, "b": 2 }),
    //         check_json: serde_json::json!({ "c": 3  }),
    //         extra_assertions: &|crate::utils::testing::EAArgs { .. }| {
    //             Box::pin(async move {
    //                 assert!(true);
    //             })
    //         },
    //     },
    // }
}
