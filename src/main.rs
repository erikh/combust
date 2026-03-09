use clap::Parser;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = combust::cli::Cli::parse();
    cli.run().await
}
