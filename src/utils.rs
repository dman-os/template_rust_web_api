// use deps::*;

/* #[derive(serde::Serialize, utoipa::ToSchema)]
#[serde(crate = "serde", rename_all = "camelCase")]
pub struct ErrorResponse<T> {
    pub code: u16,
    #[serde(flatten)]
    pub err: T,
}

impl<T> ErrorResponse<T> {
    pub const fn new(code: u16, err: T) -> Self {
        Self { code, err }
    }
} */
pub trait TypeNameRaw {
    fn type_name_raw() -> &'static str {
        let name = std::any::type_name::<Self>();
        match &name.rfind(':') {
            Some(pos) => &name[pos + 1..name.len()],
            None => name,
        }
    }
}
impl<T> TypeNameRaw for T {}

#[test]
fn test_type_name_macro() {
    struct Foo {}
    assert_eq!("Foo", Foo::type_name_raw());
}

#[cfg(test)]
pub mod testing {
    use deps::*;

    use crate::{Context, SharedContext};

    pub fn setup_tracing() -> eyre::Result<()> {
        color_eyre::install()?;
        if std::env::var("RUST_LOG_TEST").is_err() {
            std::env::set_var("RUST_LOG_TEST", "info");
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

    // Ensure that the `tracing` stack is only initialised once using `once_cell`
    // isn't required in cargo-nextest since each test runs in a new process
    pub fn setup_tracing_once() {
        use once_cell::sync::Lazy;
        static TRACING: Lazy<()> = Lazy::new(|| {
            dotenvy::dotenv().ok();
            setup_tracing().unwrap();
        });
        Lazy::force(&TRACING);
    }

    pub struct TestContext {
        pub test_name: &'static str,
        ctx: Option<SharedContext>,
        // clean_up_closure: Option<Box<dyn FnOnce(Context) -> ()>>,
        clean_up_closure:
            Option<Box<dyn FnOnce(Context) -> futures::future::BoxFuture<'static, ()>>>,
    }

    impl TestContext {
        pub async fn new(test_name: &'static str) -> Self {
            setup_tracing_once();

            let config = crate::Config {
                pass_salt_hash: b"sea brine".to_vec(),
                argon2_conf: argon2::Config::default(),
                auth_token_lifetime: time::Duration::seconds_f64(60. * 60. * 24. * 30.),
            };

            use sqlx::prelude::*;
            let opts = sqlx::postgres::PgConnectOptions::default()
                .host(
                    std::env::var("TEST_DB_HOST")
                        .expect("TEST_DB_HOST wasn't found in enviroment")
                        .as_str(),
                )
                .port(
                    std::env::var("TEST_DB_PORT")
                        .expect("TEST_DB_PORT wasn't found in enviroment")
                        .parse()
                        .expect("TEST_DB_PORT is not a valid number"),
                )
                .username(
                    std::env::var("TEST_DB_USER")
                        .expect("TEST_DB_USER wasn't found in enviroment")
                        .as_str(),
                );

            let mut opts = if let Some(pword) = std::env::var("TEST_DB_PASS").ok() {
                opts.password(pword.as_str())
            } else {
                opts
            };
            opts.log_statements("DEBUG".parse().unwrap());

            let mut connection = opts
                .connect()
                .await
                .expect("Failed to connect to Postgres without db");

            connection
                .execute(&format!(r###"DROP DATABASE IF EXISTS {}"###, test_name)[..])
                .await
                .expect("Failed to drop old database.");

            connection
                .execute(&format!(r###"CREATE DATABASE {}"###, test_name)[..])
                .await
                .expect("Failed to create database.");

            let opts = opts.database(test_name);

            // migrate database
            let db_pool = sqlx::PgPool::connect_with(opts)
                .await
                .expect("Failed to connect to Postgres as test db.");

            sqlx::migrate!("./migrations")
                .run(&db_pool)
                .await
                .expect("Failed to migrate the database");

            sqlx::migrate!("./fixtures")
                .set_ignore_missing(true) // don't inspect migrations store
                .run(&db_pool)
                .await
                .expect("Failed to add test data");

            let ctx = Context { db_pool, config };
            Self {
                test_name,
                ctx: Some(std::sync::Arc::new(ctx)),
                /* clean_up_closure: Some(Box::new(move |ctx| {
                    futures::executor::block_on(async move {
                        ctx.db_pool.close().await;

                        tracing::info!("got here: {}", ctx.db_pool.size());
                        tokio::time::timeout(
                            std::time::Duration::from_secs(1),
                            connection
                                .execute(&format!(r###"DROP DATABASE {test_name}"###)[..]),
                        )
                        .await
                        .expect("Timeout when dropping test database.")
                        .expect("Failed to drop test database.");
                        tracing::info!("got here");
                    })
                })), */
                clean_up_closure: Some(Box::new(move |ctx| {
                    Box::pin(async move {
                        ctx.db_pool.close().await;
                        connection
                            .execute(&format!(r###"DROP DATABASE {test_name}"###)[..])
                            .await
                            .expect("Failed to drop test database.");
                    })
                })),
            }
        }

        pub fn ctx(&self) -> SharedContext {
            self.ctx.clone().unwrap_or_log()
        }

        /// Call this after all holders of the [`SharedContext`] have been dropped.
        pub async fn close(mut self) {
            let ctx = self.ctx.take().unwrap_or_log();
            let ctx = std::sync::Arc::<_>::try_unwrap(ctx).unwrap_or_log();
            (self.clean_up_closure.take().unwrap())(ctx);
        }
        /* pub async fn new_with_service<F>(
            test_name: &'static str,
            cfg_callback: F,
        ) -> (
            Self,
            // TestServer,
            impl FnOnce(Self) -> futures::future::BoxFuture<'static, ()>,
        )
        where
            F:Fn(& Context) -> axum::Router + 'static + Send + Sync + Clone + Copy
        {
            let router = cfg_callback
            Self {
                test_name,
            }
        } */
    }

    impl Drop for TestContext {
        fn drop(&mut self) {
            if self.ctx.is_some() {
                tracing::warn!(
                    "test context dropped without cleaning up for: {}",
                    self.test_name
                )
            }
        }
    }

    pub fn check_json(
        (check_name, check): (&str, &serde_json::Value),
        (json_name, json): (&str, &serde_json::Value),
    ) {
        use serde_json::Value::*;
        match (check, json) {
            (Array(r_arr), Array(arr)) => {
                for ii in 0..arr.len() {
                    check_json(
                        (&format!("{check_name}[{ii}]"), &r_arr[ii]),
                        (&format!("{json_name}[{ii}]"), &arr[ii]),
                    );
                }
            }
            (Object(check), Object(response)) => {
                for (key, val) in check {
                    check_json(
                        (&format!("{check_name}.{key}"), val),
                        (&format!("{json_name}.{key}"), &response[key]),
                    );
                }
            }
            (check, json) => assert_eq!(check, json, "{check_name} != {json_name}"),
        }
    }

    pub trait JsonExt {
        fn remove_keys_from_obj(self: Self, keys: &[&str]) -> Self;
        fn destructure_into_self(self: Self, from: Self) -> Self;
    }
    impl JsonExt for serde_json::Value {
        fn remove_keys_from_obj(self: Self, keys: &[&str]) -> Self {
            match self {
                serde_json::Value::Object(mut map) => {
                    for key in keys {
                        map.remove(*key);
                    }
                    serde_json::Value::Object(map)
                }
                json => panic!("provided json was not an object: {:?}", json),
            }
        }
        fn destructure_into_self(self: Self, from: Self) -> Self {
            match (self, from) {
                (serde_json::Value::Object(mut first), serde_json::Value::Object(second)) => {
                    for (key, value) in second.into_iter() {
                        first.insert(key, value);
                    }
                    serde_json::Value::Object(first)
                }
                (first, second) => panic!(
                    "provided jsons weren't objects: first {:?}, second: {:?}",
                    first, second
                ),
            }
        }
    }
}
