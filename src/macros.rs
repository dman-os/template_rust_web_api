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
    ($token1:expr) => {
        $token1
    };
    ($token1:expr, $($token2:expr,)+) => {
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
}

// #[macro_export]
// macro_rules! integration_table_tests {
//     ($(
//         $name:ident: {
//             uri: $uri:expr,
//             method: $method:ident,
//             status: $status:expr,
//             router: $router:expr,
//             $(body: $json_body:expr,)?
//             $(check_json: $check_json:expr,)?
//             $(auth_user: $auth_user:expr,)?
//             $(extra_assertions: $extra_fn:expr,)?
//             $(print_response: $print_res:expr,)?
//         },
//     )*) => {
//         $(
//             #[allow(unused_variables)]
//             #[tokio::test]
//             async fn $name() {
//                 let ctx = TestContext::new(crate::function!()).await;
//                 {
//                     let app = $router.layer(axum::Extension(ctx.ctx()));
//
//                     let uri = $uri;
//                     let json: Option<serde_json::Value> = crate::optional_expr!($($json_body)?);
//
//                     let resp = app
//                         .oneshot(
//                             Request::builder()
//                                 .method($method)
//                                 .uri($uri)
//                                 .body(Default::default())
//                                 .unwrap_or_log(),
//                         )
//                         .await
//                         .unwrap_or_log();
//                     assert_eq!(resp.status(), StatusCode::OK);
//                     let body = resp.into_body();
//                     let body = hyper::body::to_bytes(body).await.unwrap_or_log();
//                     let body = serde_json::from_slice(&body).unwrap_or_log();
//                     check_json(
//                         (
//                             "expected",
//                             &serde_json::json!({
//                                 "id": USER_01_ID,
//                                 "username": USER_01_USERNAME,
//                                 "email": USER_01_EMAIL,
//                             }),
//                         ),
//                         ("response", &body),
//                     );
//                 }
//                 ctx.close().await;
//
//                 let (mut ctx, srv, clean_up) =
//                     crate::utils::testing::TestContext::new_with_service(crate::function!(), $configure_fn).await;
//
//                 let path = $path;
//
//                 let json: Option<serde_json::Value> = crate::optional_expr!($($json_body)?);
//
//
//                 let token: Option<String> = crate::optional_expr!($($auth_user)?)
//                     .map(|user_id: &'static str| generate_valid_token_default(
//                             user_id.parse().unwrap(),
//                             ctx.ctx.as_ref().unwrap(),
//                         )
//                         .unwrap()
//                     );
//
//                 let mut res = crate::__send_req!(srv, $method, path, json, token, $($json_body)?);
//
//                 let status_code = $status;
//                 assert_eq!(res.status(), status_code, "response: {:?}", res);
//
//                 let check_json: Option<serde_json::Value> = crate::optional_expr!($($check_json)?);
//
//                 let response_json: Option<serde_json::Value> = res.json().await.ok();
//                 if let Some(check_json) = check_json {
//                     let response_json = response_json.as_ref().unwrap();
//                     crate::utils::testing::check_json(("check", &check_json), ("response", &response_json));
//                 }
//                 let print_response: Option<bool> = crate::optional_expr!($($print_res)?);
//                 if let Some(true) = print_response{
//                     tracing::info!(response = ?res, "reponse_json: {:#?}",response_json);
//                 }
//
//                 use crate::utils::testing::{ExtraAssertions, EAArgs};
//                 let extra_assertions: Option<&ExtraAssertions> = crate::optional_expr!($($extra_fn)?);
//                 if let Some(extra_assertions) = extra_assertions {
//                     extra_assertions(EAArgs{
//                         ctx: &mut ctx,
//                         srv: &srv,
//                         auth_token: token,
//                         response_json
//                     }).await;
//                 }
//
//                 clean_up(ctx).await;
//             }
//         )*
//     }
// }
//
// #[macro_export]
// macro_rules! integration_table_tests_shorthand{
//     (
//         $sname:ident,
//         $(path: $spath:expr,)?
//         $(method: $smethod:ident,)?
//         $(status: $sstatus:expr,)?
//         $(configure_fn: $sconfigure_fn:expr,)?
//         $(body: $sjson_body:expr,)?
//         $(check_json: $scheck_json:expr,)?
//         $(extra_assertions: $sextra_fn:expr,)?
//     ) => {
//         $crate::__with_dollar_sign! {
//             ($d:tt) => {
//                 macro_rules! $sname{
//                     ($d (
//                         $d name:ident: {
//                             $d (path: $d path:expr,)?
//                             $d (method: $d method:ident,)?
//                             $d (status: $d status:expr,)?
//                             $d (configure_fn: $d configure_fn:expr,)?
//                             $d (body: $d json_body:expr,)?
//                             $d (check_json: $d check_json:expr,)?
//                             $d (extra_assertions: $d extra_fn:expr,)?
//                         },
//                     )*) =>{
//                         crate::integration_table_tests!{
//                             $d(
//                                 $d name: {
//                                     optional_ident!(
//                                         $(path: $spath,)?
//                                         $d(path: $d path,)?
//                                     );
//                                     optional_token!(
//                                         $(method: $smethod,)?
//                                         $d(check_json: $d method,)?
//                                     );
//                                     optional_token!(
//                                         $(status: $sstatus,)?
//                                         $d(check_json: $d status,)?
//                                     );
//                                     optional_token!(
//                                         $(configure_fn: $sconfigure_fn,)?
//                                         $d(configure_fn: $d configure_fn,)?
//                                     );
//                                     optional_token!(
//                                         $(body: $sjson_body,)?
//                                         $d(body: $d json_body,)?
//                                     );
//                                     optional_token!(
//                                         $(check_json: $scheck_json,)?
//                                         $d(check_json: $d check_json,)?
//                                     );
//                                     optional_token!(
//                                         $(extra_assertions: $sextra_fn,)?
//                                         $d(extra_assertions: $d extra_fn,)?
//                                     );
//                                 },
//                             )*
//                         }
//                     }
//                 }
//             }
//         }
//     }
// }
//
// #[macro_export]
// macro_rules! __send_req {
//     // without payload
//     ($srv:ident, $method:ident, $path:expr, $json:ident, $token:ident,) => {{
//         let mut req = $srv.$method($path);
//         if let Some(token) = $token.as_ref() {
//             req = req.append_header((
//                 actix_web::http::header::AUTHORIZATION,
//                 format!("Bearer {}", token),
//             ));
//         }
//         req.send().await.unwrap()
//     }};
//     // with payload
//     ($srv:ident, $method:ident, $path:expr, $json:ident, $token:ident, $_json_expr:expr) => {{
//         let mut req = $srv.$method($path);
//         if let Some(token) = $token.as_ref() {
//             req = req.append_header((
//                 actix_web::http::header::AUTHORIZATION,
//                 format!("Bearer {}", token),
//             ));
//         }
//         req.send_json(&$json.unwrap()).await.unwrap()
//     }};
// }
