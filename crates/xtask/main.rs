use clap::Parser;

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
}

fn main() -> Result<std::process::ExitCode, Box<dyn std::error::Error>> {
    // use std::os::unix::process::*;
    dotenvy::dotenv().ok();

    let args = Cli::parse();
    /* let mut cargo_bin = std::process::Command::new(
        std::env::var("XTASK_CARGO_BIN").unwrap_or_else(|_| "cargo".into()),
    ); */
    match args.commands {
        Commands::Test { args } => {
            let mut cmd = if std::process::Command::new("cargo") 
                .arg("nextest --version")
                .output()
                .is_ok()
            {
                let mut cmd = std::process::Command::new("cargo");
                cmd.args(["nextest", "run"]);
                cmd
            } else {
                let mut cmd = std::process::Command::new("cargo");
                cmd.args(["test"]);
                cmd
            };
            if let Some(args) = args {
                cmd.arg(&args);
            }
            let status = cmd.status().unwrap();
            if !status.success() {
                return Ok(std::process::ExitCode::FAILURE);
            }

            // cargo_cmd.args([
            //     "nextest",
            //     args.as_deref().unwrap_or("run"),
            // ]);
            // cargo_cmd.output()?;
        }
    }
    Ok(std::process::ExitCode::SUCCESS)
}
