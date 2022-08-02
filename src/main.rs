#![allow(clippy::single_component_path_imports)]

#[cfg(feature = "dylink")]
#[allow(unused_imports)]
use dylink;

use deps::*;

fn main() {
    setup_tracing().unwrap();
    #[cfg(feature = "dylink")]
    tracing::warn!("dylink enabled");
}

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
        .map_err(|err| eyre::anyhow!(err))?;

    Ok(())
}
