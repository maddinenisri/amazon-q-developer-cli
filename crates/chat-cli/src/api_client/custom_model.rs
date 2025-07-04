use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::Stream;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::debug;

use crate::api_client::error::ApiClientError;
use crate::api_client::model::{
    ChatResponseStream,
    ConversationState,
    ToolResult,
};

/// Configuration for a custom model proxy endpoint
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomModelConfig {
    /// Base URL for the custom model proxy endpoint
    pub base_url: String,
    /// API key for authentication (optional)
    pub api_key: Option<String>,
    /// Model identifier to use with Bedrock Converse API
    pub model_id: String,
    /// Request timeout in seconds
    pub timeout_seconds: Option<u64>,
    /// Additional headers to include in requests
    pub headers: Option<HashMap<String, String>>,
}

/// Request structure for the custom model proxy
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomModelRequest {
    /// The model ID to use with Bedrock Converse API
    pub model_id: String,
    /// The user message content
    pub message: String,
    /// Conversation ID for maintaining state
    pub conversation_id: Option<String>,
    /// Chat history as simple message pairs
    pub history: Option<Vec<SimpleMessage>>,
    /// Tool specifications available to the model
    pub tools: Option<Vec<ToolSpecification>>,
    /// Environment context including current working directory
    pub env_context: Option<EnvContext>,
    /// System prompt that provides Q identity and capabilities
    pub system_prompt: Option<String>,
    /// Additional parameters for the model
    pub parameters: Option<HashMap<String, serde_json::Value>>,
    /// Tool results from previous tool executions
    pub tool_results: Option<Vec<ToolResult>>,
}

/// SSE parser that handles line buffering for proper event reconstruction
struct SseParser {
    buffer: String,
    done_received: bool,
}

impl SseParser {
    fn new() -> Self {
        Self {
            buffer: String::new(),
            done_received: false,
        }
    }

    /// Parse a chunk of SSE data, returning events and whether the stream is complete
    fn parse_chunk(&mut self, chunk: &str) -> (Vec<Result<ChatResponseStream, ApiClientError>>, bool) {
        // Add chunk to buffer
        self.buffer.push_str(chunk);

        let mut events = Vec::new();

        // Process complete lines from buffer
        while let Some(line_end) = self.buffer.find('\n') {
            let line = self.buffer[..line_end].to_string();
            self.buffer.drain(..line_end + 1);

            // Parse SSE line
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    debug!("Received [DONE] event - stream complete");
                    self.done_received = true;
                    // Don't break - continue processing any remaining buffered lines
                    continue;
                }

                match serde_json::from_str::<serde_json::Value>(data) {
                    Ok(event_json) => {
                        if let Some(event) = Self::convert_proxy_event_to_chat_stream(&event_json) {
                            events.push(Ok(event));
                        }
                    },
                    Err(e) => {
                        debug!("Failed to parse SSE event: {}", e);
                        events.push(Err(ApiClientError::CustomModel {
                            message: format!("Failed to parse streaming event: {}", e),
                            status_code: None,
                        }));
                    },
                }
            }
        }

        (events, self.done_received)
    }

    /// Check if we've received the [DONE] event
    fn is_done(&self) -> bool {
        self.done_received
    }

    /// Process any remaining data in the buffer when stream ends
    fn flush(&mut self) -> Vec<Result<ChatResponseStream, ApiClientError>> {
        let mut events = Vec::new();

        // If there's data in the buffer without a trailing newline, process it
        if !self.buffer.is_empty() {
            debug!("Flushing remaining buffer data: {}", self.buffer);

            // Process as if it had a newline
            let remaining = self.buffer.clone();
            self.buffer.clear();

            if let Some(data) = remaining.strip_prefix("data: ") {
                if data == "[DONE]" {
                    debug!("Found [DONE] in remaining buffer");
                    self.done_received = true;
                } else {
                    match serde_json::from_str::<serde_json::Value>(data) {
                        Ok(event_json) => {
                            if let Some(event) = Self::convert_proxy_event_to_chat_stream(&event_json) {
                                events.push(Ok(event));
                            }
                        },
                        Err(e) => {
                            debug!("Failed to parse remaining SSE event: {}", e);
                            events.push(Err(ApiClientError::CustomModel {
                                message: format!("Failed to parse final streaming event: {}", e),
                                status_code: None,
                            }));
                        },
                    }
                }
            }
        }

        events
    }

    /// Convert proxy server event to ChatResponseStream
    fn convert_proxy_event_to_chat_stream(event: &serde_json::Value) -> Option<ChatResponseStream> {
        let event_type = event.get("type")?.as_str()?;

        match event_type {
            "text_delta" => {
                let text = event.get("text")?.as_str()?.to_string();
                Some(ChatResponseStream::AssistantResponseEvent { content: text })
            },
            "tool_use_start" => {
                let tool_use_id = event.get("tool_use_id")?.as_str()?.to_string();
                let name = event.get("name")?.as_str()?.to_string();
                Some(ChatResponseStream::ToolUseEvent {
                    tool_use_id,
                    name,
                    input: None,
                    stop: Some(false),
                })
            },
            "tool_use_delta" => {
                let tool_use_id = event.get("tool_use_id")?.as_str()?.to_string();
                let name = event.get("name")?.as_str()?.to_string();
                let input = event.get("input")?.as_str()?.to_string();
                Some(ChatResponseStream::ToolUseEvent {
                    tool_use_id,
                    name,
                    input: Some(input),
                    stop: Some(false),
                })
            },
            "content_block_stop" => {
                // Check if this is a tool use block by looking for tool_use_id
                if let (Some(tool_use_id), Some(name)) = (
                    event.get("tool_use_id").and_then(|v| v.as_str()),
                    event.get("name").and_then(|v| v.as_str()),
                ) {
                    Some(ChatResponseStream::ToolUseEvent {
                        tool_use_id: tool_use_id.to_string(),
                        name: name.to_string(),
                        input: None,
                        stop: Some(true), // This signals tool parsing completion and triggers execution
                    })
                } else {
                    // Regular content block stop, not a tool use
                    None
                }
            },
            "message_start" => {
                // Message started - no specific event needed
                None
            },
            "message_end" => {
                // Message ended - no specific event needed
                None
            },
            _ => {
                debug!("Unknown event type: {}", event_type);
                None
            },
        }
    }
}

/// Tool specification for the custom model
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ToolSpecification {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

/// Simplified message structure for serialization
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleMessage {
    pub role: String, // "user" or "assistant"
    pub content: String,
}

/// Environment context for the custom model
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EnvContext {
    /// Current working directory
    pub current_working_directory: Option<String>,
    /// Operating system
    pub operating_system: Option<String>,
}

/// Response structure from the custom model proxy
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct CustomModelResponse {
    /// The response content
    pub content: String,
    /// Tool calls made by the model
    pub tool_calls: Option<Vec<ToolCall>>,
    /// Conversation ID for maintaining state
    #[allow(dead_code)]
    pub conversation_id: Option<String>,
    /// Usage statistics
    pub usage: Option<UsageStats>,
    /// Any error information
    pub error: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageStats {
    pub input_tokens: Option<u32>,
    pub output_tokens: Option<u32>,
    pub total_tokens: Option<u32>,
}

impl UsageStats {
    #[allow(dead_code)]
    pub fn new(input: u32, output: u32) -> Self {
        Self {
            input_tokens: Some(input),
            output_tokens: Some(output),
            total_tokens: Some(input + output),
        }
    }

    #[allow(dead_code)]
    pub fn add(&mut self, other: &UsageStats) {
        if let Some(input) = other.input_tokens {
            self.input_tokens = Some(self.input_tokens.unwrap_or(0) + input);
        }
        if let Some(output) = other.output_tokens {
            self.output_tokens = Some(self.output_tokens.unwrap_or(0) + output);
        }
        if let Some(total) = other.total_tokens {
            self.total_tokens = Some(self.total_tokens.unwrap_or(0) + total);
        }
    }

    #[allow(dead_code)]
    pub fn format_summary(&self) -> String {
        match (self.input_tokens, self.output_tokens, self.total_tokens) {
            (Some(input), Some(output), Some(total)) => {
                format!(
                    "{} input tokens, {} output tokens, {} total tokens",
                    input, output, total
                )
            },
            (Some(input), Some(output), None) => {
                format!("{} input tokens, {} output tokens", input, output)
            },
            (None, None, Some(total)) => {
                format!("{} total tokens", total)
            },
            _ => "No usage data available".to_string(),
        }
    }
}

/// Client for interacting with custom model proxy endpoints
#[derive(Clone, Debug)]
pub struct CustomModelClient {
    config: CustomModelConfig,
    http_client: reqwest::Client,
}

impl CustomModelClient {
    #[allow(clippy::result_large_err)]
    pub fn new(config: CustomModelConfig) -> Result<Self, ApiClientError> {
        let timeout = Duration::from_secs(config.timeout_seconds.unwrap_or(30));

        let http_client =
            reqwest::Client::builder()
                .timeout(timeout)
                .build()
                .map_err(|e| ApiClientError::CustomModel {
                    message: format!("Failed to create HTTP client: {}", e),
                    status_code: None,
                })?;

        Ok(Self { config, http_client })
    }

    #[allow(dead_code)]
    pub async fn send_message(&self, conversation: ConversationState) -> Result<CustomModelResponse, ApiClientError> {
        self.send_message_with_conversation_id(conversation, None, None).await
    }

    #[allow(dead_code)]
    pub async fn send_message_with_conversation_id(
        &self,
        conversation: ConversationState,
        conversation_id: Option<String>,
        system_prompt: Option<String>,
    ) -> Result<CustomModelResponse, ApiClientError> {
        debug!("Sending message to custom model endpoint: {}", self.config.base_url);

        // Convert ConversationState to simplified format
        let mut history = Vec::new();
        if let Some(chat_history) = &conversation.history {
            for msg in chat_history {
                match msg {
                    crate::api_client::model::ChatMessage::UserInputMessage(user_msg) => {
                        history.push(SimpleMessage {
                            role: "user".to_string(),
                            content: user_msg.content.clone(),
                        });
                    },
                    crate::api_client::model::ChatMessage::AssistantResponseMessage(assistant_msg) => {
                        history.push(SimpleMessage {
                            role: "assistant".to_string(),
                            content: assistant_msg.content.clone(),
                        });
                    },
                }
            }
        }

        // Extract tool specifications from user input message context
        let tools = conversation
            .user_input_message
            .user_input_message_context
            .as_ref()
            .and_then(|ctx| ctx.tools.as_ref())
            .map(|tools| {
                tools
                    .iter()
                    .map(|tool| {
                        match tool {
                            crate::api_client::model::Tool::ToolSpecification(spec) => {
                                // Convert ToolInputSchema to serde_json::Value
                                let input_schema = spec.input_schema.json.as_ref().map_or(
                                    serde_json::Value::Object(serde_json::Map::new()),
                                    |fig_doc| {
                                        serde_json::to_value(fig_doc)
                                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
                                    },
                                );

                                ToolSpecification {
                                    name: spec.name.clone(),
                                    description: spec.description.clone(),
                                    input_schema,
                                }
                            },
                        }
                    })
                    .collect::<Vec<_>>()
            });

        // Extract environment context from user input message context
        let env_context = conversation
            .user_input_message
            .user_input_message_context
            .as_ref()
            .and_then(|ctx| ctx.env_state.as_ref())
            .map(|env_state| EnvContext {
                current_working_directory: env_state.current_working_directory.clone(),
                operating_system: env_state.operating_system.clone(),
            });

        // Extract tool results from context
        let tool_results = conversation
            .user_input_message
            .user_input_message_context
            .as_ref()
            .and_then(|ctx| ctx.tool_results.clone());

        let request_body = CustomModelRequest {
            model_id: self.config.model_id.clone(),
            message: conversation.user_input_message.content,
            conversation_id: conversation_id.or(conversation.conversation_id),
            history: if history.is_empty() { None } else { Some(history) },
            tools,
            env_context,
            system_prompt,
            parameters: None, // Can be extended for model-specific parameters
            tool_results,
        };

        let mut request_builder = self
            .http_client
            .post(format!("{}/chat", self.config.base_url))
            .json(&request_body);

        // Add API key if configured
        if let Some(api_key) = &self.config.api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        // Add custom headers if configured
        if let Some(headers) = &self.config.headers {
            for (key, value) in headers {
                request_builder = request_builder.header(key, value);
            }
        }

        let response = request_builder.send().await.map_err(|e| ApiClientError::CustomModel {
            message: format!("Failed to send request: {}", e),
            status_code: None,
        })?;

        let status_code = response.status().as_u16();

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiClientError::CustomModel {
                message: format!("Request failed: {}", error_text),
                status_code: Some(status_code),
            });
        }

        let custom_response: CustomModelResponse = response.json().await.map_err(|e| ApiClientError::CustomModel {
            message: format!("Failed to parse response: {}", e),
            status_code: Some(status_code),
        })?;

        if let Some(error) = &custom_response.error {
            return Err(ApiClientError::CustomModel {
                message: error.clone(),
                status_code: Some(status_code),
            });
        }

        Ok(custom_response)
    }

    /// Send a streaming message to the custom model endpoint
    pub async fn send_message_stream(
        &self,
        conversation: ConversationState,
        conversation_id: Option<String>,
        system_prompt: Option<String>,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<ChatResponseStream, ApiClientError>> + Send>>, ApiClientError> {
        debug!(
            "Sending streaming message to custom model endpoint: {}",
            self.config.base_url
        );

        // Convert ConversationState to simplified format (same as non-streaming)
        let mut history = Vec::new();
        if let Some(chat_history) = &conversation.history {
            for (i, msg) in chat_history.iter().enumerate() {
                match msg {
                    crate::api_client::model::ChatMessage::UserInputMessage(user_msg) => {
                        debug!(
                            "Converting UserInputMessage {}: content length = {}, content = '{}'",
                            i,
                            user_msg.content.len(),
                            user_msg.content
                        );
                        // Only include messages with non-empty content
                        if !user_msg.content.trim().is_empty() {
                            history.push(SimpleMessage {
                                role: "user".to_string(),
                                content: user_msg.content.clone(),
                            });
                        } else {
                            debug!("Skipping empty UserInputMessage {}", i);
                        }
                    },
                    crate::api_client::model::ChatMessage::AssistantResponseMessage(assistant_msg) => {
                        debug!(
                            "Converting AssistantResponseMessage {}: content length = {}, content = '{}'",
                            i,
                            assistant_msg.content.len(),
                            assistant_msg.content
                        );
                        // Only include messages with non-empty content
                        if !assistant_msg.content.trim().is_empty() {
                            history.push(SimpleMessage {
                                role: "assistant".to_string(),
                                content: assistant_msg.content.clone(),
                            });
                        } else {
                            debug!("Skipping empty AssistantResponseMessage {}", i);
                        }
                    },
                }
            }
        }

        // Extract tool specifications (same as non-streaming)
        let tools = conversation
            .user_input_message
            .user_input_message_context
            .as_ref()
            .and_then(|ctx| ctx.tools.as_ref())
            .map(|tools| {
                tools
                    .iter()
                    .map(|tool| match tool {
                        crate::api_client::model::Tool::ToolSpecification(spec) => {
                            let input_schema = spec.input_schema.json.as_ref().map_or(
                                serde_json::Value::Object(serde_json::Map::new()),
                                |fig_doc| {
                                    serde_json::to_value(fig_doc)
                                        .unwrap_or(serde_json::Value::Object(serde_json::Map::new()))
                                },
                            );

                            ToolSpecification {
                                name: spec.name.clone(),
                                description: spec.description.clone(),
                                input_schema,
                            }
                        },
                    })
                    .collect::<Vec<_>>()
            });

        // Extract environment context (same as non-streaming)
        let env_context = conversation
            .user_input_message
            .user_input_message_context
            .as_ref()
            .and_then(|ctx| ctx.env_state.as_ref())
            .map(|env_state| EnvContext {
                current_working_directory: env_state.current_working_directory.clone(),
                operating_system: env_state.operating_system.clone(),
            });

        debug!(
            "Current message content length = {}, content = '{}'",
            conversation.user_input_message.content.len(),
            conversation.user_input_message.content
        );

        // Handle empty current message by using a default prompt
        let message_content = if conversation.user_input_message.content.trim().is_empty() {
            debug!("Current message is empty, using default continuation prompt");
            "Please continue with the next steps or confirm if the task is complete.".to_string()
        } else {
            conversation.user_input_message.content
        };

        // Extract tool results from context
        let tool_results = conversation
            .user_input_message
            .user_input_message_context
            .as_ref()
            .and_then(|ctx| ctx.tool_results.clone());

        let request_body = CustomModelRequest {
            model_id: self.config.model_id.clone(),
            message: message_content,
            conversation_id: conversation_id.or(conversation.conversation_id),
            history: if history.is_empty() { None } else { Some(history) },
            tools,
            env_context,
            system_prompt,
            parameters: None,
            tool_results,
        };

        // Build request to streaming endpoint
        let mut request_builder = self
            .http_client
            .post(format!("{}/chat/stream", self.config.base_url))
            .json(&request_body);

        // Add API key if configured
        if let Some(api_key) = &self.config.api_key {
            request_builder = request_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        // Add custom headers if configured
        if let Some(headers) = &self.config.headers {
            for (key, value) in headers {
                request_builder = request_builder.header(key, value);
            }
        }

        // Send request and get streaming response
        let response = request_builder.send().await.map_err(|e| ApiClientError::CustomModel {
            message: format!("Failed to send streaming request: {}", e),
            status_code: None,
        })?;

        let status_code = response.status().as_u16();
        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(ApiClientError::CustomModel {
                message: format!("HTTP {}: {}", status_code, error_text),
                status_code: Some(status_code),
            });
        }

        // Create stream from response bytes with proper SSE buffering
        let byte_stream = response.bytes_stream();

        // Use an SSE parser that properly handles line buffering
        let sse_parser = std::sync::Arc::new(std::sync::Mutex::new(SseParser::new()));
        let parser_for_map = sse_parser.clone();
        let parser_for_take = sse_parser.clone();

        // Process the stream in two phases:
        // 1. Parse chunks until we see [DONE] or stream ends
        // 2. Flush any remaining buffered data

        let chat_stream = byte_stream
            .map(move |chunk_result| {
                let mut parser = parser_for_map.lock().unwrap();
                match chunk_result {
                    Ok(chunk) => {
                        let chunk_str = String::from_utf8_lossy(&chunk);
                        debug!("Received streaming chunk: {}", chunk_str);

                        // Parse Server-Sent Events format with buffering
                        let (events, done) = parser.parse_chunk(&chunk_str);

                        // If we received [DONE], this should be the last chunk
                        if done {
                            debug!("Stream complete - received [DONE] event");
                            // Also flush any remaining data
                            let mut all_events = events;
                            all_events.extend(parser.flush());
                            all_events
                        } else {
                            events
                        }
                    },
                    Err(e) => {
                        // On error, flush any remaining data before reporting the error
                        let mut events = parser.flush();
                        events.push(Err(ApiClientError::CustomModel {
                            message: format!("Stream error: {}", e),
                            status_code: None,
                        }));
                        events
                    },
                }
            })
            .take_while(move |events| {
                // Continue taking events until we've processed all data after [DONE]
                let parser = parser_for_take.lock().unwrap();
                let continue_stream = !events.is_empty() || !parser.is_done();
                async move { continue_stream }
            })
            .flat_map(futures::stream::iter);

        Ok(Box::pin(chat_stream))
    }

    #[allow(dead_code)]
    pub fn get_config(&self) -> &CustomModelConfig {
        &self.config
    }
}

/// Stream response for custom models (for future streaming support)
#[allow(dead_code)]
pub struct CustomModelStream {
    // This can be implemented later for streaming responses
    _placeholder: (),
}
