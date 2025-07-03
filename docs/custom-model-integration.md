# Custom Model Integration for Amazon Q Developer CLI

## Problem Statement

The Amazon Q Developer CLI currently supports only built-in models (claude-3-5-sonnet, claude-3-5-haiku, claude-3-opus) that directly integrate with AWS Bedrock services. Users need the ability to integrate custom models through proxy services while maintaining all existing functionality including:

- Streaming responses
- Tool integration 
- Authentication mechanisms
- Model selection via `/model` command
- Non-interactive mode support

### Key Requirements

1. **Pure Proxy Architecture**: Custom models should act as HTTP proxies that forward requests to existing AWS Bedrock models
2. **No Changes to Built-in Models**: Existing model functionality must remain unchanged
3. **Dynamic Model Discovery**: Custom models should appear in `/model` command alongside built-in options
4. **Configuration Management**: External JSON configuration for custom model endpoints and authentication
5. **Feature Parity**: Custom models must support all CLI features (streaming, tools, non-interactive mode)

## Design Approach

### 1. Dual Client Architecture

The solution implements a dual-client system:

- **Built-in Models**: Continue using existing `CodewhispererStreamingClient` and `QDeveloperStreamingClient`
- **Custom Models**: New `CustomModelClient` for HTTP proxy communication

### 2. Dynamic Model Loading

Instead of static model arrays, the system now uses dynamic loading:

```rust
// Before: Static array
const MODEL_OPTIONS: &[ModelOption] = &[/* built-in models */];

// After: Dynamic function
pub fn get_all_model_options() -> Result<Vec<ModelOption>, ChatError> {
    let mut options = get_builtin_models().to_vec();
    if let Ok(custom_models) = load_custom_models() {
        options.extend(custom_models);
    }
    Ok(options)
}
```

### 3. Configuration-Driven Custom Models

Custom models are defined in `~/.config/amazon-q/config.json`:

```json
{
  "custom_models": {
    "custom:test-model": {
      "name": "Test Model",
      "base_url": "http://localhost:8000",
      "token_header": "Bearer YOUR_TOKEN_HERE",
      "description": "Test custom model for development"
    }
  }
}
```

### 4. Request Routing

The system routes requests based on model ID prefix:

```rust
pub async fn send_message(&mut self, conversation: &Conversation) -> Result<SendMessageOutput, ChatError> {
    let model_id = &conversation.model_id;
    
    if model_id.starts_with("custom:") {
        return self.handle_custom_model_request(conversation).await;
    }
    
    // Existing built-in model handling
    self.handle_builtin_model_request(conversation).await
}
```

## Architectural Changes

### New Components

1. **`/crates/chat-cli/src/api_client/config.rs`**
   - Configuration management for custom models
   - JSON serialization/deserialization
   - File system operations for config loading/saving

2. **`/crates/chat-cli/src/api_client/custom_model.rs`**
   - HTTP client for custom model communication
   - Server-Sent Events (SSE) parsing for streaming
   - Error handling and response processing

### Modified Components

1. **`/crates/chat-cli/src/cli/chat/cli/model.rs`**
   - Changed from static to dynamic model loading
   - Integration of custom models in model discovery

2. **`/crates/chat-cli/src/api_client/mod.rs`**
   - Added custom model request routing
   - New `handle_custom_model_request()` method

3. **`/crates/chat-cli/src/cli/chat/mod.rs`**
   - Updated model validation to use dynamic loading
   - Enhanced non-interactive mode support

### Data Flow

```
User Request
     ↓
Model ID Check
     ↓
┌─────────────────┬─────────────────┐
│   Built-in      │     Custom      │
│   Models        │     Models      │
│                 │                 │
│ AWS Bedrock ←─── │ ──→ HTTP Proxy  │
│ (SigV4/Bearer)  │    (Bearer)     │
└─────────────────┴─────────────────┘
     ↓                     ↓
Streaming Response    SSE Parsing
     ↓                     ↓
     └─── Unified Output ──┘
```

## Request/Response JSON Payloads

### Built-in Models

#### Request Format (AWS Bedrock Converse API)
```json
{
  "modelId": "anthropic.claude-3-5-sonnet-20241022-v2:0",
  "messages": [
    {
      "role": "user",
      "content": [
        {
          "text": "Hello, how are you?"
        }
      ]
    }
  ],
  "inferenceConfig": {
    "maxTokens": 4096,
    "temperature": 0.1,
    "topP": 0.9
  },
  "toolConfig": {
    "tools": [
      {
        "toolSpec": {
          "name": "execute_bash",
          "description": "Execute bash commands",
          "inputSchema": {
            "json": {
              "type": "object",
              "properties": {
                "command": {
                  "type": "string",
                  "description": "The command to execute"
                }
              },
              "required": ["command"]
            }
          }
        }
      }
    ]
  }
}
```

#### Response Format (Streaming)
```json
{
  "messageStart": {
    "role": "assistant"
  }
}

{
  "contentBlockDelta": {
    "delta": {
      "text": "Hello! I'm doing well, thank you for asking."
    },
    "contentBlockIndex": 0
  }
}

{
  "messageStop": {
    "stopReason": "end_turn"
  }
}
```

### Custom Models

#### Request Format (HTTP Proxy)
```json
{
  "model": "claude-3-5-sonnet",
  "messages": [
    {
      "role": "user",
      "content": "Hello, how are you?"
    }
  ],
  "max_tokens": 4096,
  "temperature": 0.1,
  "top_p": 0.9,
  "tools": [
    {
      "name": "execute_bash",
      "description": "Execute bash commands",
      "input_schema": {
        "type": "object",
        "properties": {
          "command": {
            "type": "string",
            "description": "The command to execute"
          }
        },
        "required": ["command"]
      }
    }
  ],
  "stream": true
}
```

#### Response Format (Server-Sent Events)
```
data: {"type": "message_start", "message": {"role": "assistant"}}

data: {"type": "content_block_delta", "delta": {"type": "text_delta", "text": "Hello! I'm doing well, thank you for asking."}}

data: {"type": "message_delta", "delta": {"stop_reason": "end_turn"}}

data: [DONE]
```

## Key Implementation Details

### 1. Authentication Mechanisms

**Built-in Models:**
- **CodewhispererStreamingClient**: Uses Bearer token authentication
- **QDeveloperStreamingClient**: Uses AWS SigV4 signing

**Custom Models:**
- HTTP Bearer token from configuration
- Configurable token header format

### 2. Streaming Implementation

**Built-in Models:**
- Native AWS SDK streaming support
- Automatic reconnection and error handling

**Custom Models:**
- Manual SSE parsing using `reqwest` with `EventSource` pattern
- Custom parsing logic for different event types:
  ```rust
  fn parse_sse_line(line: &str) -> Option<SseEvent> {
      if line.starts_with("data: ") {
          let data = &line[6..];
          if data == "[DONE]" {
              return Some(SseEvent::Done);
          }
          // Parse JSON data
      }
      None
  }
  ```

### 3. Tool Integration

Both built-in and custom models support full tool integration:

**Tool Definition Format:**
- Built-in: AWS Bedrock `toolSpec` format
- Custom: Anthropic API `input_schema` format

**Tool Response Handling:**
- Built-in: Native AWS SDK tool result processing
- Custom: Manual JSON parsing and response formatting

### 4. Error Handling

**Built-in Models:**
```rust
match client.send_message(&request).await {
    Ok(response) => process_builtin_response(response),
    Err(SdkError::ServiceError(service_err)) => handle_aws_error(service_err),
    Err(other) => handle_sdk_error(other),
}
```

**Custom Models:**
```rust
match http_client.post(&url).json(&request).send().await {
    Ok(response) if response.status().is_success() => process_sse_stream(response),
    Ok(response) => handle_http_error(response.status(), response.text().await?),
    Err(reqwest_err) => handle_network_error(reqwest_err),
}
```

## Testing and Validation

### Test Coverage

1. **Model Discovery**: Verify custom models appear in `/model` command
2. **Request Routing**: Confirm proper routing based on model ID prefix
3. **Streaming Responses**: Test SSE parsing and response streaming
4. **Tool Integration**: Validate tool calls work with custom models
5. **Non-Interactive Mode**: Ensure multi-step tasks complete successfully
6. **Error Handling**: Test various failure scenarios

### Test Commands

```bash
# Test model discovery
./target/release/chat_cli chat /model

# Test custom model interaction
./target/release/chat_cli chat --model custom:test-model "Hello"

# Test non-interactive with custom model
./target/release/chat_cli chat --model custom:test-model --non-interactive "Create a file"

# Test tool integration
./target/release/chat_cli chat --model custom:test-model --trust-all-tools "List files"
```

## Future Enhancements

1. **Multi-Provider Support**: Support for different API formats (OpenAI, Anthropic, etc.)
2. **Custom Authentication**: Support for API keys, OAuth, and other auth methods
3. **Model Capabilities**: Dynamic capability detection for different custom models
4. **Performance Monitoring**: Metrics and logging for custom model performance
5. **Configuration UI**: Web interface for managing custom model configurations

## Conclusion

The custom model integration successfully extends the Amazon Q Developer CLI to support external model providers while maintaining full feature parity with built-in models. The pure proxy architecture ensures no changes to existing functionality while providing a flexible foundation for future enhancements.