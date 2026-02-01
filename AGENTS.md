# AI Agent Guidelines for Obelisk WebUI

This document provides guidelines for AI agents working on this repository.

**Obelisk** is a deterministic workflow engine for durable execution. Learn more at [obeli.sk](https://obeli.sk/).

## Project Overview

This is the Web UI for Obelisk, built with:
- **Yew** - Rust framework for building web applications compiled to WebAssembly
- **gRPC-Web** - Communication with the Obelisk server via `tonic-web-wasm-client`
- **yewprint** - Blueprint.js components for Yew
- **Trunk** - WASM web application bundler

## Development Environment

### Using Nix (Recommended)

This project uses Nix flakes to manage all dependencies.

**Installing Nix with flakes enabled:**

```bash
# Using the Determinate Systems installer (recommended)
curl --proto '=https' --tlsv1.2 -sSf -L https://install.determinate.systems/nix | sh -s -- install
```

**Configure Garnix cache for faster builds:**

```bash
cat << 'EOF' | sudo tee -a /etc/nix/nix.conf
extra-substituters = https://cache.garnix.io
extra-trusted-public-keys = cache.garnix.io:CTFPyKSLcx5RMJKfLo5EEPUObbA78b0YQ2DTCJXqr9g
EOF
sudo systemctl restart nix-daemon.service
```

**Enter the development shell:**

```bash
nix develop
```

This provides all necessary tools: Rust toolchain with WASM targets, trunk, protobuf, etc.

### Without Nix

If Nix is unavailable, install tools matching versions in `dev-deps.txt` and `rust-toolchain.toml`.

## Project Structure

```
webui/
├── crates/
│   ├── webui/              # Main WebUI application
│   │   ├── src/
│   │   │   ├── app.rs      # Routes and main App component
│   │   │   ├── components/ # UI components (pages, widgets)
│   │   │   ├── grpc/       # gRPC client and type helpers
│   │   │   └── util/       # Utilities (time formatting, colors)
│   │   ├── build.rs        # Proto compilation, CSS generation
│   │   └── Trunk.toml      # Trunk bundler configuration
│   └── webui-proxy/        # Development proxy server
├── obelisk/                # Git submodule with obelisk proto definitions
│   └── proto/
│       └── obelisk.proto   # gRPC service definitions
└── flake.nix               # Nix development environment
```

### Key Directories

- `crates/webui/src/components/` - Yew components for pages and UI elements
- `crates/webui/src/grpc/` - gRPC client (`grpc_client.rs`) and type wrappers
- `crates/webui/src/app.rs` - Route definitions and main App component

## Building and Running

```bash
# Enter nix shell
nix develop

# Start development server on port 8081
just serve
```

The WebUI will be available at http://localhost:8081

### Building for Release

```bash
just build
```

This creates:
- Release WASM files in `crates/webui/dist/`
- The `webui-proxy` component (see [webui-proxy README](crates/webui-proxy/README.md))

## Adding a New Page

1. **Create component file** in `crates/webui/src/components/`
   - Follow patterns from existing pages like `execution_list_page.rs`
   - Use Yew's `#[function_component]` macro

2. **Register module** in `crates/webui/src/components/mod.rs`

3. **Add route** in `crates/webui/src/app.rs`:
   - Add variant to `Route` enum with `#[at("/path")]` attribute
   - Add render case in `Route::render()`
   - Add navigation link in the `App` component if needed

4. **gRPC client usage**:
   ```rust
   use crate::grpc::grpc_client::{self, service_client::ServiceClient};
   use tonic_web_wasm_client::Client;
   use crate::BASE_URL;
   
   let mut client = ServiceClient::new(Client::new(BASE_URL.to_string()));
   let response = client.method(Request { ... }).await;
   ```

## gRPC Services

The proto definitions are in `obelisk/proto/obelisk.proto`. Available services:

- `ExecutionRepository` - Execution management (list, submit, status, events, cancel, stub)
- `FunctionRepository` - Component and function listing, WIT retrieval
- `DeploymentRepository` - Deployment state listing

The proto is compiled in `build.rs` and available via `crate::grpc::grpc_client`.

### Using gRPC Clients

```rust
use crate::grpc::grpc_client::{
    self,
    service_repository_client::ServiceRepositoryClient,
};
use tonic_web_wasm_client::Client;
use crate::BASE_URL;

let mut client = ServiceRepositoryClient::new(Client::new(BASE_URL.to_string()));
let response = client.method(grpc_client::RequestType { ... }).await;
```

All gRPC calls are made via `tonic-web-wasm-client` which works in the browser WASM environment.

## Pagination Pattern

For paginated lists, follow the pattern in `execution_list_page.rs`:

1. Use URL query parameters for filter/pagination state
2. Define cursor types matching the gRPC pagination messages
3. Use `use_effect_with` to fetch data when query changes
4. Provide "Newer" / "Older" navigation buttons

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix all warnings
- Follow existing patterns in the codebase
- Use workspace dependencies from root `Cargo.toml`

## Git Submodule

The `obelisk/` directory is a git submodule pointing to the main obelisk repository.
To update:

```bash
# Using HTTPS (recommended for CI/read-only)
git clone https://github.com/obeli-sk/obelisk.git obelisk --branch latest

# Or update existing submodule
cd obelisk && git pull origin main
```

Note: The `.gitmodules` uses SSH URL which requires GitHub SSH access.
For read-only access, clone via HTTPS as shown above.
