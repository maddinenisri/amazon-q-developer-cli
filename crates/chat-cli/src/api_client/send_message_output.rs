use std::pin::Pin;

use aws_types::request_id::RequestId;
use futures::stream::Stream;

use crate::api_client::ApiClientError;
use crate::api_client::model::ChatResponseStream;

pub enum SendMessageOutput {
    Codewhisperer(
        amzn_codewhisperer_streaming_client::operation::generate_assistant_response::GenerateAssistantResponseOutput,
    ),
    QDeveloper(amzn_qdeveloper_streaming_client::operation::send_message::SendMessageOutput),
    Mock(Vec<ChatResponseStream>),
    CustomModel(Pin<Box<dyn Stream<Item = Result<ChatResponseStream, ApiClientError>> + Send>>),
}

impl std::fmt::Debug for SendMessageOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SendMessageOutput::Codewhisperer(output) => f.debug_tuple("Codewhisperer").field(output).finish(),
            SendMessageOutput::QDeveloper(output) => f.debug_tuple("QDeveloper").field(output).finish(),
            SendMessageOutput::Mock(vec) => f.debug_tuple("Mock").field(vec).finish(),
            SendMessageOutput::CustomModel(_) => f.debug_tuple("CustomModel").field(&"<streaming>").finish(),
        }
    }
}

impl SendMessageOutput {
    pub fn request_id(&self) -> Option<&str> {
        match self {
            SendMessageOutput::Codewhisperer(output) => output.request_id(),
            SendMessageOutput::QDeveloper(output) => output.request_id(),
            SendMessageOutput::Mock(_) => None,
            SendMessageOutput::CustomModel(_) => None,
        }
    }

    pub async fn recv(&mut self) -> Result<Option<ChatResponseStream>, ApiClientError> {
        use futures::StreamExt;

        match self {
            SendMessageOutput::Codewhisperer(output) => {
                let event = output.generate_assistant_response_response.recv().await?;
                if let Some(ref e) = event {
                    tracing::debug!("Codewhisperer Event: {:#?}", e);
                }
                Ok(event.map(|s| s.into()))
            },
            SendMessageOutput::QDeveloper(output) => {
                let event = output.send_message_response.recv().await?;
                if let Some(ref e) = event {
                    tracing::debug!("Q Developer Event: {:#?}", e);
                }
                Ok(event.map(|s| s.into()))
            },
            SendMessageOutput::Mock(vec) => Ok(vec.pop()),
            SendMessageOutput::CustomModel(stream) => match stream.next().await {
                Some(Ok(event)) => Ok(Some(event)),
                Some(Err(e)) => Err(e),
                None => Ok(None),
            },
        }
    }
}

impl RequestId for SendMessageOutput {
    fn request_id(&self) -> Option<&str> {
        match self {
            SendMessageOutput::Codewhisperer(output) => output.request_id(),
            SendMessageOutput::QDeveloper(output) => output.request_id(),
            SendMessageOutput::Mock(_) => Some("<mock-request-id>"),
            SendMessageOutput::CustomModel(_) => Some("<custom-model-request-id>"),
        }
    }
}
