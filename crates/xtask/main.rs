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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let mut cargo_cmd = std::env::var("XTASK_CARGO_CMD")
        .map(|cmd| std::process::Command::new(cmd))
        .unwrap_or_else(|_| {
            if let Ok(_) = std::process::Command::new("mold").output() {
                let mut cmd = std::process::Command::new("mold");
                cmd.args(["-run cargo"]);
                cmd
            } else {
                std::process::Command::new("cargo")
            }
        });
    match args.commands {
        Commands::Test { args } => {
            println!("TODO: nextest support");
            // cargo_cmd.args([
            //     "nextest",
            //     args.as_deref().unwrap_or("run"),
            // ]);
            // cargo_cmd.output()?;
        }
    }
    Ok(())
}
