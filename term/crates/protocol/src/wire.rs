//! Length-prefixed JSON framing.

use serde::Serialize;
use serde::de::DeserializeOwned;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// Largest accepted message (a full-screen frame is far smaller).
pub const MAX_FRAME: usize = 16 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("connection closed")]
    Closed,
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("message too large ({0} bytes)")]
    TooLarge(usize),
    #[error("malformed message: {0}")]
    Json(#[from] serde_json::Error),
}

pub async fn write_msg<W, T>(w: &mut W, msg: &T) -> Result<(), WireError>
where
    W: AsyncWrite + Unpin,
    T: Serialize,
{
    let body = serde_json::to_vec(msg)?;
    if body.len() > MAX_FRAME {
        return Err(WireError::TooLarge(body.len()));
    }
    let len = u32::try_from(body.len()).map_err(|_| WireError::TooLarge(body.len()))?;
    w.write_all(&len.to_be_bytes()).await?;
    w.write_all(&body).await?;
    w.flush().await?;
    Ok(())
}

pub async fn read_msg<R, T>(r: &mut R) -> Result<T, WireError>
where
    R: AsyncRead + Unpin,
    T: DeserializeOwned,
{
    let mut len = [0u8; 4];
    match r.read_exact(&mut len).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Err(WireError::Closed),
        Err(e) => return Err(e.into()),
    }
    let len = u32::from_be_bytes(len) as usize;
    if len > MAX_FRAME {
        return Err(WireError::TooLarge(len));
    }
    let mut body = vec![0u8; len];
    r.read_exact(&mut body).await?;
    Ok(serde_json::from_slice(&body)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ClientMsg, Request};

    #[tokio::test]
    async fn frames_round_trip_and_reject_oversize() {
        let (mut a, mut b) = tokio::io::duplex(1024);
        let m = ClientMsg {
            id: 1,
            req: Request::List,
        };
        write_msg(&mut a, &m).await.expect("write");
        let got: ClientMsg = read_msg(&mut b).await.expect("read");
        assert_eq!(got, m);

        a.write_all(&(u32::MAX).to_be_bytes()).await.expect("w");
        assert!(matches!(
            read_msg::<_, ClientMsg>(&mut b).await,
            Err(WireError::TooLarge(_))
        ));
        drop(a);
        assert!(matches!(
            read_msg::<_, ClientMsg>(&mut b).await,
            Err(WireError::Closed)
        ));
    }
}
