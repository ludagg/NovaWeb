mod ast;
mod environment;
mod interpreter;
mod parser;
mod server;
mod template;
mod value;

use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "novaw")]
#[command(about = "NovaWeb Programming Language CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Run { path: String },
    Serve {
        #[arg(short, long, default_value = "127.0.0.1")]
        host: String,
        #[arg(short, long, default_value = "3000")]
        port: u16,
        #[arg(short, long, default_value = "pages")]
        pages: String,
        #[arg(short, long, default_value = "static")]
        static_dir: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Run { path } => {
            let content = fs::read_to_string(path)?;
            let statements = parser::parse(&content)?;

            let mut interp = interpreter::Interpreter::new();
            interp.interpret(&statements)?;
        }
        Commands::Serve {
            host,
            port,
            pages,
            static_dir,
        } => {
            let pages_dir = PathBuf::from(pages);
            let static_dir_path = PathBuf::from(static_dir);

            // Ensure pages directory exists
            if !pages_dir.exists() {
                fs::create_dir_all(&pages_dir)?;
            }

            // Ensure static directory exists
            if !static_dir_path.exists() {
                fs::create_dir_all(&static_dir_path)?;
            }

            server::serve(host.clone(), *port, pages_dir, static_dir_path).await;
        }
    }

    Ok(())
}
