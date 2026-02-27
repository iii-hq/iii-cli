use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "iii-cli",
    about = "Unified CLI dispatcher for iii tools",
    version,
    after_help = "COMMANDS:\n  console    Launch the iii web console\n  create     Create a new iii project from a template\n  motia      Create a new Motia project from a template\n  start      Start the iii process communication engine\n  update     Update iii-cli and managed binaries to their latest versions\n  list       Show installed binaries and their versions\n\nSELF-UPDATE:\n  iii-cli update              Update iii-cli + all managed binaries\n  iii-cli update self         Update only iii-cli\n  iii-cli update iii-cli      Update only iii-cli\n  iii-cli update console      Update only iii-console"
)]
pub struct Cli {
    /// Disable background update and advisory checks
    #[arg(long, global = true)]
    pub no_update_check: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Launch the iii web console
    #[command(
        trailing_var_arg = true,
        allow_hyphen_values = true,
    )]
    Console {
        /// Arguments passed through to the binary
        #[arg(num_args = 0..)]
        args: Vec<String>,
    },

    /// Create a new iii project from a template
    #[command(
        trailing_var_arg = true,
        allow_hyphen_values = true,
    )]
    Create {
        /// Arguments passed through to the binary
        #[arg(num_args = 0..)]
        args: Vec<String>,
    },

    /// Create a new Motia project from a template
    #[command(
        trailing_var_arg = true,
        allow_hyphen_values = true,
    )]
    Motia {
        /// Arguments passed through to the binary
        #[arg(num_args = 0..)]
        args: Vec<String>,
    },

    /// Start the iii process communication engine
    #[command(
        trailing_var_arg = true,
        allow_hyphen_values = true,
    )]
    Start {
        /// Arguments passed through to the binary
        #[arg(num_args = 0..)]
        args: Vec<String>,
    },

    /// Update iii-cli and managed binaries to their latest versions
    Update {
        /// Specific command or binary to update (e.g., "console", "self").
        /// Use "self" or "iii-cli" to update only iii-cli.
        /// If omitted, updates iii-cli and all installed binaries.
        #[arg(name = "command")]
        target: Option<String>,
    },

    /// Show installed binaries and their versions
    List,
}

/// Extract the command name and passthrough args from a parsed Commands value.
pub fn extract_command_info(cmd: &Commands) -> CommandInfo<'_> {
    match cmd {
        Commands::Console { args } => CommandInfo::Dispatch {
            command: "console",
            args,
        },
        Commands::Create { args } => CommandInfo::Dispatch {
            command: "create",
            args,
        },
        Commands::Motia { args } => CommandInfo::Dispatch {
            command: "motia",
            args,
        },
        Commands::Start { args } => CommandInfo::Dispatch {
            command: "start",
            args,
        },
        Commands::Update { target } => CommandInfo::Update {
            target: target.as_deref(),
        },
        Commands::List => CommandInfo::List,
    }
}

/// Parsed command information for the main dispatcher.
pub enum CommandInfo<'a> {
    /// Dispatch to a managed binary with passthrough args
    Dispatch {
        command: &'static str,
        args: &'a [String],
    },
    /// Update command
    Update { target: Option<&'a str> },
    /// List installed binaries
    List,
}
