use std::result;

use async_stream_packed::TlsClientUpgrader;
use futures_util::io::{AsyncRead, AsyncWrite};
use lettre::transport::smtp::{
    authentication::{Credentials, Mechanism},
    error::Error,
    extension::ClientId,
};

use crate::connection::AsyncConnection;
use crate::session::AsyncSession;

pub struct AsyncClient<S, STU>
where
    STU: TlsClientUpgrader<S>,
{
    connection: AsyncConnection<S, STU>,
}

impl<S, STU> AsyncClient<S, STU>
where
    STU: TlsClientUpgrader<S>,
{
    pub fn new(connection: AsyncConnection<S, STU>) -> Self {
        Self { connection }
    }
}

impl<S, STU> AsyncClient<S, STU>
where
    STU: TlsClientUpgrader<S> + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
    STU::Output: AsyncRead + AsyncWrite + Unpin,
{
    pub async fn handshake(
        &mut self,
        is_smtps: bool,
        hello_name: ClientId,
    ) -> result::Result<(), Error> {
        self.connection.handshake(is_smtps, hello_name).await
    }

    pub async fn auth<'a>(
        &'a mut self,
        mechanisms: &[Mechanism],
        credentials: &Credentials,
    ) -> result::Result<AsyncSession<'_, S, STU>, Error> {
        self.connection.auth(mechanisms, credentials).await?;

        Ok(AsyncSession::new(&mut self.connection))
    }
}
