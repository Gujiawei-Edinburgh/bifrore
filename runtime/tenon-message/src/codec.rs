use prost::Message;

const MAX_FRAME_LEN: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    FrameTooLarge { len: usize, max: usize },
    IncompleteFrame,
    Decode(String),
}

impl std::fmt::Display for CodecError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FrameTooLarge { len, max } => {
                write!(formatter, "frame length {len} exceeds max {max}")
            }
            Self::IncompleteFrame => formatter.write_str("incomplete frame"),
            Self::Decode(error) => write!(formatter, "failed to decode frame: {error}"),
        }
    }
}

impl std::error::Error for CodecError {}

pub fn encode_frame<M: Message>(message: &M) -> Result<Vec<u8>, CodecError> {
    let len = message.encoded_len();
    if len > MAX_FRAME_LEN {
        return Err(CodecError::FrameTooLarge {
            len,
            max: MAX_FRAME_LEN,
        });
    }
    let mut frame = Vec::with_capacity(4 + len);
    frame.extend_from_slice(&(len as u32).to_le_bytes());
    message.encode(&mut frame).map_err(|error| {
        CodecError::Decode(error.to_string())
    })?;
    Ok(frame)
}

pub fn decode_frame<M: Message + Default>(frame: &[u8]) -> Result<M, CodecError> {
    if frame.len() < 4 {
        return Err(CodecError::IncompleteFrame);
    }
    let len = u32::from_le_bytes(frame[..4].try_into().expect("slice length checked")) as usize;
    if len > MAX_FRAME_LEN {
        return Err(CodecError::FrameTooLarge {
            len,
            max: MAX_FRAME_LEN,
        });
    }
    if frame.len() < 4 + len {
        return Err(CodecError::IncompleteFrame);
    }
    M::decode(&frame[4..4 + len]).map_err(|error| CodecError::Decode(error.to_string()))
}
