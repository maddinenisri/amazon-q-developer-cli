"""
Configuration management for the proxy service.
"""

import os
from typing import Optional
from pydantic import BaseModel


class ProxyConfig(BaseModel):
    """Configuration for the proxy service."""

    # AWS Configuration
    aws_region: str = os.getenv("AWS_REGION", "us-east-1")
    aws_profile: Optional[str] = os.getenv("AWS_PROFILE")

    # Server Configuration
    host: str = os.getenv("HOST", "0.0.0.0")
    port: int = int(os.getenv("PORT", "8080"))

    # Logging Configuration
    log_level: str = os.getenv("LOG_LEVEL", "DEBUG")

    # Request Configuration
    max_request_size: int = int(os.getenv("MAX_REQUEST_SIZE", "10000000"))  # 10MB
    request_timeout: int = int(os.getenv("REQUEST_TIMEOUT", "300"))  # 5 minutes

    # Model Configuration
    default_model_id: str = os.getenv(
        "DEFAULT_MODEL_ID", "anthropic.claude-3-5-sonnet-20241022-v2:0"
    )


# Global configuration instance
config = ProxyConfig()
