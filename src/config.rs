use clap::Parser;
use derive_getters::Getters;
use ethers::signers::WalletError;
use graphcast_sdk::{
    build_wallet,
    callbook::CallBook,
    cf_nameserver,
    graphcast_agent::{
        message_typing::IdentityValidation, GraphcastAgentConfig, GraphcastAgentError,
    },
    graphql::QueryError,
    init_tracing, wallet_address, GraphcastNetworkName, LogFormat,
};
use serde::{Deserialize, Serialize};
use tracing::info;

#[derive(clap::ValueEnum, Clone, Debug, Serialize, Deserialize, Default)]
pub enum CoverageLevel {
    Minimal,
    #[default]
    OnChain,
    Comprehensive,
}

#[derive(Clone, Debug, Parser, Serialize, Deserialize, Getters, Default)]
#[clap(
    name = "listener-radio",
    about = "Listen and store all messages on Graphcast network in real time",
    author = "GraphOps"
)]
pub struct Config {
    #[clap(
        long,
        value_name = "DATABASE_URL",
        env = "DATABASE_URL",
        help = "Postgres database url"
    )]
    pub database_url: String,
    #[clap(
        long,
        value_name = "FILTER_PROTOCOL",
        env = "FILTER_PROTOCOL",
        help = "Enable filter subscriptions based on topic generation"
    )]
    pub filter_protocol: Option<bool>,
    #[clap(
        long,
        value_name = "INDEXER_ADDRESS",
        env = "INDEXER_ADDRESS",
        help = "Graph account corresponding to Graphcast operator"
    )]
    pub indexer_address: Option<String>,
    #[clap(
        long,
        value_name = "KEY",
        value_parser = Config::parse_key,
        env = "PRIVATE_KEY",
        hide_env_values = true,
        help = "Private key to the Graphcast ID wallet (Precendence over mnemonics)",
    )]
    pub private_key: Option<String>,
    #[clap(
        long,
        value_name = "KEY",
        value_parser = Config::parse_key,
        env = "MNEMONIC",
        hide_env_values = true,
        help = "Mnemonic to the Graphcast ID wallet (first address of the wallet is used; Only one of private key or mnemonic is needed)",
    )]
    pub mnemonic: Option<String>,
    #[clap(
        long,
        value_name = "SUBGRAPH",
        env = "REGISTRY_SUBGRAPH",
        help = "Subgraph endpoint to the Graphcast Registry",
        default_value = "https://api.thegraph.com/subgraphs/name/hopeyen/graphcast-registry-goerli"
    )]
    pub registry_subgraph: String,
    #[clap(
        long,
        value_name = "SUBGRAPH",
        env = "NETWORK_SUBGRAPH",
        help = "Subgraph endpoint to The Graph network subgraph",
        default_value = "https://gateway.testnet.thegraph.com/network"
    )]
    pub network_subgraph: String,
    #[clap(
        long,
        value_name = "GRAPHCAST_NETWORK",
        default_value = "testnet",
        env = "GRAPHCAST_NETWORK",
        help = "Supported Graphcast networks: mainnet, testnet"
    )]
    pub graphcast_network: GraphcastNetworkName,
    #[clap(
        long,
        value_name = "[TOPIC]",
        value_delimiter = ',',
        env = "TOPICS",
        help = "Comma separated static list of content topics to subscribe to (Static list to include)"
    )]
    pub topics: Vec<String>,
    #[clap(
        long,
        value_name = "WAKU_HOST",
        help = "Host for the Waku gossip client",
        env = "WAKU_HOST"
    )]
    pub waku_host: Option<String>,
    #[clap(
        long,
        value_name = "WAKU_PORT",
        help = "Port for the Waku gossip client",
        env = "WAKU_PORT"
    )]
    pub waku_port: Option<String>,
    #[clap(
        long,
        value_name = "KEY",
        env = "WAKU_NODE_KEY",
        hide_env_values = true,
        help = "Private key to the Waku node id"
    )]
    pub waku_node_key: Option<String>,
    #[clap(
        long,
        value_name = "KEY",
        env = "WAKU_ADDRESS",
        hide_env_values = true,
        help = "Advertised address to be connected among the Waku peers"
    )]
    pub waku_addr: Option<String>,
    #[clap(
        long,
        value_name = "NODE_ADDRESSES",
        help = "Comma separated static list of waku boot nodes to connect to",
        env = "BOOT_NODE_ADDRESSES"
    )]
    pub boot_node_addresses: Vec<String>,
    #[clap(
        long,
        value_name = "WAKU_LOG_LEVEL",
        help = "Waku node logging configuration",
        env = "WAKU_LOG_LEVEL"
    )]
    pub waku_log_level: Option<String>,
    #[clap(
        long,
        value_name = "DISCV5_ENRS",
        help = "Comma separated ENRs for Waku discv5 bootstrapping",
        env = "DISCV5_ENRS"
    )]
    pub discv5_enrs: Option<Vec<String>>,
    #[clap(
        long,
        value_name = "DISCV5_PORT",
        help = "Waku node to expose discoverable udp port",
        env = "DISCV5_PORT"
    )]
    pub discv5_port: Option<u16>,
    #[clap(
        long,
        value_name = "LOG_LEVEL",
        default_value = "info",
        help = "logging configurationt to set as RUST_LOG",
        env = "RUST_LOG"
    )]
    pub log_level: String,
    #[clap(
        long,
        value_name = "SLACK_WEBHOOK",
        help = "Slack webhook URL to send messages to",
        env = "SLACK_WEBHOOK"
    )]
    pub slack_webhook: Option<String>,
    #[clap(
        long,
        value_name = "DISCORD_WEBHOOK",
        help = "Discord webhook URL to send messages to",
        env = "DISCORD_WEBHOOK"
    )]
    pub discord_webhook: Option<String>,
    #[clap(
        long,
        value_name = "TELEGRAM_TOKEN",
        help = "Telegram Bot API Token",
        env = "TELEGRAM_TOKEN"
    )]
    pub telegram_token: Option<String>,
    #[clap(
        long,
        value_name = "TELEGRAM_CHAT_ID",
        help = "Id of Telegram chat (DM or group) to send messages to",
        env = "TELEGRAM_CHAT_ID"
    )]
    pub telegram_chat_id: Option<i64>,
    #[clap(
        long,
        value_name = "METRICS_HOST",
        default_value = "0.0.0.0",
        help = "If port is set, the Radio will expose Prometheus metrics on the given host. This requires having a local Prometheus server running and scraping metrics on the given port.",
        env = "METRICS_HOST"
    )]
    pub metrics_host: String,
    #[clap(
        long,
        value_name = "METRICS_PORT",
        help = "If set, the Radio will expose Prometheus metrics on the given port (off by default). This requires having a local Prometheus server running and scraping metrics on the given port.",
        env = "METRICS_PORT"
    )]
    pub metrics_port: Option<u16>,
    #[clap(
        long,
        value_name = "SERVER_HOST",
        default_value = "0.0.0.0",
        help = "If port is set, the Radio will expose API service on the given host.",
        env = "SERVER_HOST"
    )]
    pub server_host: String,
    #[clap(
        long,
        value_name = "SERVER_PORT",
        help = "If set, the Radio will expose API service on the given port (off by default).",
        env = "SERVER_PORT"
    )]
    pub server_port: Option<u16>,
    #[clap(
        long,
        value_name = "LOG_FORMAT",
        env = "LOG_FORMAT",
        help = "Support logging formats: pretty, json, full, compact",
        long_help = "pretty: verbose and human readable; json: not verbose and parsable; compact:  not verbose and not parsable; full: verbose and not parsible",
        default_value = "pretty"
    )]
    pub log_format: LogFormat,
    // This is only used when filtering for specific radios
    #[clap(
        long,
        value_name = "RADIO_NAME",
        env = "RADIO_NAME",
        default_value = "subgraph-radio"
    )]
    pub radio_name: String,
    #[clap(long, value_name = "MAX_STORAGE", env = "MAX_STORAGE")]
    pub max_storage: Option<i32>,
    #[clap(
        long,
        value_name = "ID_VALIDATION",
        value_enum,
        env = "ID_VALIDATION",
        default_value = "valid-address",
        help = "Identity validaiton mechanism for message signers",
        long_help = "Identity validaiton mechanism for message signers\n
        no-check: all messages signer is valid, \n
        valid-address: signer needs to be an valid Eth address, \n
        graphcast-registered: must be registered at Graphcast Registry, \n
        graph-network-account: must be a Graph account, \n
        registered-indexer: must be registered at Graphcast Registry, correspond to and Indexer statisfying indexer minimum stake requirement, \n
        indexer: must be registered at Graphcast Registry or is a Graph Account, correspond to and Indexer statisfying indexer minimum stake requirement"
    )]
    pub id_validation: IdentityValidation,
    #[clap(
        long,
        value_name = "RETENTION",
        env = "RETENTION",
        default_value_t = 1440
    )]
    pub retention: i32,
}

impl Config {
    /// Parse config arguments
    pub fn args() -> Self {
        // TODO: load config file before parse (maybe add new level of subcommands)
        let config = Config::parse();
        std::env::set_var("RUST_LOG", config.log_level.clone());
        // Enables tracing under RUST_LOG variable
        init_tracing(config.log_format.to_string()).expect("Could not set up global default subscriber for logger, check environmental variable `RUST_LOG` or the CLI input `log-level`");
        config
    }

    /// Validate that private key as an Eth wallet
    fn parse_key(value: &str) -> Result<String, WalletError> {
        // The wallet can be stored instead of the original private key
        let wallet = build_wallet(value)?;
        let address = wallet_address(&wallet);
        info!(address, "Resolved Graphcast id");
        Ok(String::from(value))
    }

    /// Private key takes precedence over mnemonic
    pub fn wallet_input(&self) -> Result<&String, ConfigError> {
        match (&self.private_key, &self.mnemonic) {
            (Some(p), _) => Ok(p),
            (_, Some(m)) => Ok(m),
            _ => Err(ConfigError::ValidateInput(
                "Must provide either private key or mnemonic".to_string(),
            )),
        }
    }

    pub async fn to_graphcast_agent_config(
        &self,
    ) -> Result<GraphcastAgentConfig, GraphcastAgentError> {
        let wallet_key = self.wallet_input().unwrap().to_string();
        let topics = self.topics.clone();

        GraphcastAgentConfig::new(
            wallet_key,
            self.indexer_address.clone().unwrap_or("none".to_string()),
            self.radio_name.clone(),
            self.registry_subgraph.clone(),
            self.network_subgraph.clone(),
            self.id_validation.clone(),
            None,
            Some(self.boot_node_addresses.clone()),
            Some(self.graphcast_network.to_string()),
            Some(topics),
            self.waku_node_key.clone(),
            self.waku_host.clone(),
            self.waku_port.clone(),
            self.waku_addr.clone(),
            self.filter_protocol,
            self.discv5_enrs.clone(),
            self.discv5_port,
            self.discv5_enrs().clone().unwrap_or_default(),
            Some(cf_nameserver().to_string()),
        )
        .await
    }

    pub fn callbook(&self) -> CallBook {
        CallBook::new(
            self.registry_subgraph.clone(),
            self.network_subgraph.clone(),
            None,
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("Validate the input: {0}")]
    ValidateInput(String),
    #[error("Generate JSON representation of the config file: {0}")]
    GenerateJson(serde_json::Error),
    #[error("QueryError: {0}")]
    QueryError(QueryError),
    #[error("Toml file error: {0}")]
    ReadStr(std::io::Error),
    #[error("Unknown error: {0}")]
    Other(anyhow::Error),
}
