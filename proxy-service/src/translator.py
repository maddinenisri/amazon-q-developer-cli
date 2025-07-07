"""
Translation layer with conversation state management.
Uses conversation manager to maintain perfect tool use tracking.
"""

import json
import logging
from typing import AsyncGenerator, Dict, List, Optional, Any

from .models import (
    QCliRequest,
    BedrockRequest,
    ToolUse,
)
from .conversation_manager import ConversationManager

logger = logging.getLogger(__name__)


class RequestTranslator:
    """Request translator with conversation state management."""

    def __init__(self, conversation_manager: ConversationManager):
        """Initialize with conversation manager."""
        self.conversation_manager = conversation_manager

    def qcli_to_bedrock(self, qcli_request: QCliRequest) -> BedrockRequest:
        """
        Convert Q CLI request to Bedrock format using managed conversation state.
        Uses reliable proxy state management for perfect tool use tracking.
        """
        conversation_id = qcli_request.conversation_id or "default"
        logger.info(f"Processing request for conversation: {conversation_id}")

        # Process tool results first (add them as events)
        if qcli_request.tool_results:
            logger.debug(f"Processing {len(qcli_request.tool_results)} tool results")

            valid_results, errors = self.conversation_manager.validate_tool_results(
                conversation_id, qcli_request.tool_results
            )

            if errors:
                logger.warning(f"Tool result validation errors: {errors}")

            # Add each valid tool result as an event
            for tool_result in valid_results:
                self.conversation_manager.add_tool_result(conversation_id, tool_result)
                logger.debug(f"Added tool result event: {tool_result.tool_use_id}")

        # Add current user message if present and not just tool results marker
        if qcli_request.message and qcli_request.message.strip():
            stripped_message = qcli_request.message.strip()
            if stripped_message != "[TOOL_RESULTS]":
                logger.debug(f"Adding user message: '{stripped_message[:100]}...'")
                self.conversation_manager.add_user_message(
                    conversation_id, stripped_message
                )
            else:
                logger.debug("Skipping [TOOL_RESULTS] marker message")

        # Get complete conversation history in Bedrock format (includes all events)
        messages = self.conversation_manager.get_bedrock_history(conversation_id)
        logger.debug(
            f"Retrieved {len(messages)} messages from conversation manager after adding events"
        )

        # Build system prompt
        system = None
        if qcli_request.system_prompt:
            system = [{"text": qcli_request.system_prompt}]

        # Build tool config
        tool_config = None
        if qcli_request.tools:
            bedrock_tools = []
            for tool in qcli_request.tools:
                bedrock_tool = {
                    "toolSpec": {
                        "name": tool.name,
                        "description": tool.description,
                        "inputSchema": {"json": tool.input_schema},
                    }
                }
                bedrock_tools.append(bedrock_tool)

            tool_config = {"tools": bedrock_tools}
            logger.debug(f"Added {len(bedrock_tools)} tool specifications")

        # Create Bedrock request
        bedrock_request = BedrockRequest(
            model_id=qcli_request.model_id,
            messages=messages,
            system=system,
            tool_config=tool_config,
            inference_config={"maxTokens": 4096, "temperature": 0.7, "topP": 0.9},
        )

        # Log final request structure
        logger.debug(f"Final Bedrock request: {len(messages)} messages")
        for i, msg in enumerate(messages):
            content_summary = []
            for content in msg.content:
                if isinstance(content, dict):
                    if "text" in content:
                        content_summary.append(f"text('{content['text'][:50]}...')")
                    elif "toolUse" in content:
                        content_summary.append(f"toolUse({content['toolUse']['name']})")
                    elif "toolResult" in content:
                        content_summary.append(
                            f"toolResult({content['toolResult']['toolUseId']})"
                        )

            logger.debug(f"Message {i} ({msg.role}): {', '.join(content_summary)}")

        # Consume tool results from pending list after successful request construction
        if qcli_request.tool_results:
            valid_results, _ = self.conversation_manager.validate_tool_results(
                conversation_id, qcli_request.tool_results
            )
            self.conversation_manager.consume_tool_results(
                conversation_id, valid_results
            )
            logger.debug(
                f"Consumed {len(valid_results)} tool results after building request"
            )

        return bedrock_request

    async def bedrock_to_qcli_stream(
        self, bedrock_stream: AsyncGenerator[Dict[str, Any], None], conversation_id: str
    ) -> AsyncGenerator[str, None]:
        """
        Convert Bedrock streaming events to Q CLI SSE format.
        Also captures assistant responses in conversation manager.
        """
        # Track assistant message construction
        assistant_content = ""
        current_tool_uses = []
        tool_context = {}  # content_block_index -> {tool_use_id, name}

        logger.debug(f"Starting stream processing for conversation: {conversation_id}")

        async for bedrock_event in bedrock_stream:
            logger.debug(
                f"Processing Bedrock event: {bedrock_event.get('type', bedrock_event)}"
            )

            qcli_events = self._map_bedrock_event_to_qcli(bedrock_event, tool_context)

            # Track assistant message content and tool uses
            for qcli_event in qcli_events:
                if qcli_event:
                    event_type = qcli_event.get("type")

                    # Accumulate assistant text
                    if event_type == "text_delta":
                        assistant_content += qcli_event["text"]

                    # Track tool uses being constructed
                    elif event_type == "tool_use_start":
                        current_tool_uses.append(
                            {
                                "tool_use_id": qcli_event["tool_use_id"],
                                "name": qcli_event["name"],
                                "input": "",
                            }
                        )

                    elif event_type == "tool_use_delta":
                        # Find the tool use being updated
                        for tool_use in current_tool_uses:
                            if tool_use["tool_use_id"] == qcli_event["tool_use_id"]:
                                tool_use["input"] += qcli_event["input"]
                                break

                    # Convert to SSE and yield
                    event_json = json.dumps(qcli_event)
                    yield f"data: {event_json}\n\n"

        # Store complete assistant response as separate events
        if assistant_content:
            self.conversation_manager.add_assistant_message(
                conversation_id, assistant_content
            )

        if current_tool_uses:
            # Convert accumulated tool uses to ToolUse objects and store as events
            for tool_data in current_tool_uses:
                try:
                    # Parse tool input as JSON
                    input_dict = (
                        json.loads(tool_data["input"]) if tool_data["input"] else {}
                    )
                    tool_use = ToolUse(
                        tool_use_id=tool_data["tool_use_id"],
                        name=tool_data["name"],
                        input=input_dict,
                    )
                    self.conversation_manager.add_tool_use(conversation_id, tool_use)
                except json.JSONDecodeError as e:
                    logger.warning(
                        f"Failed to parse tool input as JSON: {e}, using as string"
                    )
                    tool_use = ToolUse(
                        tool_use_id=tool_data["tool_use_id"],
                        name=tool_data["name"],
                        input={"raw": tool_data["input"]},
                    )
                    self.conversation_manager.add_tool_use(conversation_id, tool_use)

            logger.debug(
                f"Stored assistant message with {len(current_tool_uses)} tool uses as separate events"
            )

        # Send final [DONE] marker
        yield "data: [DONE]\n\n"
        logger.debug(f"Completed stream processing for conversation: {conversation_id}")

    def _map_bedrock_event_to_qcli(
        self, bedrock_event: Dict[str, Any], tool_context: Dict[int, Dict[str, str]]
    ) -> List[Optional[Dict[str, Any]]]:
        """Map single Bedrock event to Q CLI event(s). Same logic as before."""
        events = []

        if "messageStart" in bedrock_event:
            events.append(
                {
                    "type": "message_start",
                    "role": bedrock_event["messageStart"]["role"],
                }
            )

        elif "contentBlockStart" in bedrock_event:
            block_start = bedrock_event["contentBlockStart"]
            content_block_index = block_start.get("contentBlockIndex", 0)

            if "start" in block_start and "toolUse" in block_start["start"]:
                tool_use = block_start["start"]["toolUse"]
                tool_use_id = tool_use["toolUseId"]
                name = tool_use["name"]

                tool_context[content_block_index] = {
                    "tool_use_id": tool_use_id,
                    "name": name,
                }

                events.append(
                    {
                        "type": "tool_use_start",
                        "tool_use_id": tool_use_id,
                        "name": name,
                    }
                )

        elif "contentBlockDelta" in bedrock_event:
            delta_event = bedrock_event["contentBlockDelta"]
            delta = delta_event["delta"]
            content_block_index = delta_event.get("contentBlockIndex", 0)

            if "text" in delta:
                events.append(
                    {
                        "type": "text_delta",
                        "text": delta["text"],
                    }
                )
            elif "toolUse" in delta:
                tool_info = tool_context.get(content_block_index, {})
                events.append(
                    {
                        "type": "tool_use_delta",
                        "input": delta["toolUse"]["input"],
                        "tool_use_id": tool_info.get("tool_use_id", "unknown"),
                        "name": tool_info.get("name", "unknown"),
                    }
                )

        elif "contentBlockStop" in bedrock_event:
            content_block_index = bedrock_event["contentBlockStop"].get(
                "contentBlockIndex", 0
            )
            tool_info = tool_context.get(content_block_index, {})

            events.append(
                {
                    "type": "content_block_stop",
                    "tool_use_id": tool_info.get("tool_use_id"),
                    "name": tool_info.get("name"),
                }
            )

        elif "messageStop" in bedrock_event:
            stop_reason = bedrock_event["messageStop"].get("stopReason")
            events.append({"type": "message_stop", "stop_reason": stop_reason})

        elif "metadata" in bedrock_event:
            usage = bedrock_event["metadata"].get("usage", {})
            events.append(
                {
                    "type": "usage",
                    "input_tokens": usage.get("inputTokens"),
                    "output_tokens": usage.get("outputTokens"),
                    "total_tokens": usage.get("totalTokens"),
                }
            )

        elif bedrock_event.get("type") == "error":
            events.append(bedrock_event)

        else:
            logger.debug(f"Unknown Bedrock event type: {bedrock_event}")

        return events
