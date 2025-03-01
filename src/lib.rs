use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use std::net::SocketAddr;
use anyhow::{Result, anyhow};
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use tokio::signal::unix::{signal, SignalKind};
use std::sync::atomic::{AtomicBool, Ordering};
use tracing::{info, debug, error, instrument};

/// Configuration for the forward proxy
#[derive(Debug, Clone)]
pub struct ProxyConfig {
    /// Local host to bind to
    pub local_host: String,
    /// Local port to bind to
    pub local_port: u16,
    /// Upstream proxy host
    pub proxy_host: String,
    /// Upstream proxy port
    pub proxy_port: u16,
    /// Upstream proxy username
    pub proxy_user: String,
    /// Upstream proxy password
    pub proxy_password: String,
}

impl ProxyConfig {
    /// Create a new proxy configuration
    pub fn new(
        local_host: String,
        local_port: u16,
        proxy_host: String,
        proxy_port: u16,
        proxy_user: String,
        proxy_password: String,
    ) -> Self {
        ProxyConfig {
            local_host,
            local_port,
            proxy_host,
            proxy_port,
            proxy_user,
            proxy_password,
        }
    }
}

static RUNNING: AtomicBool = AtomicBool::new(true);

/// Start the forward proxy server with the provided configuration
#[instrument(skip(config), fields(local_host = %config.local_host, local_port = %config.local_port))]
pub async fn start_proxy(config: ProxyConfig) -> Result<()> {
    // Initialize the proxy configuration
    let config = Arc::new(config);
    
    // Create Basic auth header
    let auth = format!("{}:{}", config.proxy_user, config.proxy_password);
    let encoded_auth = Arc::new(BASE64.encode(auth));
    
    // Output configuration information
    info!("Starting proxy server on {}:{}", config.local_host, config.local_port);
    if !config.proxy_user.is_empty() {
        info!("Forwarding to {}:{} with auth", config.proxy_host, config.proxy_port);
    } else {
        info!("Forwarding to {}:{} without auth", config.proxy_host, config.proxy_port);
    }
    
    // Set up signal handling for graceful shutdown
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown_clone = shutdown.clone();
    
    tokio::spawn(async move {
        // Set up signal handlers
        let mut sigterm = signal(SignalKind::terminate()).unwrap();
        let mut sigint = signal(SignalKind::interrupt()).unwrap();
        
        tokio::select! {
            _ = sigterm.recv() => {
                info!("Received SIGTERM, initiating graceful shutdown");
            }
            _ = sigint.recv() => {
                info!("Received SIGINT, initiating graceful shutdown");
            }
        }
        
        shutdown_clone.store(true, Ordering::SeqCst);
        RUNNING.store(false, Ordering::SeqCst);
    });
    
    // Bind to the server address
    let addr = format!("{}:{}", config.local_host, config.local_port);
    let listener = match TcpListener::bind(&addr).await {
        Ok(listener) => listener,
        Err(e) => {
            error!("Failed to bind to {}: {}", addr, e);
            return Err(anyhow::anyhow!("Failed to bind to {}: {}", addr, e));
        }
    };
    
    info!("Proxy server listening on {}", addr);
    
    // Accept connections
    let mut connection_count = 0;
    
    while RUNNING.load(Ordering::SeqCst) {
        // Use timeout to check shutdown flag periodically
        let accept_result = tokio::time::timeout(
            std::time::Duration::from_secs(1),
            listener.accept()
        ).await;
        
        match accept_result {
            Ok(Ok((stream, addr))) => {
                connection_count += 1;
                debug!("Accepted connection #{} from {}", connection_count, addr);
                
                // Clone the config for this connection
                let config_clone = config.clone();
                let encoded_auth_clone = encoded_auth.clone();
                let client_addr = addr;
                let conn_id = connection_count;
                
                // Handle each client in a separate task
                tokio::spawn(async move {
                    // Create a new span inside the spawned task
                    let span = tracing::info_span!("connection", addr = %client_addr, id = conn_id);
                    let _enter = span.enter();
                    
                    if let Err(e) = handle_tcp_stream(stream, client_addr, config_clone, encoded_auth_clone).await {
                        error!("Error handling connection from {}: {}", client_addr, e);
                    }
                });
            }
            Ok(Err(e)) => {
                error!("Failed to accept connection: {}", e);
                // Brief pause before retrying to avoid CPU spinning on persistent errors
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            Err(_) => {
                // Timeout occurred, just loop to check the shutdown flag
                continue;
            }
        }
    }
    
    info!("Proxy server shutting down. Waiting for existing connections to complete...");
    // Wait for a short period to allow in-flight connections to complete
    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    info!("Proxy server shutdown complete");
    
    Ok(())
}

/// Handle incoming TCP connections
#[instrument(skip(stream, config, _encoded_auth), fields(remote=%addr))]
async fn handle_tcp_stream(
    mut stream: TcpStream, 
    addr: SocketAddr, 
    config: Arc<ProxyConfig>, 
    _encoded_auth: Arc<String>
) -> Result<()> {
    // Set read timeout to avoid hanging connections
    stream.set_nodelay(true)?;
    
    info!("New connection from {}", addr);
    let mut buf = [0; 1024];
    
    // Read with timeout to avoid hanging
    let n = match tokio::time::timeout(
        std::time::Duration::from_secs(10), // 10 second timeout
        stream.read(&mut buf)
    ).await {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => {
            return Err(anyhow!("Error reading from client: {}", e));
        },
        Err(_) => {
            return Err(anyhow!("Timeout reading from client"));
        }
    };
    
    if n == 0 {
        error!("Client disconnected immediately");
        return Ok(());
    }
    
    let data_str = String::from_utf8_lossy(&buf[..n]);
    debug!("Received request: {}", data_str);
    
    if data_str.starts_with("CONNECT") {
        info!("Handling HTTPS CONNECT request from {}", addr);
        handle_connect_direct(&mut stream, &data_str, config.as_ref()).await?;
    } else {
        info!("Handling HTTP request from {}", addr);
        handle_request_internal(&mut stream, &buf[..n], config.as_ref()).await?;
    }
    
    info!("Connection from {} completed", addr);
    Ok(())
}

/// Handle CONNECT requests at the socket level
#[instrument(skip(stream, config))]
async fn handle_connect_direct(
    stream: &mut TcpStream,
    req: &str,
    config: &ProxyConfig,
) -> Result<()> {
    let req_line = req.lines().next().ok_or_else(|| anyhow!("Invalid request"))?;
    let parts: Vec<&str> = req_line.split_whitespace().collect();
    if parts.len() < 2 {
        return Err(anyhow!("Invalid CONNECT request"));
    }
    
    let addr = parts[1];
    info!(target_addr = %addr, "CONNECT request");
    
    // Send the CONNECT request to the upstream proxy with authentication
    let upstream_addr = format!("{}:{}", config.proxy_host, config.proxy_port);
    let mut upstream = TcpStream::connect(&upstream_addr).await?;
    info!("Connected to upstream proxy at {}", upstream_addr);
    
    // Format the Basic auth header
    let auth = format!("{}:{}", config.proxy_user, config.proxy_password);
    let base64_auth = BASE64.encode(auth);
    
    // Send the CONNECT request to the upstream proxy
    let connect_req = format!(
        "CONNECT {} HTTP/1.1\r\nHost: {}\r\nProxy-Authorization: Basic {}\r\nProxy-Connection: Keep-Alive\r\n\r\n",
        addr, addr, base64_auth
    );
    
    upstream.write_all(connect_req.as_bytes()).await?;
    info!("Sent CONNECT request to upstream proxy");
    
    // Read the response from the upstream proxy
    let mut buf = [0; 1024];
    let n = upstream.read(&mut buf).await?;
    
    if n == 0 {
        return Err(anyhow!("Upstream proxy closed connection"));
    }
    
    // Check if the response is successful (HTTP/1.x 200)
    let response = String::from_utf8_lossy(&buf[..n]);
    debug!("Upstream proxy response: {}", response);
    
    if !response.contains("200") {
        error!("Upstream proxy returned error: {}", response);
        stream.write_all(&buf[..n]).await?;
        return Err(anyhow!("Upstream proxy returned error: {}", response));
    }
    
    // Send success to the client
    stream.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n").await?;
    info!("CONNECT tunnel established for {}", addr);
    
    // Start bidirectional tunneling
    let (mut ri, mut wi) = stream.split();
    let (mut ro, mut wo) = upstream.split();
    
    let client_to_upstream = tokio::io::copy(&mut ri, &mut wo);
    let upstream_to_client = tokio::io::copy(&mut ro, &mut wi);
    
    info!("Starting bidirectional tunnel for {}", addr);
    let (client_bytes, upstream_bytes) = tokio::try_join!(client_to_upstream, upstream_to_client)?;
    info!("Tunnel closed. Client sent {} bytes, upstream sent {} bytes", client_bytes, upstream_bytes);
    
    Ok(())
}

/// Handle HTTP requests at the socket level
#[instrument(skip(stream, buf, config))]
async fn handle_request_internal(
    stream: &mut TcpStream,
    buf: &[u8],
    config: &ProxyConfig,
) -> Result<()> {
    // Parse the request to extract the target URL
    let req_str = String::from_utf8_lossy(buf);
    let lines: Vec<&str> = req_str.lines().collect();
    if lines.is_empty() {
        return Err(anyhow!("Empty request"));
    }
    
    let request_line = lines[0];
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(anyhow!("Invalid request line"));
    }
    
    let method = parts[0];
    let uri = parts[1];
    info!(method = %method, uri = %uri, "HTTP request");
    
    // Connect to the upstream proxy
    let upstream_addr = format!("{}:{}", config.proxy_host, config.proxy_port);
    let mut upstream = TcpStream::connect(&upstream_addr).await?;
    info!("Connected to upstream HTTP proxy at {}", upstream_addr);
    
    // Format the Basic auth header
    let auth = format!("{}:{}", config.proxy_user, config.proxy_password);
    let base64_auth = BASE64.encode(auth);
    
    // Modify the request to include proxy authentication
    let mut modified_request = Vec::new();
    let mut has_proxy_auth = false;
    
    for line in lines {
        if line.starts_with("Proxy-Authorization:") {
            has_proxy_auth = true;
            modified_request.push(format!("Proxy-Authorization: Basic {}", base64_auth));
        } else if !line.is_empty() {
            modified_request.push(line.to_string());
        } else {
            // Empty line indicates end of headers
            modified_request.push(line.to_string());
            if !has_proxy_auth {
                // Insert auth header before empty line
                modified_request.insert(
                    modified_request.len() - 1,
                    format!("Proxy-Authorization: Basic {}", base64_auth),
                );
            }
        }
    }
    
    // Send the modified request to upstream
    let modified_req_str = modified_request.join("\r\n") + "\r\n";
    debug!("Sending modified request to upstream");
    upstream.write_all(modified_req_str.as_bytes()).await?;
    
    // Read the response and send it back to the client
    let mut response_buf = [0; 8192];
    info!("Waiting for upstream response");
    
    let mut total_bytes = 0;
    loop {
        let n = match upstream.read(&mut response_buf).await {
            Ok(0) => break, // Connection closed
            Ok(n) => n,
            Err(e) => return Err(anyhow!("Error reading from upstream: {}", e)),
        };
        
        total_bytes += n;
        stream.write_all(&response_buf[..n]).await?;
        
        // If we read less than the buffer size, we might be done
        if n < response_buf.len() {
            // Try to read one more time with a small timeout
            if tokio::time::timeout(
                tokio::time::Duration::from_millis(100),
                upstream.read(&mut response_buf),
            ).await.is_err() {
                break;
            }
        }
    }
    
    info!("HTTP request completed, sent {} bytes back to client", total_bytes);
    Ok(())
}
