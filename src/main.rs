use clap::Parser;
use mangowm_mod::cli::{run_cli, Cli};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    run_cli(cli)
}
