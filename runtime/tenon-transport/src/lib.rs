use prost::Message;
use std::fmt;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InProcTransportConfig {
    pub channel_capacity: usize,
}

pub struct InProcTransportProvider {
    config: InProcTransportConfig,
}

pub struct InProcClient<Request, Response> {
    sender: mpsc::Sender<(Request, oneshot::Sender<Response>)>,
}

pub struct InProcServer<Request, Response> {
    receiver: mpsc::Receiver<(Request, oneshot::Sender<Response>)>,
}

pub struct InProcRequest<Request, Response> {
    request: Option<Request>,
    reply: oneshot::Sender<Response>,
}

impl InProcTransportProvider {
    pub fn create<Request, Response>(
        &self,
    ) -> (InProcClient<Request, Response>, InProcServer<Request, Response>) {
        let (sender, receiver) = mpsc::channel(self.config.channel_capacity);
        (InProcClient { sender }, InProcServer { receiver })
    }
}

impl TransportProvider for InProcTransportProvider {
    type Config = InProcTransportConfig;

    fn new(config: Self::Config) -> Self {
        Self { config }
    }
}

impl<Request, Response> Clone for InProcClient<Request, Response> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<Request, Response> InProcClient<Request, Response> {
    pub async fn request(&self, request: Request) -> Result<Response, InProcTransportError> {
        let (reply, response) = oneshot::channel();
        self.sender
            .send((request, reply))
            .await
            .map_err(|_| InProcTransportError::Stopped)?;
        response
            .await
            .map_err(|_| InProcTransportError::ResponseDropped)
    }
}

impl<Request, Response> InProcServer<Request, Response> {
    pub async fn receive(&mut self) -> Option<InProcRequest<Request, Response>> {
        self.receiver
            .recv()
            .await
            .map(|(request, reply)| InProcRequest {
                request: Some(request),
                reply,
            })
    }
}

impl<Request, Response> InProcRequest<Request, Response> {
    pub fn take_request(&mut self) -> Option<Request> {
        self.request.take()
    }

    pub fn respond(self, response: Response) -> Result<(), InProcTransportError> {
        self.reply
            .send(response)
            .map_err(|_| InProcTransportError::ResponseDropped)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InProcTransportError {
    Stopped,
    ResponseDropped,
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
}

impl Default for UdsTransportConfig {
    fn default() -> Self {
        Self {
            framing: TransportConfig::default(),
            connect_timeout: Duration::from_secs(10),
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

#[derive(Debug)]
pub struct FramedTransport<S> {
    io: S,
    config: TransportConfig,
}

impl<S> FramedTransport<S> {
    pub fn new(io: S, config: TransportConfig) -> Self {
        Self { io, config }
    }

    pub fn into_inner(self) -> S {
        self.io
    }

    pub fn config(&self) -> &TransportConfig {
        &self.config
    }
}

impl<S> FramedTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn send<M: Message>(&mut self, message: &M) -> Result<(), TransportError> {
        let timeout = self.config.write_timeout;
        let operation = self.send_inner(message);
        match timeout {
            Some(timeout) => tokio::time::timeout(timeout, operation)
                .await
                .map_err(|_| TransportError::Timeout)?,
            None => operation.await,
        }
    }

    pub async fn receive<M: Message + Default>(&mut self) -> Result<M, TransportError> {
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
    pub async fn connect(&self, path: impl Into<PathBuf>) -> Result<UdsConnection, TransportError> {
        let path = path.into();
        let display_path = path.clone();
        let config = self.config.clone();
        tokio::time::timeout(config.connect_timeout, async move {
            loop {
                match UnixStream::connect(&path).await {
                    Ok(stream) => {
                        return Ok(UdsConnection::from_stream(stream, config.framing.clone()));
                    }
                    Err(_) => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .map_err(|_| TransportError::ConnectTimeout {
            path: display_path,
        })?
    }

    pub fn bind(&self, path: impl AsRef<Path>) -> Result<UdsTransportListener, TransportError> {
        let listener = UnixListener::bind(path)?;
        Ok(UdsTransportListener {
            listener,
            framing: self.config.framing.clone(),
        })
    }
}

#[derive(Debug)]
pub struct UdsTransportListener {
    listener: UnixListener,
    framing: TransportConfig,
}

impl UdsTransportListener {
    pub async fn accept(&self) -> Result<UdsConnection, TransportError> {
        let (stream, _) = self.listener.accept().await?;
        Ok(UdsConnection::from_stream(stream, self.framing.clone()))
    }
}

#[derive(Debug)]
pub struct UdsConnection {
    framed: FramedTransport<UnixStream>,
}

impl UdsConnection {
    pub fn from_stream(stream: UnixStream, config: TransportConfig) -> Self {
        Self {
            framed: FramedTransport::new(stream, config),
        }
    }

    pub async fn send<M: Message>(&mut self, message: &M) -> Result<(), TransportError> {
        self.framed.send(message).await
    }

    pub async fn receive<M: Message + Default>(&mut self) -> Result<M, TransportError> {
        self.framed.receive().await
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
    use prost::Message;
    use tokio::io::DuplexStream;

    #[derive(Clone, PartialEq, Message)]
    struct TestMessage {
        #[prost(string, tag = "1")]
        value: String,
    }

    fn pair() -> (FramedTransport<DuplexStream>, FramedTransport<DuplexStream>) {
        let (left, right) = tokio::io::duplex(128);
        (
            FramedTransport::new(left, TransportConfig::default()),
            FramedTransport::new(right, TransportConfig::default()),
        )
    }

    #[tokio::test]
    async fn round_trips_protobuf_messages() {
        let (mut sender, mut receiver) = pair();
        let expected = TestMessage {
            value: "hello".to_string(),
        };
        sender.send(&expected).await.expect("send");
        assert_eq!(receiver.receive::<TestMessage>().await.expect("receive"), expected);
    }

    #[tokio::test]
    async fn rejects_oversized_frame_before_allocating_payload() {
        let (mut writer, mut reader) = pair();
        writer.io.write_all(&129u32.to_le_bytes()).await.expect("header");
        reader.config.max_frame_size = 128;
        let error = reader.receive::<TestMessage>().await.expect_err("oversized frame");
        assert!(matches!(error, TransportError::FrameTooLarge { len: 129, max: 128 }));
    }
}
