use prost::Message;
use std::fmt;
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{mpsc, oneshot};

pub const DEFAULT_MAX_FRAME_SIZE: usize = 64 * 1024 * 1024;

pub struct Transport;

impl Transport {
    pub fn provide<P: TransportProvider>(config: P::Config) -> P {
        P::new(config)
    }
}

pub trait TransportProvider: Sized {
    type Config;

    fn new(config: Self::Config) -> Self;
}

#[allow(async_fn_in_trait)]
pub trait Requester<Request, Response> {
    async fn request(&self, request: Request) -> Result<Response, TransportError>;
}

#[allow(async_fn_in_trait)]
pub trait Responder<Request, Response> {
    async fn receive(&mut self) -> Result<Request, TransportError>;

    async fn respond(&mut self, response: Response) -> Result<(), TransportError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InProcTransportConfig {
    pub channel_capacity: usize,
}

pub struct InProcTransportProvider {
    config: InProcTransportConfig,
}

pub struct InProcRequester<Request, Response> {
    sender: mpsc::Sender<(Request, oneshot::Sender<Response>)>,
}

pub struct InProcResponder<Request, Response> {
    receiver: mpsc::Receiver<(Request, oneshot::Sender<Response>)>,
    pending_reply: Option<oneshot::Sender<Response>>,
}

impl TransportProvider for InProcTransportProvider {
    type Config = InProcTransportConfig;

    fn new(config: Self::Config) -> Self {
        Self { config }
    }
}

impl InProcTransportProvider {
    pub fn pair<Request, Response>(
        &self,
    ) -> (
        InProcRequester<Request, Response>,
        InProcResponder<Request, Response>,
    ) {
        let (sender, receiver) = mpsc::channel(self.config.channel_capacity);
        (
            InProcRequester { sender },
            InProcResponder {
                receiver,
                pending_reply: None,
            },
        )
    }
}

impl<Request, Response> Clone for InProcRequester<Request, Response> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<Request, Response> Requester<Request, Response> for InProcRequester<Request, Response> {
    async fn request(&self, request: Request) -> Result<Response, TransportError> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send((request, reply))
            .await
            .map_err(|_| TransportError::Stopped)?;
        response.await.map_err(|_| TransportError::ResponseDropped)
    }
}

impl<Request, Response> Responder<Request, Response> for InProcResponder<Request, Response> {
    async fn receive(&mut self) -> Result<Request, TransportError> {
        if self.pending_reply.is_some() {
            return Err(TransportError::ResponsePending);
        }
        let (request, reply) = self.receiver.recv().await.ok_or(TransportError::Stopped)?;
        self.pending_reply = Some(reply);
        Ok(request)
    }

    async fn respond(&mut self, response: Response) -> Result<(), TransportError> {
        self.pending_reply
            .take()
            .ok_or(TransportError::NoPendingRequest)?
            .send(response)
            .map_err(|_| TransportError::ResponseDropped)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransportConfig {
    pub max_frame_size: usize,
    pub read_timeout: Option<Duration>,
    pub write_timeout: Option<Duration>,
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            max_frame_size: DEFAULT_MAX_FRAME_SIZE,
            read_timeout: None,
            write_timeout: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdsTransportConfig {
    pub framing: TransportConfig,
    pub connect_timeout: Duration,
    pub request_timeout: Option<Duration>,
    pub request_queue_capacity: usize,
}

impl Default for UdsTransportConfig {
    fn default() -> Self {
        Self {
            framing: TransportConfig::default(),
            connect_timeout: Duration::from_secs(10),
            request_timeout: Some(Duration::from_secs(10)),
            request_queue_capacity: 64,
        }
    }
}

#[derive(Debug)]
pub enum TransportError {
    FrameTooLarge { len: usize, max: usize },
    InvalidFrameLength,
    ConnectTimeout { path: PathBuf },
    Io(std::io::Error),
    Encode(prost::EncodeError),
    Decode(prost::DecodeError),
    Timeout,
    Stopped,
    ResponseDropped,
    ResponsePending,
    NoPendingRequest,
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FrameTooLarge { len, max } => {
                write!(formatter, "frame length {len} exceeds max {max}")
            }
            Self::InvalidFrameLength => formatter.write_str("frame length is invalid"),
            Self::ConnectTimeout { path } => {
                write!(formatter, "timed out connecting to UDS {}", path.display())
            }
            Self::Io(error) => write!(formatter, "transport I/O error: {error}"),
            Self::Encode(error) => write!(formatter, "failed to encode transport frame: {error}"),
            Self::Decode(error) => write!(formatter, "failed to decode transport frame: {error}"),
            Self::Timeout => formatter.write_str("transport operation timed out"),
            Self::Stopped => formatter.write_str("transport is stopped"),
            Self::ResponseDropped => formatter.write_str("transport response was dropped"),
            Self::ResponsePending => formatter.write_str("previous request has not been answered"),
            Self::NoPendingRequest => formatter.write_str("there is no pending request"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Encode(error) => Some(error),
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TransportError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

struct FramedTransport<S> {
    io: S,
    config: TransportConfig,
}

impl<S> FramedTransport<S> {
    fn new(io: S, config: TransportConfig) -> Self {
        Self { io, config }
    }
}

impl<S> FramedTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    async fn send<M: Message>(&mut self, message: &M) -> Result<(), TransportError> {
        let timeout = self.config.write_timeout;
        let operation = self.send_inner(message);
        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, operation)
                .await
                .map_err(|_| TransportError::Timeout)?,
            None => operation.await,
        }
    }

    async fn receive<M: Message + Default>(&mut self) -> Result<M, TransportError> {
        let timeout = self.config.read_timeout;
        let operation = self.receive_inner::<M>();
        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, operation)
                .await
                .map_err(|_| TransportError::Timeout)?,
            None => operation.await,
        }
    }

    async fn send_inner<M: Message>(&mut self, message: &M) -> Result<(), TransportError> {
        let len = message.encoded_len();
        validate_length(len, self.config.max_frame_size)?;
        let len = u32::try_from(len).map_err(|_| TransportError::InvalidFrameLength)?;
        self.io.write_all(&len.to_le_bytes()).await?;
        let mut payload = Vec::with_capacity(len as usize);
        message.encode(&mut payload).map_err(TransportError::Encode)?;
        self.io.write_all(&payload).await?;
        self.io.flush().await?;
        Ok(())
    }

    async fn receive_inner<M: Message + Default>(&mut self) -> Result<M, TransportError> {
        let mut header = [0u8; 4];
        self.io.read_exact(&mut header).await?;
        let len = u32::from_le_bytes(header) as usize;
        validate_length(len, self.config.max_frame_size)?;
        let mut payload = vec![0u8; len];
        self.io.read_exact(&mut payload).await?;
        M::decode(payload.as_slice()).map_err(TransportError::Decode)
    }
}

#[derive(Debug)]
pub struct UdsTransportProvider {
    config: UdsTransportConfig,
}

impl TransportProvider for UdsTransportProvider {
    type Config = UdsTransportConfig;

    fn new(config: Self::Config) -> Self {
        Self { config }
    }
}

impl UdsTransportProvider {
    pub async fn connect<Request, Response>(
        &self,
        path: impl Into<PathBuf>,
    ) -> Result<UdsRequester<Request, Response>, TransportError>
    where
        Request: Message + Send + 'static,
        Response: Message + Default + Send + 'static,
    {
        let path = path.into();
        let display_path = path.clone();
        let config = self.config.clone();
        let stream = tokio::time::timeout(config.connect_timeout, async move {
            loop {
                match UnixStream::connect(&path).await {
                    Ok(stream) => return stream,
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .map_err(|_| TransportError::ConnectTimeout { path: display_path })?;
        Ok(UdsRequester::from_stream(stream, &self.config))
    }

    pub fn bind<Request, Response>(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<UdsResponder<Request, Response>, TransportError> {
        let listener = UnixListener::bind(path)?;
        Ok(UdsResponder {
            listener: Some(listener),
            connection: None,
            framing: self.config.framing.clone(),
            pending_response: false,
            marker: PhantomData,
        })
    }

}

#[cfg(any(test, feature = "test-support"))]
pub mod test_support {
    use super::*;

    pub fn uds_pair<Request, Response>(
        provider: &UdsTransportProvider,
    ) -> Result<
        (
            UdsRequester<Request, Response>,
            UdsResponder<Request, Response>,
        ),
        TransportError,
    >
    where
        Request: Message + Send + 'static,
        Response: Message + Default + Send + 'static,
    {
        let (requester_stream, responder_stream) = UnixStream::pair()?;
        Ok((
            UdsRequester::from_stream(requester_stream, &provider.config),
            UdsResponder {
                listener: None,
                connection: Some(FramedTransport::new(
                    responder_stream,
                    provider.config.framing.clone(),
                )),
                framing: provider.config.framing.clone(),
                pending_response: false,
                marker: PhantomData,
            },
        ))
    }
}

struct UdsRequest<Request, Response> {
    request: Request,
    reply: oneshot::Sender<Result<Response, TransportError>>,
}

#[derive(Debug)]
pub struct UdsRequester<Request, Response> {
    sender: mpsc::Sender<UdsRequest<Request, Response>>,
}

impl<Request, Response> Clone for UdsRequester<Request, Response> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<Request, Response> UdsRequester<Request, Response>
where
    Request: Message + Send + 'static,
    Response: Message + Default + Send + 'static,
{
    fn from_stream(stream: UnixStream, config: &UdsTransportConfig) -> Self {
        let (sender, receiver) = mpsc::channel(config.request_queue_capacity);
        let framing = config.framing.clone();
        tokio::spawn(run_uds_requester(
            stream,
            framing,
            config.request_timeout,
            receiver,
        ));
        Self { sender }
    }
}

impl<Request, Response> Requester<Request, Response> for UdsRequester<Request, Response>
where
    Request: Message + Send + 'static,
    Response: Message + Default + Send + 'static,
{
    async fn request(&self, request: Request) -> Result<Response, TransportError> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send(UdsRequest { request, reply })
            .await
            .map_err(|_| TransportError::Stopped)?;
        response.await.map_err(|_| TransportError::ResponseDropped)?
    }
}

async fn run_uds_requester<Request, Response>(
    stream: UnixStream,
    config: TransportConfig,
    request_timeout: Option<Duration>,
    mut requests: mpsc::Receiver<UdsRequest<Request, Response>>,
) where
    Request: Message + Send + 'static,
    Response: Message + Default + Send + 'static,
{
    let mut connection = FramedTransport::new(stream, config);
    while let Some(command) = requests.recv().await {
        let operation = async {
            connection.send(&command.request).await?;
            connection.receive::<Response>().await
        };
        let result = match request_timeout {
            Some(timeout) => tokio::time::timeout(timeout, operation)
                .await
                .map_err(|_| TransportError::Timeout)
                .and_then(|result| result),
            None => operation.await,
        };
        let failed = result.is_err();
        let _ = command.reply.send(result);
        if failed {
            break;
        }
    }
}

pub struct UdsResponder<Request, Response> {
    listener: Option<UnixListener>,
    connection: Option<FramedTransport<UnixStream>>,
    framing: TransportConfig,
    pending_response: bool,
    marker: PhantomData<(Request, Response)>,
}

impl<Request, Response> UdsResponder<Request, Response> {
    pub fn disconnect(&mut self) {
        self.connection = None;
        self.pending_response = false;
    }
}

impl<Request, Response> Responder<Request, Response> for UdsResponder<Request, Response>
where
    Request: Message + Default,
    Response: Message,
{
    async fn receive(&mut self) -> Result<Request, TransportError> {
        if self.pending_response {
            return Err(TransportError::ResponsePending);
        }
        if self.connection.is_none() {
            let listener = self.listener.as_ref().ok_or(TransportError::Stopped)?;
            let (stream, _) = listener.accept().await?;
            self.connection = Some(FramedTransport::new(stream, self.framing.clone()));
        }
        let result = if let Some(listener) = self.listener.as_ref() {
            loop {
                let connection = self.connection.as_mut().expect("UDS connection is present");
                tokio::select! {
                    result = connection.receive::<Request>() => break result,
                    accepted = listener.accept() => {
                        let (_extra, _) = accepted?;
                    }
                }
            }
        } else {
            self.connection
                .as_mut()
                .expect("UDS connection is present")
                .receive::<Request>()
                .await
        };
        match result {
            Ok(request) => {
                self.pending_response = true;
                Ok(request)
            }
            Err(error) => {
                self.connection = None;
                Err(error)
            }
        }
    }

    async fn respond(&mut self, response: Response) -> Result<(), TransportError> {
        if !self.pending_response {
            return Err(TransportError::NoPendingRequest);
        }
        self.pending_response = false;
        let result = self
            .connection
            .as_mut()
            .ok_or(TransportError::Stopped)?
            .send(&response)
            .await;
        if result.is_err() {
            self.connection = None;
        }
        result
    }
}

fn validate_length(len: usize, max: usize) -> Result<(), TransportError> {
    if len > max {
        return Err(TransportError::FrameTooLarge { len, max });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, PartialEq, Message)]
    struct TestMessage {
        #[prost(string, tag = "1")]
        value: String,
    }

    #[tokio::test]
    async fn round_trips_in_process_requests() {
        let provider = Transport::provide::<InProcTransportProvider>(InProcTransportConfig {
            channel_capacity: 8,
        });
        let (requester, mut responder) = provider.pair::<String, String>();
        let response = tokio::spawn(async move {
            let request = responder.receive().await.expect("request");
            responder.respond(format!("{request}-response")).await.expect("response");
        });
        assert_eq!(requester.request("request".to_string()).await.expect("request"), "request-response");
        response.await.expect("responder");
    }

    #[tokio::test]
    async fn serializes_concurrent_uds_requests() {
        let provider = Transport::provide::<UdsTransportProvider>(UdsTransportConfig::default());
        let (requester, mut responder) = test_support::uds_pair::<TestMessage, TestMessage>(&provider)
            .expect("UDS pair");
        let responder_task = tokio::spawn(async move {
            for _ in 0..2 {
                let request = responder.receive().await.expect("request");
                responder.respond(TestMessage {
                    value: format!("{}-response", request.value),
                }).await.expect("response");
            }
        });
        let first = requester.clone();
        let first_task = tokio::spawn(async move {
            first.request(TestMessage { value: "first".to_string() }).await
        });
        let second_task = tokio::spawn(async move {
            requester.request(TestMessage { value: "second".to_string() }).await
        });
        assert_eq!(first_task.await.expect("first task").expect("first").value, "first-response");
        assert_eq!(second_task.await.expect("second task").expect("second").value, "second-response");
        responder_task.await.expect("responder task");
    }

    #[tokio::test]
    async fn rejects_oversized_frame_before_allocating_payload() {
        let (mut writer, reader) = tokio::io::duplex(128);
        writer.write_all(&129u32.to_le_bytes()).await.expect("header");
        let mut transport = FramedTransport::new(reader, TransportConfig {
            max_frame_size: 128,
            ..TransportConfig::default()
        });
        let error = transport.receive::<TestMessage>().await.expect_err("oversized frame");
        assert!(matches!(error, TransportError::FrameTooLarge { len: 129, max: 128 }));
    }
}
