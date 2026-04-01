# VZGLYD Kernel

Platform-agnostic display engine core for VZGLYD.

## Overview

This crate contains the platform-agnostic core of the VZGLYD display engine. It handles:

- Slide scheduling and playlist management
- Transition resolution and state machines
- Shader validation against the VZGLYD contract
- Slide manifest parsing and validation
- SlideSpec decoding (postcard wire format)
- Engine state machine and frame timing
- Render command generation

The kernel is designed to work with any graphics backend through the `Host` trait.

## Architecture

```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│  Native Host    │     │   VZGLYD Kernel  │     │   WebGPU Host   │
│  (wgpu/winit)   │────▶│  (platform-agnostic)│◀────│  (web)          │
└─────────────────┘     └──────────────────┘     └─────────────────┘
        │                       │                        │
        └───────────────────────┼────────────────────────┘
                                │
                   ┌────────────▼────────────┐
                   │    Host Trait           │
                   │  - request_data()       │
                   │  - submit_render_commands()│
                   │  - log()                │
                   │  - now()                │
                   └─────────────────────────┘
```

## Usage

Implement the `Host` trait for your platform, then use `Engine` to run the display engine:

```rust
use vzglyd_kernel::{Engine, EngineInput, Host, LogLevel, RenderCommand};

struct MyHost {
    // Platform-specific state
}

impl Host for MyHost {
    fn request_data(&mut self, key: &str) -> Option<Vec<u8>> {
        // Load data from filesystem, network, etc.
        None
    }

    fn submit_render_commands(&mut self, cmds: &[RenderCommand]) {
        // Execute render commands using native graphics API
    }

    fn log(&mut self, level: LogLevel, msg: &str) {
        // Log message
    }

    fn now(&self) -> f32 {
        // Return current time in seconds
        0.0
    }
}

let mut engine = Engine::new();
let mut host = MyHost { /* ... */ };

// Initialize
engine.init(&mut host);

// Main loop
loop {
    let input = EngineInput {
        dt: 0.016, // 60 FPS
        events: vec![],
    };
    let output = engine.update(&mut host, input);
    // output.commands contains render commands to execute
}
```

## Modules

- **`types`**: Core types including `Host`, `RenderCommand`, `EngineInput`, `EngineOutput`
- **`kernel`**: Main engine state machine (`Engine`)
- **`slide`**: Slide manifest parsing, SlideSpec decoding, validation
- **`schedule`**: Playlist parsing and schedule management
- **`transition`**: Transition types and state machine
- **`lifecycle`**: Slide lifecycle management (states, events)
- **`shader`**: Shader validation against the VZGLYD contract

## Platform Abstraction

The `Host` trait provides the platform abstraction layer:

```rust
pub trait Host {
    /// Request external data asynchronously
    fn request_data(&mut self, key: &str) -> Option<Vec<u8>>;

    /// Submit render commands for immediate execution
    fn submit_render_commands(&mut self, cmds: &[RenderCommand]);

    /// Log a message at the specified level
    fn log(&mut self, level: LogLevel, msg: &str);

    /// Get the current time in seconds
    fn now(&self) -> f32;
}
```

### What the Kernel Does NOT Do

The kernel intentionally avoids:

- **File system access** (`std::fs`) - handled by the host
- **Threading** (`std::thread`) - handled by the host
- **Window management** (`winit`) - handled by the host
- **Direct GPU calls** (`wgpu`, `WebGL`, `WebGPU`) - abstracted as `RenderCommand`
- **WASM instantiation** (`wasmtime`, `wasm3`) - handled by the host
- **Logging backends** (`env_logger`) - abstracted through `Host::log()`

## Targets

This crate compiles to:

- **Native** (`x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`)
- **WASM** (`wasm32-wasip1`)

```bash
# Native
cargo check

# WASM
cargo check --target wasm32-wasip1
```

## Testing

```bash
cargo test
```

All tests pass without requiring a display or GPU.

## Phase 1 Status

This crate represents **Phase 1** of the VZGLYD kernel extraction:

✅ Define and isolate the kernel boundary  
✅ Create platform abstraction (`Host` trait)  
✅ Extract pure logic (transitions, scheduling, validation)  
✅ Define core loop (`Engine::update()`)  
✅ Compile to native and WASM targets  

### Future Phases

**Phase 2:** Update native host (`lume/`) to implement `Host` trait  
**Phase 3:** Create `vzglyd-webgpu` host (TypeScript + WebGPU)  
**Phase 4:** Create `vzglyd-webgl` host (TypeScript + WebGL2)  

## License

MIT OR Apache-2.0 (same as parent VZGLYD project)
