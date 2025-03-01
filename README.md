# Rust Forward Proxy

A high-performance forward proxy implementation in Rust that automatically handles authentication for upstream proxies (i.e. oxylabs).

Useful if you want to use geckodriver (firefox) instead of chrome with authenticated proxies.

## Usage

### Build the proxy

```bash
cargo build --release
```

### Run with environment variables

```bash
# Set proxy details as environment variables
export PROXY_HOST=squid
export PROXY_PORT=3128
export PROXY_USER=testuser
export PROXY_PASSWORD=testpass

# Run the proxy
./target/release/forward-proxy
```

### Run with command-line arguments

```bash
./target/release/forward-proxy \
  --local-host 127.0.0.1 \
  --local-port 8118 \
  --proxy-host squid \
  --proxy-port 3128 \
  --proxy-user testuser \
  --proxy-password testpass
```

## Testing

Configure browser or curl to use the local proxy at 127.0.0.1:8118. The proxy will handle authentication with the upstream proxy automatically.

```bash
curl -v --proxy http://127.0.0.1:8118 http://httpbin.org/ip
```
