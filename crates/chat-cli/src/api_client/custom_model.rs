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

        let request_body = CustomModelRequest {
            model_id: self.config.model_id.clone(),
            message: conversation.user_input_message.content,
            conversation_id: conversation_id.or(conversation.conversation_id),
            history: if history.is_empty() { None } else { Some(history) },
            tools,
            env_context,
            system_prompt,
            parameters: None, // Can be extended for model-specific parameters
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

        let request_body = CustomModelRequest {
            model_id: self.config.model_id.clone(),
            message: conversation.user_input_message.content,
            conversation_id: conversation_id.or(conversation.conversation_id),
            history: if history.is_empty() { None } else { Some(history) },
            tools,
            env_context,
            system_prompt,
            parameters: None,
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

        // Create stream from response bytes
        let byte_stream = response.bytes_stream();

        // Convert Server-Sent Events to ChatResponseStream
        let chat_stream = byte_stream
            .map(|chunk_result| {
                match chunk_result {
                    Ok(chunk) => {
                        let chunk_str = String::from_utf8_lossy(&chunk);
                        debug!("Received streaming chunk: {}", chunk_str);

                        // Parse Server-Sent Events format
                        Self::parse_sse_chunk(&chunk_str)
                    },
                    Err(e) => vec![Err(ApiClientError::CustomModel {
                        message: format!("Stream error: {}", e),
                        status_code: None,
                    })],
                }
            })
            .flat_map(futures::stream::iter);

        Ok(Box::pin(chat_stream))
    }

    /// Parse Server-Sent Events chunk into ChatResponseStream events
    fn parse_sse_chunk(chunk: &str) -> Vec<Result<ChatResponseStream, ApiClientError>> {
        let mut events = Vec::new();

        for line in chunk.lines() {
            if let Some(data) = line.strip_prefix("data: ") {
                if data == "[DONE]" {
                    break;
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
                    input: None, // Will be filled by subsequent tool_use_delta events
                    stop: None,  // This is the start event, so no stop flag
                })
            },
            "tool_use_delta" => {
                // For tool use delta, we need to get the tool_use_id and name from context
                // The proxy server should include these in the delta event
                let input = event.get("input")?.as_str()?.to_string();
                let tool_use_id = event
                    .get("tool_use_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let name = event
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                Some(ChatResponseStream::ToolUseEvent {
                    tool_use_id,
                    name,
                    input: Some(input),
                    stop: Some(false), // Still streaming
                })
            },
            "content_block_stop" => {
                // End of a content block - this should NOT generate a ToolUseEvent
                // The built-in parser handles tool completion internally
                // Only generate events for text content blocks if needed
                None
            },
            "message_stop" => {
                // End of stream - no specific event needed as stream will end
                None
            },
            "error" => {
                // Error events are handled at the stream level
                None
            },
            _ => {
                debug!("Unknown event type: {}", event_type);
                None
            },
        }
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
