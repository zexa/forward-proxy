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

## Docker

### Using with Docker

```bash
# Build the Docker image
docker build -t forward-proxy .

# Run the container
docker run -p 8118:8118 \
  -e PROXY_HOST=your-proxy-host \
  -e PROXY_PORT=3128 \
  -e PROXY_USER=your-username \
  -e PROXY_PASSWORD=your-password \
  forward-proxy
```

### Using Docker Compose

We provide a `compose.yml` file for easier deployment:

1. Edit the environment variables in `compose.yml` to match your proxy setup
2. Run the service:

```bash
docker compose up -d
```

### Configuration

The proxy can be configured using the following environment variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `LOCAL_HOST` | Address the forward proxy listens on | `0.0.0.0` |
| `LOCAL_PORT` | Port the forward proxy listens on | `8118` |
| `PROXY_HOST` | Hostname of your upstream authenticated proxy | - |
| `PROXY_PORT` | Port of your upstream authenticated proxy | `3128` |
| `PROXY_USER` | Username for upstream proxy authentication | - |
| `PROXY_PASSWORD` | Password for upstream proxy authentication | - |
