use clap::Parser;
use std::process::Command;

#[derive(Debug, Clone, clap::Parser)]
#[clap(version, about)]
#[clap(propagate_version = true)]
struct Cli {
    #[clap(subcommand)]
    commands: Commands,
}

#[derive(Debug, Clone, clap::Subcommand)]
enum Commands {
    /// Interact with the nextest runner.
    /// Will run the tests by default.
    #[clap(visible_alias = "t")]
    Test {
        /// The arguments to pass to nextest.
        args: Option<String>,
    },
    PreCommit {},
    ResetDb {
        ///Automatic confirmation. Without this option, you will be prompted before dropping your database.
        #[clap(short)]
        yes: bool,
    },
}

fn main() -> Result<(), AnyErr> {
    // use std::os::unix::process::*;
    dotenvy::dotenv().ok();

    let args = Cli::parse();
    /* let mut cargo_bin = std::process::Command::new(
        std::env::var("XTASK_CARGO_BIN").unwrap_or_else(|_| "cargo".into()),
    ); */
    match args.commands {
        Commands::Test { args } => {
            if is_nextest_avail() {
                assert!(
                    show_cmd(cargo_cmd().args(&[
                        "nextest",
                        "run",
                        if let Some(ref args) = args {
                            &args[..]
                        } else {
                            ""
                        }
                    ]))
                    .status()
                    .unwrap()
                    .success(),
                    "failed to run cargo nextest"
                );
            } else {
                assert!(
                    show_cmd(cargo_cmd().args(&["test"]))
                        .status()
                        .unwrap()
                        .success(),
                    "failed to run cargo test"
                );
            }
        }
        Commands::PreCommit {} => {
            assert!(
                show_cmd(cargo_cmd().args(&["fmt",]))
                    .status()
                    .unwrap()
                    .success(),
                "failed to cargo fmt"
            );
            assert!(
                show_cmd(cargo_cmd().args(&["sqlx", "prepare", "--", "--lib",]))
                    .status()
                    .unwrap()
                    .success(),
                "failed to prepare sqlx-data.json file"
            );
            assert!(
                show_cmd(
                    cargo_cmd()
                        .args(&["run", "--bin", "print_oas",])
                        .stdout(std::fs::File::create("api.oas3.json")?)
                )
                .status()
                .unwrap()
                .success(),
                "failed to create api.oas3.json file"
            );
        }
        Commands::ResetDb { yes: no_confirm } => {
            let mut sqlx_cmd = cargo_cmd();
            sqlx_cmd.args(&["sqlx", "database", "reset"]);
            if no_confirm {
                sqlx_cmd.arg("-y");
            }
            assert!(
                show_cmd(&mut sqlx_cmd).status().unwrap().success(),
                "failed to reset database"
            );
            assert!(
                show_cmd(Command::new("podman").args(&[
                    "exec",
                    "-i",
                    "postgres-server-dev", // FIXME: read this name from the compose file
                    "psql",
                    "-U",
                    "web_api" // FIXME: read this from the environment
                ]))
                // FIXME: pipe in all `sql` files in the `fixtures` dir
                .stdin(std::fs::File::open("fixtures/000_test_data.sql")?)
                .status()
                .unwrap()
                .success(),
                "failed to repopulate database"
            );
        }
    }
    Ok(())
}

type AnyErr = Box<dyn std::error::Error>;

/*
fn build_bin(name: &str, release: bool) {
    let mut build_cmd = cargo_cmd();
    build_cmd.args(&["build", "--bin", name]);
    if release {
        build_cmd.arg("--release");
    }
    assert!(
        show_cmd(&mut build_cmd).status().unwrap().success(),
        "failed to build binary \"{name}\""
    );
}
*/

fn is_nextest_avail() -> bool {
    static NEXTEST_AVAIL: once_cell::sync::Lazy<bool> = once_cell::sync::Lazy::new(|| {
        let val = show_cmd(cargo_cmd().args(["nextest", "--version"]))
            .output()
            .is_ok();
        if val {
            // println!("cargo nextest found in path");
        } else {
            println!("cargo nextest not found in path");
        }
        val
    });
    *NEXTEST_AVAIL
}

fn show_cmd(cmd: &mut Command) -> &mut Command {
    println!(
        "[{:?}, {:?} ]",
        cmd.get_program(),
        cmd.get_args().collect::<Vec<_>>()
    );
    cmd
}

fn cargo_cmd() -> Command {
    /* let mut cargo_bin = Command::new(
        std::env::var("XTASK_CARGO_BIN").unwrap_or_else(|_| "cargo".into()),
    ); */
    Command::new("cargo")
}
