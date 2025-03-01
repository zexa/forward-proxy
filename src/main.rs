use std::env;
use anyhow::Result;
use clap::Parser;
use forward_proxy::{ProxyConfig, start_proxy};
use tracing::info;
use tracing_subscriber::{fmt, EnvFilter};
use tracing_log::LogTracer;

/**
 * Forward Proxy that automatically handles authentication
 *
 * This application creates a local proxy server that doesn't require authentication
 * but forwards requests to an upstream proxy that does require authentication.
 * This helps Selenium tests work without needing to handle authentication dialogs.
 */

// CLI arguments
#[derive(Parser, Debug, Clone)]
#[clap(author, version, about)]
struct Args {
    /// Local proxy host to bind to
    #[clap(long, env = "LOCAL_HOST", default_value = "0.0.0.0")]
    local_host: String,
    
    /// Local proxy port to bind to
    #[clap(long, env = "LOCAL_PORT", default_value_t = 8118)]
    local_port: u16,
    
    /// Upstream proxy host
    #[clap(long, env = "PROXY_HOST", default_value = "squid")]
    proxy_host: String,
    
    /// Upstream proxy port
    #[clap(long, env = "PROXY_PORT", default_value_t = 3128)]
    proxy_port: u16,
    
    /// Upstream proxy username
    #[clap(long, env = "PROXY_USER", default_value = "")]
    proxy_user: String,
    
    /// Upstream proxy password
    #[clap(long, env = "PROXY_PASSWORD", default_value = "")]
    proxy_password: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set up tracing/logging
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    
    // Configure the subscriber with env filter
    let filter = EnvFilter::from_default_env();
    
    // Initialize the subscriber as the global default
    fmt()
        .with_env_filter(filter)
        .with_thread_ids(true)
        .with_target(true)
        .init();
    
    // Initialize LogTracer to convert standard log crate records to tracing events
    LogTracer::init()
        .expect("Failed to initialize LogTracer");
    
    // Parse command line arguments
    let args = Args::parse();
    
    info!(
        proxy_host = %args.proxy_host, 
        proxy_port = %args.proxy_port,
        "Args from CLI/ENV"
    );
    
    // Convert CLI args to ProxyConfig
    let config = ProxyConfig::new(
        args.local_host,
        args.local_port,
        args.proxy_host,
        args.proxy_port,
        args.proxy_user,
        args.proxy_password,
    );
    
    info!("Starting proxy server using library implementation");
    
    // Start the proxy server
    start_proxy(config).await
}
