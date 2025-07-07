# Q CLI Bedrock Proxy Service

A pure passthrough proxy service that translates between Q CLI and AWS Bedrock formats with no business logic or state management.

## Overview

This proxy service acts as a transparent translation layer:
- Receives Q CLI requests in their native format
- Translates to AWS Bedrock Converse API format
- Streams Bedrock responses back in Q CLI expected format
- Preserves all conversation context and tool handling

## Architecture

```
Q CLI ←→ CustomModelClient ←→ Proxy Service ←→ AWS Bedrock
      (HTTP/SSE)           (Format Translation)
```

## Features

- **Pure Translation**: No conversation management or business logic
- **Streaming Support**: Server-Sent Events for real-time responses
- **Tool Support**: Full tool use/result handling with proper format conversion
- **Error Handling**: Transparent error propagation from Bedrock
- **AWS Integration**: Supports AWS profiles and credential management

## Setup

### Prerequisites

- Python 3.8+
- AWS credentials configured (via profile or environment)
- Access to AWS Bedrock

### Installation

1. Install dependencies with Poetry:
```bash
cd proxy-service
poetry install
```

2. Configure AWS credentials (choose one):
```bash
# Option 1: AWS Profile
export AWS_PROFILE=your-profile

# Option 2: Environment variables
export AWS_ACCESS_KEY_ID=your-key
export AWS_SECRET_ACCESS_KEY=your-secret
export AWS_REGION=us-east-1
```

3. Run the service:
```bash
# Using Poetry script
poetry run proxy-server

# Or with Poetry shell
poetry shell
python -m src.main

# Or with uvicorn directly
poetry run uvicorn src.main:app --host 0.0.0.0 --port 8080
```

## Configuration

Environment variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `HOST` | `0.0.0.0` | Server host |
| `PORT` | `8080` | Server port |
| `AWS_REGION` | `us-east-1` | AWS region |
| `AWS_PROFILE` | `None` | AWS profile name |
| `LOG_LEVEL` | `INFO` | Logging level |
| `REQUEST_TIMEOUT` | `300` | Request timeout (seconds) |
| `DEFAULT_MODEL_ID` | `anthropic.claude-3-5-sonnet-20241022-v2:0` | Default Bedrock model |

## API Endpoints

### POST /chat/stream
Main endpoint for Q CLI streaming requests.
- **Input**: Q CLI conversation format
- **Output**: Server-Sent Events stream
- **Headers**: `Content-Type: text/plain`

### POST /chat
Non-streaming endpoint (for compatibility).
- **Input**: Q CLI conversation format
- **Output**: JSON response

### GET /health
Health check endpoint.
- **Output**: Service status information

## Q CLI Integration

Configure Q CLI to use this proxy by updating your Q CLI config:

```json
{
  "custom_models": {
    "test-model": {
      "base_url": "http://localhost:8080",
      "model_id": "anthropic.claude-3-5-sonnet-20241022-v2:0"
    }
  }
}
```

Then use with:
```bash
q chat --model custom:test-model
```

## Development

### Running in Development
```bash
# With auto-reload
poetry run uvicorn src.main:app --reload --host 0.0.0.0 --port 8080

# With debug logging
LOG_LEVEL=DEBUG poetry run proxy-server

# Code formatting
poetry run black src/
poetry run isort src/
```

### Testing
```bash
# Test health endpoint
curl http://localhost:8080/health

# Test with Q CLI
q chat --model custom:test-model "Hello, world!"
```

## Deployment

### Docker
```dockerfile
FROM python:3.11-slim

WORKDIR /app
COPY requirements.txt .
RUN pip install -r requirements.txt

COPY src/ ./src/
EXPOSE 8080

CMD ["python", "-m", "src.main"]
```

### Production Considerations
- Use proper AWS IAM roles/policies
- Configure CORS appropriately
- Set up proper logging and monitoring
- Consider rate limiting and request size limits
- Use HTTPS in production

## Troubleshooting

### Common Issues

1. **AWS Credentials**: Ensure AWS credentials are properly configured
2. **Model Access**: Verify Bedrock model access in your AWS account
3. **Network**: Check network connectivity to AWS Bedrock
4. **Logs**: Check application logs for detailed error information

### Debug Mode
Run with `LOG_LEVEL=DEBUG` to see detailed request/response information.