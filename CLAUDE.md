# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build Commands
- `npm run build`: Build all packages using Turbo
- `npm run test`: Run Vitest tests across the monorepo
- `npm run lint`: Run ESLint and format checks in parallel
- `npm run format`: Format Rust code using cargo fmt
- `npm run setup`: Initialize development environment (recommended for first-time setup)
- `cargo run --bin chat_cli`: Run q chat locally for development
- `cargo test -p <crate_name>`: Run tests for a specific Rust crate
- `cargo test --locked --workspace`: Run all Rust tests
- `cargo +nightly fmt`: Format Rust code
- `cargo clippy --locked --workspace`: Run Rust linter

## Code Style Preferences
- Use ES modules (import/export) syntax, not CommonJS (require)
- Destructure imports when possible (e.g., `import { foo } from 'bar'`)
- Use TypeScript for all new JavaScript code
- Follow existing naming conventions (camelCase for variables, PascalCase for classes)
- Add JSDoc comments for public APIs
- Use async/await instead of Promise chains
- Prefer const/let over var
- For Rust code, use Rust 2024 edition features
- Run `cargo +nightly fmt` before committing Rust changes

## Workflow Guidelines
- Always run `cargo clippy` and fix warnings before committing Rust code
- Run tests before committing changes (`npm run test` for JS, `cargo test` for Rust)
- Use meaningful commit messages following conventional commits
- The project uses pre-commit hooks - ensure they pass
- For chat-related changes, test with `cargo run --bin chat_cli`

## Project Architecture
This is the Amazon Q Developer CLI, an AI-powered command-line tool with these key components:

### Core Crates (Rust)
- **q_cli**: Main CLI providing the `q` command interface
- **chat_cli**: Interactive chat functionality with AI assistant
- **fig_desktop**: Desktop app using tao/wry for windowing
- **figterm**: Headless terminal/pseudoterminal management
- **fig_input_method**: Autocomplete engine and input handling

### Web Applications (TypeScript/React)
- **packages/autocomplete**: React-based autocomplete UI
- **packages/dashboard-app**: Web dashboard interface
- **packages/shared**: Shared utilities and components

### Key Features
- **Auto Completion**: IDE-style completions for git, npm, docker, aws, etc.
- **Natural Language Chat**: Interactive terminal chat with contextual awareness
- **Tool System**: Chat includes tools for file operations (fs_read, fs_write), command execution (execute_bash), and AWS interactions (use_aws)
- **Cross-platform**: Supports macOS, Linux, and partial Windows support

### Architecture Patterns
- **Monorepo**: Uses Turbo for orchestrating builds across packages
- **IPC Communication**: Protocol buffers for inter-process communication (see proto/)
- **Event-driven**: Components communicate via events
- **AWS Integration**: Multiple AWS SDK clients for CodeWhisperer, Q Developer services

## Important Notes
- This is a monorepo - use `pnpm` (v10) for JavaScript dependencies
- Node version: 22.x (managed via mise)
- Python version: 3.11 (for build scripts)
- The chat interface requires user confirmation for file writes and command execution
- When modifying chat tools, check `q_cli/src/cli/chat/tools/` directory
- For AWS-related changes, ensure proper authentication handling

## Custom Model Integration Analysis
Based on analysis of reference implementation at `/Users/srini/workspace/karsun_ws/amazon-q-developer-cli`:

### Current Model Architecture
- **Model Selection**: Static `MODEL_OPTIONS` array in `/crates/chat-cli/src/cli/chat/cli/model.rs:24`
- **API Client**: Dual-path architecture in `/crates/chat-cli/src/api_client/mod.rs:251`
  - CodewhispererStreamingClient for Bearer token auth
  - QDeveloperStreamingClient for SigV4 auth
- **Request Flow**: ConversationState → UserInputMessage → AWS Streaming API
- **Tool System**: Full integration via UserInputMessageContext with tools, env_state, git_state

### Reference Custom Model Implementation
Found complete implementation with:
- **Configuration**: JSON config at `~/.config/amazon-q/config.json` with custom_models section
- **HTTP Client**: CustomModelClient in `/api_client/custom_model.rs` with streaming support
- **Proxy Server**: Python FastAPI server in `/server/bedrock_proxy/` directory
  - Endpoints: `POST /chat/stream`, `POST /chat`, `GET /health`
  - Request format: ChatRequest with modelId, message, tools, envContext, systemPrompt
  - Response: Server-Sent Events with `data: {json}\n\n` format
  - Bedrock integration: Uses Converse Stream API with tool passthrough

### Integration Points for Custom Models
1. **Model Selection**: Modify `MODEL_OPTIONS` to load from config + built-in models
2. **API Routing**: Add custom model detection in `send_message()` method
3. **HTTP Client**: Port CustomModelClient with exact proxy server compatibility
4. **Tool Preservation**: Maintain all tool specs, system prompts, environment context
5. **Streaming**: Handle SSE parsing for real-time responses

### Required Files to Port
- `/api_client/config.rs` - Configuration management
- `/api_client/custom_model.rs` - HTTP client implementation  
- `/cli/custom_model.rs` - CLI commands (optional)
- Error handling updates in `/api_client/error.rs`

## Debugging
- Chat logs: Run with `RUST_LOG=debug cargo run --bin chat_cli`
- Check GitHub Actions workflows in `.github/workflows/` for CI configuration
- Integration tests are in `tests/` directory
- Use `q chat` itself for codebase exploration - add `/context add codebase-summary.md`