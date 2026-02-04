# AI Agent Guidelines for Obelisk WebUI

This document provides guidelines for AI agents working on this repository.

**Obelisk** is a deterministic workflow engine for durable execution. Learn more at [obeli.sk](https://obeli.sk/).

## Project Overview

This is the Web UI for Obelisk, built with:
- **Yew** - Rust framework for building web applications compiled to WebAssembly
- **gRPC-Web** - Communication with the Obelisk server via `tonic-web-wasm-client`
- **Custom tree component** - Custom tree/icon components (in `crates/webui/src/tree/`)
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

- `ExecutionRepository` - Execution management:
  - `ListExecutions`, `ListExecutionEvents`, `GetStatus` - Query executions
  - `Submit`, `Stub`, `Cancel` - Control executions
  - `ReplayExecution` - Replay a workflow execution
  - `UpgradeExecutionComponent` - Upgrade workflow to new component version
- `FunctionRepository` - Component and function listing, WIT retrieval
- `DeploymentRepository` - Deployment state listing

The proto is compiled in `build.rs` and available via `crate::grpc::grpc_client`.

### Using gRPC Clients

```rust
use crate::grpc::grpc_client::{
    self,
    execution_repository_client::ExecutionRepositoryClient,
};
use tonic_web_wasm_client::Client;
use crate::BASE_URL;

let mut client = ExecutionRepositoryClient::new(Client::new(BASE_URL.to_string()));
let response = client.replay_execution(grpc_client::ReplayExecutionRequest {
    execution_id: Some(execution_id),
}).await;
```

All gRPC calls are made via `tonic-web-wasm-client` which works in the browser WASM environment.

### Key Types

- `ExecutionId` - Unique identifier for executions (format: `E_<ulid>`)
- `ContentDigest` - Component hash (format: `sha256:<64 hex chars>`)
- `ComponentId` - Contains `component_type`, `name`, and `digest`
- `ComponentType` - Enum: `Workflow`, `ActivityWasm`, `WebhookEndpoint`, etc.

## Pagination Pattern

For paginated lists, follow the pattern in `execution_list_page.rs`:

1. Use URL query parameters for filter/pagination state
2. Define cursor types matching the gRPC pagination messages
3. Use `use_effect_with` to fetch data when query changes
4. Provide "Newer" / "Older" navigation buttons

## Yew Patterns and Gotchas

### State in Async/Interval Callbacks

When using `use_state` with interval callbacks or async closures, the captured state handle reads from the state **at closure creation time**, not the current value. This is a common pitfall.

**Problem:**
```rust
let counter = use_state(|| 0);
use_effect_with((), move |()| {
    let counter = counter.clone();
    Interval::new(1000, move || {
        // BUG: *counter always returns the initial value (0)
        log::info!("Counter: {}", *counter);
    });
});
```

**Solution:** Use `use_mut_ref` for values that need to be read/updated across async boundaries:
```rust
let counter = use_state(|| 0);
let counter_ref = use_mut_ref(|| 0);

use_effect_with((), move |()| {
    Interval::new(1000, move || {
        // Correct: reads current value
        let current = *counter_ref.borrow();
        log::info!("Counter: {}", current);
    });
});
```

### Timers

Use `gloo::timers::callback::Interval` (not `gloo_timers`):
```rust
use gloo::timers::callback::Interval;

let interval = Interval::new(5000, || {
    // Called every 5 seconds
});
// Drop the interval to cancel it
```

## Submodule Management

The `obelisk/` directory is a git submodule containing proto definitions. If you need newer proto definitions:

```bash
git submodule update --remote obelisk
```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix all warnings
- Follow existing patterns in the codebase
- Use workspace dependencies from root `Cargo.toml`

**Note:** Use `nix develop -c` prefix for cargo commands to ensure correct toolchain:
```bash
nix develop -c cargo clippy
nix develop -c cargo fmt
```

## Notification System

The app uses a unified notification/toast system for displaying success, error, and info messages.

### Usage

```rust
use crate::components::notification::{Notification, NotificationContext};

// In a component:
let notifications = use_context::<NotificationContext>()
    .expect("NotificationContext should be provided");

// Push notifications:
notifications.push(Notification::success("Operation completed"));
notifications.push(Notification::error(format!("Failed: {}", e.message())));
notifications.push(Notification::info("Processing..."));
```

### Guidelines

- **Always notify on gRPC errors** - When any RPC call fails, push an error notification
- **Use success notifications sparingly** - Only for user-initiated actions (button clicks, form submissions)
- **Include error details** - Use `e.message()` from tonic errors for context
- **Connection status** - The app automatically notifies when server connection is lost/restored

### Implementation

- `NotificationProvider` wraps the app in `app.rs`
- `NotificationContext` is available to all components via `use_context`
- Notifications auto-dismiss after 5 seconds with fade-out animation
- Users can manually dismiss via the X button
- Styles in `styles/components/_notifications.scss`

## Tree Component

The tree component (`crates/webui/src/tree/`) is a custom implementation replacing the archived yewprint library.

### Architecture

- `icon.rs` - Icon enum using Unicode characters for cross-platform support
- `tree_data.rs` - `TreeData<T>` wrapper around `id_tree::Tree` with `RefCell` for interior mutability
- `tree_view.rs` - `Tree` function component that renders the tree

