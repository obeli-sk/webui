# Obelisk WebUI

A [Yew](https://yew.rs)-based web interface for [Obelisk](https://obeli.sk/), a deterministic workflow engine for durable execution.

## Features

- **Execution List** - Browse, filter, and paginate through workflow executions
- **Deployment List** - View deployment states and execution counts
- **Component List** - Explore registered components and their interfaces
- **Execution Detail** - Inspect execution events, traces, and logs
- **Debugger** - Step through execution history with source mapping

## Development

### Prerequisites

This project uses [Nix flakes](https://nixos.wiki/wiki/Flakes) for dependency management.

```bash
# Install Nix (recommended: Determinate Systems installer)
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install

# Configure Garnix cache for faster builds
cat << 'EOF' | sudo tee -a /etc/nix/nix.conf
extra-substituters = https://cache.garnix.io
extra-trusted-public-keys = cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g
EOF
sudo systemctl restart nix-daemon.service
```

### Running Locally

Enter the Nix development shell and start the development server:

```bash
nix develop
just serve
```

The WebUI will be available at [http://localhost:8081](http://localhost:8081).

Make sure the Obelisk server is running for gRPC connectivity.

### Building for Release

```bash
nix develop
just build
```

This creates:
- Release WASM files in `crates/webui/dist/`
- The `webui-proxy` component at `target/wasm32-wasip2/release/webui_proxy.wasm`

See [webui-proxy README](crates/webui-proxy/README.md) for deployment instructions.

## Project Structure

```
webui/
├── crates/
│   ├── webui/              # Main WebUI application (Yew + WASM)
│   └── webui-proxy/        # Webhook component for serving WebUI
├── obelisk/                # Git submodule with proto definitions
├── Justfile                # Build commands
└── flake.nix               # Nix development environment
```

## License

AGPL-3.0-only - See [LICENSE-AGPL](LICENSE-AGPL) for details.
