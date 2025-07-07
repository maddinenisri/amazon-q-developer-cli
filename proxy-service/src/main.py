"""
FastAPI proxy server for Q CLI to AWS Bedrock translation.
Pure passthrough service with format conversion only.
"""

import logging
import sys
from contextlib import asynccontextmanager

from fastapi import FastAPI, HTTPException
from fastapi.responses import StreamingResponse
from fastapi.middleware.cors import CORSMiddleware

from .config import config
from .models import QCliRequest
from .bedrock_client import BedrockClient
from .translator import RequestTranslator
from .conversation_manager import ConversationManager

# Configure logging
logging.basicConfig(
    level=getattr(logging, config.log_level.upper()),
    format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
    stream=sys.stdout,
)
logger = logging.getLogger(__name__)

# Global instances
bedrock_client = None
conversation_manager = None
translator = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    """Application lifespan handler."""
    global bedrock_client, conversation_manager, translator

    # Startup
    logger.info("Starting proxy service with conversation state management...")
    bedrock_client = BedrockClient()
    conversation_manager = ConversationManager()
    translator = RequestTranslator(conversation_manager)
    logger.info(f"Proxy service started on {config.host}:{config.port}")

    yield

    # Shutdown
    logger.info("Shutting down proxy service...")


# Create FastAPI app
app = FastAPI(
    title="Q CLI Bedrock Proxy",
    description="Pure passthrough proxy service for Q CLI to AWS Bedrock translation",
    version="1.0.0",
    lifespan=lifespan,
)

# Add CORS middleware
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # Configure appropriately for production
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {
        "status": "healthy",
        "service": "q-cli-bedrock-proxy",
        "region": config.aws_region,
        "version": "1.0.0",
    }


@app.post("/chat/stream")
async def chat_stream(request: QCliRequest):
    """
    Main endpoint for Q CLI streaming chat requests.
    Pure passthrough with format translation.
    """
    try:
        logger.info(f"Received streaming chat request for model: {request.model_id}")
        logger.debug(
            f"Request details: conversation_id={request.conversation_id}, "
            f"message_length={len(request.message)}, "
            f"history_length={len(request.history) if request.history else 0}, "
            f"tools_count={len(request.tools) if request.tools else 0}"
        )

        # Translate Q CLI request to Bedrock format
        bedrock_request = translator.qcli_to_bedrock(request)

        # Get Bedrock streaming response
        bedrock_stream = bedrock_client.converse_stream(bedrock_request)

        # Translate Bedrock stream to Q CLI SSE format
        conversation_id = request.conversation_id or "default"
        qcli_stream = translator.bedrock_to_qcli_stream(bedrock_stream, conversation_id)

        # Return as Server-Sent Events
        return StreamingResponse(
            qcli_stream,
            media_type="text/plain",
            headers={
                "Cache-Control": "no-cache",
                "Connection": "keep-alive",
                "Content-Type": "text/plain; charset=utf-8",
            },
        )

    except Exception as e:
        logger.error(f"Error processing chat stream request: {e}")
        raise HTTPException(status_code=500, detail=f"Internal server error: {str(e)}")


@app.post("/chat")
async def chat_non_streaming(request: QCliRequest):
    """
    Non-streaming endpoint (for compatibility).
    Note: Q CLI primarily uses streaming, but this is here for completeness.
    """
    try:
        logger.info(
            f"Received non-streaming chat request for model: {request.model_id}"
        )

        # For non-streaming, we'll collect all streaming events and return the final result
        bedrock_request = translator.qcli_to_bedrock(request)
        bedrock_stream = bedrock_client.converse_stream(bedrock_request)
        conversation_id = request.conversation_id or "default"
        qcli_stream = translator.bedrock_to_qcli_stream(bedrock_stream, conversation_id)

        # Collect all text content
        content_parts = []
        tool_calls = []

        async for sse_line in qcli_stream:
            if sse_line.startswith("data: "):
                event_data = sse_line[6:].strip()
                if event_data == "[DONE]":
                    break

                try:
                    import json

                    event = json.loads(event_data)

                    if event.get("type") == "text_delta":
                        content_parts.append(event["text"])
                    elif event.get("type") == "tool_use_start":
                        # Handle tool calls if needed
                        pass

                except json.JSONDecodeError:
                    continue

        return {
            "content": "".join(content_parts),
            "tool_calls": tool_calls,
            "usage": None,
            "error": None,
        }

    except Exception as e:
        logger.error(f"Error processing non-streaming chat request: {e}")
        raise HTTPException(status_code=500, detail=f"Internal server error: {str(e)}")


def main():
    """Main entry point for the proxy server."""
    import uvicorn

    uvicorn.run(
        "src.main:app",
        host=config.host,
        port=config.port,
        log_level=config.log_level.lower(),
        reload=False,
    )


if __name__ == "__main__":
    main()
