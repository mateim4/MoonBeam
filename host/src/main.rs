mod config;
mod error;
mod display;
mod capture;
mod encode;
mod transport;
mod input;
mod proto;

use clap::Parser;

#[derive(Parser)]
#[command(version, about = "MoonBeam host daemon — Linux-native virtual display + streaming + input passthrough")]
struct Cli {
    #[arg(short, long, default_value = "moonbeamd.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    tracing::info!(config = %cli.config, "moonbeamd scaffold — no runtime behavior yet (M0)");
    Ok(())
}
