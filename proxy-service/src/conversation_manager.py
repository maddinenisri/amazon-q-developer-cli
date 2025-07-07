"""
Conversation state management for the proxy server.
Maintains conversation history in memory with proper tool use tracking.
"""

import logging
import time
from typing import Dict, List, Optional
from threading import Lock

from .models import ConversationState, ToolUse, ToolResult, BedrockMessage

logger = logging.getLogger(__name__)


class ConversationManager:
    """Manages conversation state in memory for the proxy server."""

    def __init__(self, cleanup_interval_hours: float = 24.0):
        """Initialize conversation manager.

        Args:
            cleanup_interval_hours: How often to clean up old conversations
        """
        self._conversations: Dict[str, ConversationState] = {}
        self._lock = Lock()
        self._cleanup_interval = cleanup_interval_hours * 3600  # Convert to seconds

        logger.info(
            f"Initialized ConversationManager with cleanup interval: {cleanup_interval_hours}h"
        )

    def get_or_create_conversation(self, conversation_id: str) -> ConversationState:
        """Get existing conversation or create new one."""
        with self._lock:
            if conversation_id not in self._conversations:
                logger.info(f"Creating new conversation: {conversation_id}")
                self._conversations[conversation_id] = ConversationState(
                    conversation_id=conversation_id
                )
            else:
                logger.debug(f"Retrieved existing conversation: {conversation_id}")

            return self._conversations[conversation_id]

    def add_user_message(self, conversation_id: str, content: str) -> None:
        """Add user message to conversation."""
        conversation = self.get_or_create_conversation(conversation_id)
        conversation.add_user_message(content)
        logger.debug(f"Added user message to {conversation_id}: '{content[:100]}...'")

    def add_assistant_message(self, conversation_id: str, content: str) -> None:
        """Add assistant message event."""
        conversation = self.get_or_create_conversation(conversation_id)
        conversation.add_assistant_message(content)
        logger.debug(
            f"Added assistant message to {conversation_id}: '{content[:100]}...'"
        )

    def add_tool_use(self, conversation_id: str, tool_use: ToolUse) -> None:
        """Add tool use event."""
        conversation = self.get_or_create_conversation(conversation_id)
        conversation.add_tool_use(tool_use)
        logger.debug(
            f"Added tool use to {conversation_id}: {tool_use.name} ({tool_use.tool_use_id})"
        )

    def add_tool_result(self, conversation_id: str, tool_result: ToolResult) -> None:
        """Add tool result event."""
        conversation = self.get_or_create_conversation(conversation_id)
        conversation.add_tool_result(tool_result)
        logger.debug(
            f"Added tool result to {conversation_id}: {tool_result.tool_use_id}"
        )

    def add_assistant_message_legacy(
        self,
        conversation_id: str,
        content: str,
        tool_uses: Optional[List[ToolUse]] = None,
    ) -> None:
        """Add assistant message with optional tool uses (legacy method)."""
        # Add assistant message
        if content and content.strip():
            self.add_assistant_message(conversation_id, content)

        # Add individual tool uses as separate events
        if tool_uses:
            for tool_use in tool_uses:
                self.add_tool_use(conversation_id, tool_use)

    def validate_tool_results(
        self, conversation_id: str, tool_results: List[ToolResult]
    ) -> tuple[List[ToolResult], List[str]]:
        """Validate tool results against pending tool uses.

        Returns:
            tuple of (valid_tool_results, error_messages)
        """
        conversation = self.get_or_create_conversation(conversation_id)
        valid_results = []
        errors = []

        for tool_result in tool_results:
            if conversation.validate_tool_result(tool_result):
                valid_results.append(tool_result)
                logger.debug(f"Valid tool result: {tool_result.tool_use_id}")
            else:
                error_msg = f"Tool result {tool_result.tool_use_id} has no corresponding tool use"
                errors.append(error_msg)
                logger.warning(f"Invalid tool result: {error_msg}")

        # Note: We don't consume tool uses here - they'll be consumed after successful Bedrock request
        # This ensures tool uses remain available for building the Bedrock request history

        return valid_results, errors

    def consume_tool_results(
        self, conversation_id: str, tool_results: List[ToolResult]
    ) -> None:
        """Consume tool uses after successful Bedrock request."""
        conversation = self.get_or_create_conversation(conversation_id)

        for tool_result in tool_results:
            consumed = conversation.consume_tool_use(tool_result.tool_use_id)
            if consumed:
                logger.debug(
                    f"Consumed tool use after successful request: {consumed.name} ({consumed.tool_use_id})"
                )
            else:
                logger.warning(f"Could not consume tool use: {tool_result.tool_use_id}")

    def get_bedrock_history(self, conversation_id: str) -> List[BedrockMessage]:
        """Get conversation history in Bedrock format."""
        conversation = self.get_or_create_conversation(conversation_id)
        history = conversation.get_bedrock_history()

        logger.debug(
            f"Retrieved {len(history)} messages for conversation {conversation_id}"
        )
        return history

    def get_conversation_info(self, conversation_id: str) -> Dict:
        """Get conversation information for debugging."""
        with self._lock:
            if conversation_id not in self._conversations:
                return {"exists": False}

            conv = self._conversations[conversation_id]
            return {
                "exists": True,
                "event_count": len(conv.events),
                "pending_tool_uses": len(conv.pending_tool_uses),
                "last_updated": conv.last_updated,
                "age_seconds": time.time() - conv.last_updated,
            }

    def cleanup_old_conversations(self) -> int:
        """Clean up conversations older than cleanup_interval.

        Returns:
            Number of conversations cleaned up
        """
        current_time = time.time()
        cutoff_time = current_time - self._cleanup_interval
        cleaned_count = 0

        with self._lock:
            conversations_to_remove = [
                conv_id
                for conv_id, conv in self._conversations.items()
                if conv.last_updated < cutoff_time
            ]

            for conv_id in conversations_to_remove:
                del self._conversations[conv_id]
                cleaned_count += 1
                logger.info(f"Cleaned up old conversation: {conv_id}")

        if cleaned_count > 0:
            logger.info(f"Cleaned up {cleaned_count} old conversations")

        return cleaned_count

    def get_stats(self) -> Dict:
        """Get memory usage statistics."""
        with self._lock:
            total_events = sum(
                len(conv.events) for conv in self._conversations.values()
            )
            total_pending_tools = sum(
                len(conv.pending_tool_uses) for conv in self._conversations.values()
            )

            return {
                "active_conversations": len(self._conversations),
                "total_events": total_events,
                "total_pending_tool_uses": total_pending_tools,
                "cleanup_interval_hours": self._cleanup_interval / 3600,
            }
