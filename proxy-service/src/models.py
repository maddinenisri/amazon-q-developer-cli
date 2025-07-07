"""
Pydantic models for Q CLI and Bedrock API data structures.
"""

from typing import Any, Dict, List, Optional, Union
from pydantic import BaseModel, Field


# Q CLI Request Models
class EnvContext(BaseModel):
    """Environment context from Q CLI."""

    current_working_directory: Optional[str] = Field(
        None, alias="currentWorkingDirectory"
    )
    operating_system: Optional[str] = Field(None, alias="operatingSystem")


class ToolSpecification(BaseModel):
    """Tool specification from Q CLI."""

    name: str
    description: str
    input_schema: Dict[str, Any] = Field(alias="inputSchema")


class ToolUse(BaseModel):
    """Tool use information from assistant messages."""

    tool_use_id: str
    name: str
    input: Dict[str, Any]


class HistoryMessage(BaseModel):
    """Complete message for conversation history with tool information."""

    role: str
    content: str
    tool_uses: Optional[List[ToolUse]] = None


# Keep SimpleMessage for backward compatibility
class SimpleMessage(BaseModel):
    """Simplified message for chat history (legacy)."""

    role: str
    content: str


class ToolResult(BaseModel):
    """Tool execution result from Q CLI."""

    tool_use_id: str  # Q CLI sends with underscore
    content: List[str]
    status: str


class QCliRequest(BaseModel):
    """Complete request from Q CLI to the proxy server."""

    model_id: str = Field(alias="modelId")
    message: str
    conversation_id: Optional[str] = Field(None, alias="conversationId")
    history: Optional[List[Union[HistoryMessage, SimpleMessage]]] = None
    tools: Optional[List[ToolSpecification]] = None
    tool_results: Optional[List[ToolResult]] = Field(None, alias="toolResults")
    env_context: Optional[EnvContext] = Field(None, alias="envContext")
    system_prompt: Optional[str] = Field(None, alias="systemPrompt")


# Bedrock API Models
class BedrockContentBlock(BaseModel):
    """Content block for Bedrock messages."""

    text: Optional[str] = None
    tool_use: Optional[Dict[str, Any]] = None
    tool_result: Optional[Dict[str, Any]] = None


class BedrockMessage(BaseModel):
    """Message format for Bedrock Converse API."""

    role: str
    content: List[Union[Dict[str, str], Dict[str, Any]]]


class BedrockToolSpec(BaseModel):
    """Tool specification for Bedrock."""

    name: str
    description: str
    input_schema: Dict[str, Any]


class BedrockRequest(BaseModel):
    """Request format for Bedrock Converse API."""

    model_id: str
    messages: List[BedrockMessage]
    system: Optional[List[Dict[str, str]]] = None
    tool_config: Optional[Dict[str, Any]] = None
    inference_config: Optional[Dict[str, Any]] = None


# Q CLI SSE Event Models
class QCliEvent(BaseModel):
    """Base Q CLI streaming event."""

    type: str


class TextDeltaEvent(QCliEvent):
    """Text content streaming event."""

    type: str = "text_delta"
    text: str


class ToolUseStartEvent(QCliEvent):
    """Tool use start event."""

    type: str = "tool_use_start"
    tool_use_id: str
    name: str


class ToolUseDeltaEvent(QCliEvent):
    """Tool use delta event."""

    type: str = "tool_use_delta"
    tool_use_id: str
    name: str
    input: str


class ContentBlockStopEvent(QCliEvent):
    """Content block stop event."""

    type: str = "content_block_stop"
    tool_use_id: Optional[str] = None
    name: Optional[str] = None


class MessageStartEvent(QCliEvent):
    """Message start event."""

    type: str = "message_start"
    role: str


class MessageStopEvent(QCliEvent):
    """Message stop event."""

    type: str = "message_stop"
    stop_reason: Optional[str] = None


# Conversation State Management Models
class ConversationEvent(BaseModel):
    """Individual event in conversation history (like log entries)."""

    event_type: str  # "user_message", "assistant_message", "tool_use", "tool_result"
    timestamp: float = Field(default_factory=lambda: __import__("time").time())

    # Content for different event types
    content: Optional[str] = None  # For user_message, assistant_message
    tool_use_id: Optional[str] = None  # For tool_use, tool_result
    tool_name: Optional[str] = None  # For tool_use
    tool_input: Optional[Dict[str, Any]] = None  # For tool_use
    tool_content: Optional[List[str]] = None  # For tool_result
    tool_status: Optional[str] = None  # For tool_result


class StoredMessage(BaseModel):
    """Complete message stored in proxy memory with tool information (legacy)."""

    role: str  # "user" or "assistant"
    content: str
    tool_uses: Optional[List[ToolUse]] = None
    timestamp: float = Field(default_factory=lambda: __import__("time").time())


class ConversationState(BaseModel):
    """Complete conversation state managed by proxy with event-based logging."""

    conversation_id: str
    events: List[ConversationEvent] = Field(
        default_factory=list
    )  # Sequential event log
    pending_tool_uses: Dict[str, ToolUse] = Field(
        default_factory=dict
    )  # tool_use_id -> ToolUse
    last_updated: float = Field(default_factory=lambda: __import__("time").time())

    def add_user_message(self, content: str) -> None:
        """Add a user message event to the conversation."""
        self.events.append(
            ConversationEvent(event_type="user_message", content=content)
        )
        self.last_updated = __import__("time").time()

    def add_assistant_message(self, content: str) -> None:
        """Add an assistant message event to the conversation."""
        if content and content.strip():
            self.events.append(
                ConversationEvent(event_type="assistant_message", content=content)
            )
        self.last_updated = __import__("time").time()

    def add_tool_use(self, tool_use: ToolUse) -> None:
        """Add a tool use event to the conversation."""
        self.events.append(
            ConversationEvent(
                event_type="tool_use",
                tool_use_id=tool_use.tool_use_id,
                tool_name=tool_use.name,
                tool_input=tool_use.input,
            )
        )
        # Store for validation
        self.pending_tool_uses[tool_use.tool_use_id] = tool_use
        self.last_updated = __import__("time").time()

    def add_tool_result(self, tool_result: ToolResult) -> None:
        """Add a tool result event to the conversation."""
        self.events.append(
            ConversationEvent(
                event_type="tool_result",
                tool_use_id=tool_result.tool_use_id,
                tool_content=tool_result.content,
                tool_status=tool_result.status,
            )
        )
        self.last_updated = __import__("time").time()

    def validate_tool_result(self, tool_result: ToolResult) -> bool:
        """Validate that a tool result corresponds to a pending tool use."""
        return tool_result.tool_use_id in self.pending_tool_uses

    def consume_tool_use(self, tool_use_id: str) -> Optional[ToolUse]:
        """Remove and return a pending tool use (when tool result is processed)."""
        return self.pending_tool_uses.pop(tool_use_id, None)

    def get_bedrock_history(self) -> List[BedrockMessage]:
        """Convert event log to Bedrock format for API calls."""
        bedrock_messages = []
        current_message = None
        current_content_blocks = []

        for event in self.events:
            if event.event_type == "user_message":
                # Finalize previous message if exists
                if current_message is not None and current_content_blocks:
                    bedrock_messages.append(
                        BedrockMessage(
                            role=current_message["role"], content=current_content_blocks
                        )
                    )

                # Start new user message
                current_message = {"role": "user"}
                current_content_blocks = [{"text": event.content}]

            elif event.event_type == "assistant_message":
                # Finalize previous message if exists
                if current_message is not None and current_content_blocks:
                    bedrock_messages.append(
                        BedrockMessage(
                            role=current_message["role"], content=current_content_blocks
                        )
                    )

                # Start new assistant message
                current_message = {"role": "assistant"}
                current_content_blocks = (
                    [{"text": event.content}] if event.content else []
                )

            elif event.event_type == "tool_use":
                # Add tool use to current assistant message
                if current_message and current_message["role"] == "assistant":
                    current_content_blocks.append(
                        {
                            "toolUse": {
                                "toolUseId": event.tool_use_id,
                                "name": event.tool_name,
                                "input": event.tool_input,
                            }
                        }
                    )

            elif event.event_type == "tool_result":
                # Finalize previous message if exists
                if current_message is not None and current_content_blocks:
                    bedrock_messages.append(
                        BedrockMessage(
                            role=current_message["role"], content=current_content_blocks
                        )
                    )

                # Start new user message with tool result
                tool_result_content = []
                if event.tool_content:
                    for content_item in event.tool_content:
                        if content_item.strip():
                            tool_result_content.append({"text": content_item.strip()})

                if not tool_result_content:
                    tool_result_content = [{"text": "Tool executed successfully"}]

                current_message = {"role": "user"}
                current_content_blocks = [
                    {
                        "toolResult": {
                            "toolUseId": event.tool_use_id,
                            "content": tool_result_content,
                            "status": event.tool_status.lower()
                            if event.tool_status
                            else "success",
                        }
                    }
                ]

        # Finalize last message
        if current_message is not None and current_content_blocks:
            bedrock_messages.append(
                BedrockMessage(
                    role=current_message["role"], content=current_content_blocks
                )
            )

        return bedrock_messages
