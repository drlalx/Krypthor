use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "cryptomator_rs")]
#[command(about = "Cryptomator client-side encryption tool in Rust", long_about = None)]
pub struct CliArgs {
    /// Start the Terminal User Interface (TUI)
    #[arg(long)]
    pub tui: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start the Terminal User Interface (TUI)
    Tui,
    /// List all registered vaults
    List,
    /// Create a new vault
    Create {
        /// Name of the vault
        #[arg(short, long)]
        name: String,
        /// Path where the vault folder should be created
        #[arg(short, long)]
        path: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
    },
    /// Unlock and mount a vault using FUSE
    Mount {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Directory where the vault should be mounted (e.g. ~/my-mount)
        #[arg(short, long)]
        mountpoint: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
    },
}
