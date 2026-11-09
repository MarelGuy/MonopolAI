use clap::{
    Parser,
    builder::{
        Styles,
        styling::{AnsiColor, Effects},
    },
};

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

    /// Base URL of the remote OpenAI-compatible LLM API (e.g. Alibaba Cloud
    /// Model Studio). Defaults to `OpenCode` Go when only the key is set.
    #[arg(
        long,
        env = "LLM_API_URL",
        required = true,
        help_heading = "LLM Service"
    )]
    pub llm_api_url: Option<String>,

    /// API key for the remote LLM API: when set, chat generation uses the
    /// remote endpoint instead of the local llama-server
    #[arg(
        long,
        env = "LLM_API_KEY",
        required = true,
        help_heading = "LLM Service"
    )]
    pub llm_api_key: Option<String>,

    /// Model ID sent to the remote LLM API (e.g. deepseek-v4.1-flash).
    /// Used as the default for all tiers when a tier-specific model is unset.
    #[arg(
        long,
        env = "LLM_API_MODEL",
        required = true,
        help_heading = "LLM Service"
    )]
    pub llm_api_model: String,

    /// Comma-separated list of allowed CORS origins
    #[arg(long, env, default_value = "*", help_heading = "Security")]
    pub allowed_origins: String,
}
