use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "cryptomator_rs")]
#[command(about = "Cryptomator client-side encryption tool in Rust", long_about = None)]
pub struct CliArgs {
    /// Start the Terminal User Interface (TUI)
    #[arg(long)]
    pub tui: bool,

    /// Start the background daemon with embedded Web UI (Syncthing-style)
    #[arg(long)]
    pub serve: bool,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Start the Terminal User Interface (TUI)
    Tui,
    /// Start the background daemon with embedded Web UI (Syncthing-style)
    Serve {
        /// Host address to bind to
        #[arg(short = 'H', long, default_value = "127.0.0.1")]
        host: String,
        /// Port to bind to
        #[arg(short, long, default_value_t = 8080)]
        port: u16,
    },
    /// List all registered vaults
    List,
    /// Register an existing vault folder in the configuration
    Add {
        /// Optional display name (defaults to folder name)
        #[arg(short, long)]
        name: Option<String>,
        /// Path to the existing vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
    },
    /// Remove a registered vault from the configuration
    Remove {
        /// Path of the vault to unregister
        #[arg(short, long)]
        path: String,
    },
    /// Rename a registered vault's display name
    RenameVault {
        /// Path of the vault to rename
        #[arg(short, long)]
        path: String,
        /// New display name
        #[arg(short, long)]
        new_name: String,
    },
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
        /// Optional path to a keyfile for two-factor protection
        #[arg(short, long)]
        keyfile: Option<String>,
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
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Change the password of an existing vault
    ChangePassword {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Current password (if not provided, you will be prompted)
        #[arg(long)]
        old_password: Option<String>,
        /// New password (if not provided, you will be prompted)
        #[arg(long)]
        new_password: Option<String>,
        /// Optional path to current keyfile
        #[arg(long)]
        old_keyfile: Option<String>,
        /// Optional path to new keyfile
        #[arg(long)]
        new_keyfile: Option<String>,
    },
    /// Export the recovery key of a vault
    RecoveryKey {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Verify or recover a vault using its recovery key
    Recover {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// 64-character recovery key (if not provided, you will be prompted)
        #[arg(short, long)]
        key: Option<String>,
    },
    /// List all files stored inside a vault
    Ls {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
        /// Display detailed long listing with size and modification timestamp
        #[arg(short = 'l', long)]
        long: bool,
        /// Optional substring filter pattern
        #[arg(long)]
        pattern: Option<String>,
    },
    /// Search for files inside the vault matching a query or pattern
    Find {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Search pattern or substring
        #[arg(short = 'q', long)]
        query: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
        /// Display detailed long listing with size and modification timestamp
        #[arg(short = 'l', long)]
        long: bool,
    },
    /// Encrypt and store a file into the vault
    Put {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Local file to encrypt and store
        #[arg(short, long)]
        source: String,
        /// Relative path inside the vault (defaults to source filename)
        #[arg(short, long)]
        target: Option<String>,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Decrypt and print the contents of a file in the vault to stdout
    Cat {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Relative path of the file inside the vault
        #[arg(short, long)]
        file: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Delete a file inside the vault
    Rm {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Relative path of the file inside the vault
        #[arg(short, long)]
        file: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Rename or move a file or directory inside the vault
    Mv {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Current relative path inside the vault
        #[arg(long)]
        old_path: String,
        /// New relative path inside the vault
        #[arg(long)]
        new_path: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Show vault statistics (number of decrypted files and total payload size)
    Stats {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Create a directory inside the vault
    Mkdir {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Relative path of the folder to create inside the vault
        #[arg(short, long)]
        folder: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Export and decrypt the entire vault into a local directory
    Export {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Target destination directory to write decrypted files
        #[arg(short, long)]
        dest: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Recursively encrypt and import a local directory into the vault
    Import {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Local directory to import
        #[arg(short, long)]
        source: String,
        /// Optional target directory prefix inside the vault
        #[arg(short, long)]
        dest: Option<String>,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Verify vault integrity and authenticate all encrypted files
    Verify {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Password (if not provided, you will be prompted)
        #[arg(long)]
        password: Option<String>,
        /// Optional path to a keyfile
        #[arg(short, long)]
        keyfile: Option<String>,
    },
    /// Create an explicit timestamped or custom backup copy of masterkey.json
    BackupKey {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Optional custom destination path for the backup file
        #[arg(short, long)]
        out: Option<String>,
    },
    /// Restore masterkey.json from masterkey.json.bkup or a custom backup file
    RestoreKey {
        /// Path to the vault folder containing masterkey.json
        #[arg(short, long)]
        path: String,
        /// Optional custom path of the backup file to restore from
        #[arg(short, long)]
        from: Option<String>,
    },
}
