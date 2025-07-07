"""
AWS Bedrock client wrapper for the proxy service.
"""

import asyncio
import json
import logging
from typing import AsyncGenerator, Dict, Any

import boto3
from botocore.exceptions import ClientError

from .config import config
from .models import BedrockRequest

logger = logging.getLogger(__name__)


class BedrockClient:
    """Async wrapper for AWS Bedrock client with streaming support."""

    def __init__(self):
        """Initialize Bedrock client."""
        # Initialize boto3 session
        session = (
            boto3.Session(profile_name=config.aws_profile)
            if config.aws_profile
            else boto3.Session()
        )
        self.bedrock_client = session.client(
            "bedrock-runtime", region_name=config.aws_region
        )

        logger.info(f"Initialized Bedrock client for region: {config.aws_region}")
        if config.aws_profile:
            logger.info(f"Using AWS profile: {config.aws_profile}")

    async def converse_stream(
        self, request: BedrockRequest
    ) -> AsyncGenerator[Dict[str, Any], None]:
        """
        Stream chat responses using Bedrock Converse Stream API.

        This is a pure passthrough to Bedrock - no custom logic.
        """
        try:
            # Build Bedrock request
            bedrock_request = self._build_bedrock_request(request)

            logger.debug(
                f"Sending to Bedrock Stream: {json.dumps(bedrock_request, indent=2)}"
            )

            # Call Bedrock Converse Stream in thread pool
            loop = asyncio.get_event_loop()
            response = await loop.run_in_executor(
                None, lambda: self.bedrock_client.converse_stream(**bedrock_request)
            )

            # Stream events from Bedrock
            stream = response.get("stream", [])
            async for event_data in self._process_stream_events(stream):
                yield event_data

        except ClientError as e:
            error_code = e.response["Error"]["Code"]
            error_message = e.response["Error"]["Message"]
            logger.error(f"Bedrock ClientError: {error_code} - {error_message}")

            # Yield error event in Bedrock format
            yield {
                "type": "error",
                "error": f"AWS Error ({error_code}): {error_message}",
            }

        except Exception as e:
            logger.error(f"Unexpected error: {e}")
            yield {"type": "error", "error": f"Internal server error: {str(e)}"}

    async def _process_stream_events(
        self, stream
    ) -> AsyncGenerator[Dict[str, Any], None]:
        """Process Bedrock stream events."""

        # Process the stream in a thread pool since it's not async
        loop = asyncio.get_event_loop()

        def process_stream():
            """Process the stream synchronously."""
            events = []

            try:
                for event in stream:
                    logger.debug(f"Processing Bedrock stream event: {event}")

                    # Pass through Bedrock events as-is
                    # The translator will handle the format conversion
                    events.append(event)

            except Exception as e:
                logger.error(f"Error processing stream: {e}")
                events.append({"type": "error", "error": str(e)})

            return events

        # Process stream in thread pool
        events = await loop.run_in_executor(None, process_stream)

        # Yield events one by one
        for event in events:
            yield event

    def _build_bedrock_request(self, request: BedrockRequest) -> Dict[str, Any]:
        """Build Bedrock converse request."""

        bedrock_request = {
            "modelId": request.model_id,
            "messages": [msg.dict() for msg in request.messages],
            "inferenceConfig": request.inference_config
            or {"maxTokens": 4096, "temperature": 0.7, "topP": 0.9},
        }

        # Add system prompt if provided
        if request.system:
            bedrock_request["system"] = request.system

        # Add tools if provided
        if request.tool_config:
            bedrock_request["toolConfig"] = request.tool_config

        return bedrock_request
