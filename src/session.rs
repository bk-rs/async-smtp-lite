use std::result;

use async_stream_packed::TlsClientUpgrader;
use futures_util::io::{AsyncRead, AsyncWrite};
use lettre::message::Message;
use lettre::transport::smtp::error::Error;
use lettre::Envelope;

use crate::connection::AsyncConnection;

pub struct AsyncSession<'a, S, STU>
where
    STU: TlsClientUpgrader<S>,
{
    pub connection: &'a mut AsyncConnection<S, STU>,
}

impl<'a, S, STU> AsyncSession<'a, S, STU>
where
    STU: TlsClientUpgrader<S> + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
    STU::Output: AsyncRead + AsyncWrite + Unpin,
{
    pub(crate) fn new(connection: &'a mut AsyncConnection<S, STU>) -> Self {
        Self { connection }
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/lib.rs#L145-L149
    pub async fn send(&mut self, message: &Message) -> result::Result<(), Error> {
        let raw = message.formatted();
        self.send_raw(message.envelope(), &raw).await
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/mod.rs#L264-L276
    pub async fn send_raw(
        &mut self,
        envelope: &Envelope,
        email: &[u8],
    ) -> result::Result<(), Error> {
        let _ = self.connection.send(envelope, email).await?;

        self.connection.quit().await?;

        Ok(())
    }
}
