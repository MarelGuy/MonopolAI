use clap::{
    Parser,
    builder::{
        Styles,
        styling::{AnsiColor, Effects},
    },
};

use crate::llm::LlmApiType;

#[derive(Parser, Debug)]
#[command(
    name = "MonopolAI Server",
    styles = Styles::styled()
        .header(AnsiColor::Green.on_default() | Effects::BOLD)
        .usage(AnsiColor::Green.on_default() | Effects::BOLD)
        .literal(AnsiColor::Blue.on_default() | Effects::BOLD)
        .placeholder(AnsiColor::Cyan.on_default()),
    version,
    about = "MonopolAI is a rust Backend for the MonopolAI Chat App",
    long_about = "MonopolAI is a rust Backend for the MonopolAI Chat App where you can play the Monopoly board  game with AI players.",
    arg_required_else_help = false
)]
pub struct Args {
    /// Host address to bind the server to
    #[arg(long, env, default_value = "127.0.0.1")]
    pub host: String,

    /// Port to bind the server to
    #[arg(short, long, env, default_value_t = 3000)]
    pub port: u16,

    /// Log level filter (e.g. INFO, DEBUG, trace)
    #[arg(
        long,
        env = "RUST_LOG",
        default_value = "INFO",
        help_heading = "Logging"
    )]
    pub log_level: String,

    /// Wire protocol of the remote LLM API.
    ///
    /// `anthropic` (Messages API) is recommended for best performance.
    /// `openai-responses` (Responses API) is supported; `openai-chat`
    /// (classic Chat Completions) is kept for compatibility and logs an
    /// advisory to prefer Anthropic or Responses.
    #[arg(
        long,
        env = "LLM_API_TYPE",
        value_enum,
        default_value = "anthropic",
        help_heading = "LLM Service"
    )]
    pub llm_api_type: LlmApiType,

    /// Base URL of the remote LLM API. Defaults to the provider's public host:
    /// `https://api.anthropic.com` for Anthropic and `https://api.openai.com`
    /// for the `OpenAI` API types.
    #[arg(long, env = "LLM_API_URL", help_heading = "LLM Service")]
    pub llm_api_url: Option<String>,

    /// API key for the remote LLM API.
    #[arg(long, env = "LLM_API_KEY", help_heading = "LLM Service")]
    pub llm_api_key: Option<String>,

    /// Model ID sent to the remote LLM API.
    #[arg(long, env = "LLM_API_MODEL", help_heading = "LLM Service")]
    pub llm_api_model: String,

    /// Edition of the game you want to play, if not supported by default, you can also import a file
    /// Currently available editions: UK edition
    #[arg(
        long,
        env = "GAME_EDITION",
        required = true,
        help_heading = "Edition of the game"
    )]
    pub game_edition: String,

    /// SQLite file to use as database
    #[arg(
        long,
        env = "SQLITE_FILE",
        default_value = "monopolai.db",
        help_heading = "Database"
    )]
    pub sqlite_file: String,

    /// Comma-separated list of allowed CORS origins
    #[arg(long, env, default_value = "*", help_heading = "Security")]
    pub allowed_origins: String,
}
