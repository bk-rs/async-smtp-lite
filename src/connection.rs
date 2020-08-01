use std::fmt;
use std::io;
use std::result;
use std::str::FromStr;

use async_stream_packed::{TlsClientUpgrader, UpgradableAsyncStream};
use futures_util::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use lettre::transport::smtp::{
    authentication::{Credentials, Mechanism},
    commands::*,
    error::Error,
    extension::{ClientId, Extension, MailBodyParameter, MailParameter, ServerInfo},
    response::Response,
};
use lettre::Envelope;

#[cfg(feature = "async_native_tls")]
pub use async_stream_tls_upgrader::AsyncNativeTlsClientTlsUpgrader;
#[cfg(feature = "async_tls")]
pub use async_stream_tls_upgrader::AsyncTlsClientTlsUpgrader;

use self::codec::ClientCodec;

pub type AsyncStream<S, STU> = UpgradableAsyncStream<S, STU>;

// ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L99-L107
pub struct AsyncConnection<S, STU>
where
    STU: TlsClientUpgrader<S>,
{
    stream: AsyncStream<S, STU>,
    panic: bool,
    server_info_: ServerInfo,
}

impl<S, STU> AsyncConnection<S, STU>
where
    STU: TlsClientUpgrader<S>,
{
    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L110-L112
    pub fn server_info(&self) -> &ServerInfo {
        &self.server_info_
    }

    fn from_parts(stream: AsyncStream<S, STU>) -> Self {
        Self {
            stream,
            panic: false,
            server_info_: Default::default(),
        }
    }

    pub fn new(stream: S, upgrader: STU) -> Self {
        Self::from_parts(AsyncStream::new(stream, upgrader))
    }
}

impl<S> AsyncConnection<S, ()>
where
    S: Send + 'static,
{
    pub fn with_tls_stream(stream: S) -> Self {
        Self::from_parts(AsyncStream::with_upgraded_stream(stream))
    }
}

#[cfg(feature = "async_native_tls")]
impl<S> AsyncConnection<S, AsyncNativeTlsClientTlsUpgrader>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub fn with_async_native_tls_upgrader(
        stream: S,
        upgrader: AsyncNativeTlsClientTlsUpgrader,
    ) -> Self {
        Self::new(stream, upgrader)
    }
}

#[cfg(feature = "async_tls")]
impl<S> AsyncConnection<S, AsyncTlsClientTlsUpgrader>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    pub fn with_async_tls_upgrader(stream: S, upgrader: AsyncTlsClientTlsUpgrader) -> Self {
        Self::new(stream, upgrader)
    }
}

impl<S, STU> AsyncConnection<S, STU>
where
    STU: TlsClientUpgrader<S>,
{
    pub async fn stream_tls_upgrade(&mut self) -> result::Result<(), Error> {
        self.stream.upgrade().await.map_err(|err| err.into())
    }
}

//
//
//

// ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L86-L96
#[macro_export]
macro_rules! try_smtp (
    ($err: expr, $client: ident) => ({
        match $err {
            Ok(val) => val,
            Err(err) => {
                $client.abort();
                return Err(From::from(err))
            },
        }
    })
);

impl<S, STU> AsyncConnection<S, STU>
where
    STU: TlsClientUpgrader<S> + Unpin,
    S: AsyncRead + AsyncWrite + Unpin,
    STU::Output: AsyncRead + AsyncWrite + Unpin,
{
    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L187-L191
    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/mod.rs#L441-L475
    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L119-L141
    pub async fn handshake(
        &mut self,
        is_smtps: bool,
        hello_name: ClientId,
    ) -> result::Result<(), Error> {
        if is_smtps && !self.stream.is_upgraded() {
            self.stream_tls_upgrade().await?;
        }

        let _ = self.read_response().await?;

        self.ehlo(&hello_name).await?;

        if self.can_starttls() {
            self.starttls().await?;

            self.stream_tls_upgrade().await?;

            self.ehlo(&hello_name).await?;
        }

        Ok(())
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L143-L166
    pub async fn send(
        &mut self,
        envelope: &Envelope,
        email: &[u8],
    ) -> result::Result<Response, Error> {
        // Mail
        let mut mail_options = vec![];

        if self.server_info().supports_feature(Extension::EightBitMime) {
            mail_options.push(MailParameter::Body(MailBodyParameter::EightBitMime));
        }
        try_smtp!(
            self.command(Mail::new(envelope.from().cloned(), mail_options,))
                .await,
            self
        );

        // Recipient
        for to_address in envelope.to() {
            try_smtp!(
                self.command(Rcpt::new(to_address.clone(), vec![])).await,
                self
            );
        }

        // Data
        try_smtp!(self.command(Data).await, self);

        // Message content
        let result = try_smtp!(self.message(email).await, self);
        Ok(result)
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L168-L170
    pub fn has_broken(&self) -> bool {
        self.panic
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L172-L175
    pub fn can_starttls(&self) -> bool {
        !self.is_encrypted() && self.server_info().supports_feature(Extension::StartTls)
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L178-L201
    pub async fn starttls(&mut self) -> result::Result<(), Error> {
        if self.server_info().supports_feature(Extension::StartTls) {
            try_smtp!(self.command(Starttls).await, self);
            Ok(())
        } else {
            Err(Error::Client("STARTTLS is not supported on this server"))
        }
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L204-L211
    pub async fn ehlo(&mut self, hello_name: &ClientId) -> result::Result<(), Error> {
        let ehlo_response = try_smtp!(
            self.command(Ehlo::new(ClientId::new(hello_name.to_string())))
                .await,
            self
        );
        self.server_info_ = try_smtp!(ServerInfo::from_response(&ehlo_response), self);
        Ok(())
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L213-L215
    pub async fn quit(&mut self) -> result::Result<Response, Error> {
        Ok(try_smtp!(self.command(Quit).await, self))
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L217-L223
    pub fn abort(&mut self) {
        if !self.panic {
            self.panic = true;
            let _ = self.command(Quit);
        }
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L231-L233
    pub fn is_encrypted(&self) -> bool {
        self.stream.is_upgraded()
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L242-L244
    pub async fn test_connected(&mut self) -> bool {
        self.command(Noop).await.is_ok()
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L247-L282
    pub async fn auth(
        &mut self,
        mechanisms: &[Mechanism],
        credentials: &Credentials,
    ) -> result::Result<Response, Error> {
        let mechanism = match self.server_info_.get_auth_mechanism(mechanisms) {
            Some(m) => m,
            None => {
                return Err(Error::Client(
                    "No compatible authentication mechanism was found",
                ))
            }
        };

        // Limit challenges to avoid blocking
        let mut challenges = 10;
        let mut response = self
            .command(Auth::new(mechanism, credentials.clone(), None)?)
            .await?;

        while challenges > 0 && response.has_code(334) {
            challenges -= 1;
            response = try_smtp!(
                self.command(Auth::new_from_response(
                    mechanism,
                    credentials.clone(),
                    &response,
                )?)
                .await,
                self
            );
        }

        if challenges == 0 {
            Err(Error::ResponseParsing("Unexpected number of challenges"))
        } else {
            Ok(response)
        }
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L285-L292
    pub async fn message(&mut self, message: &[u8]) -> result::Result<Response, Error> {
        let mut out_buf: Vec<u8> = vec![];
        let mut codec = ClientCodec::new();
        codec.encode(message, &mut out_buf)?;
        self.write(out_buf.as_slice()).await?;
        self.write(b"\r\n.\r\n").await?;
        self.read_response().await
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L295-L298
    pub async fn command<C: fmt::Display>(
        &mut self,
        command: C,
    ) -> result::Result<Response, Error> {
        self.write(command.to_string().as_bytes()).await?;
        self.read_response().await
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L301-L311
    async fn write(&mut self, string: &[u8]) -> result::Result<(), Error> {
        self.stream.write_all(string).await?;
        self.stream.flush().await?;

        Ok(())
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L314-L340
    pub async fn read_response(&mut self) -> result::Result<Response, Error> {
        let mut buffer = String::with_capacity(100);

        let mut buf_reader = BufReader::new(&mut self.stream);

        while buf_reader.read_line(&mut buffer).await? > 0 {
            match Response::from_str(&buffer) {
                Ok(response) => {
                    if response.is_positive() {
                        return Ok(response);
                    }

                    return Err(response.into());
                }
                // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L328-L334
                // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/error.rs#L104-L112
                Err(Error::Parsing(nom::error::ErrorKind::Complete)) => { /* read more */ }
                Err(err) => return Err(err),
            }
        }

        Err(io::Error::new(io::ErrorKind::Other, "incomplete").into())
    }
}

//
//
//
mod codec {
    use std::io::{self, Write};

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L31-L35
    #[derive(Default, Clone, Copy, Debug)]
    pub struct ClientCodec {
        escape_count: u8,
    }

    // ref https://github.com/lettre/lettre/blob/v0.10.0-alpha.1/src/transport/smtp/client/mod.rs#L37-L77
    impl ClientCodec {
        /// Creates a new client codec
        pub fn new() -> Self {
            ClientCodec::default()
        }

        /// Adds transparency
        pub fn encode(&mut self, frame: &[u8], buf: &mut Vec<u8>) -> io::Result<()> {
            match frame.len() {
                0 => {
                    match self.escape_count {
                        0 => buf.write_all(b"\r\n.\r\n")?,
                        1 => buf.write_all(b"\n.\r\n")?,
                        2 => buf.write_all(b".\r\n")?,
                        _ => unreachable!(),
                    }
                    self.escape_count = 0;
                    Ok(())
                }
                _ => {
                    let mut start = 0;
                    for (idx, byte) in frame.iter().enumerate() {
                        match self.escape_count {
                            0 => self.escape_count = if *byte == b'\r' { 1 } else { 0 },
                            1 => self.escape_count = if *byte == b'\n' { 2 } else { 0 },
                            2 => self.escape_count = if *byte == b'.' { 3 } else { 0 },
                            _ => unreachable!(),
                        }
                        if self.escape_count == 3 {
                            self.escape_count = 0;
                            buf.write_all(&frame[start..idx])?;
                            buf.write_all(b".")?;
                            start = idx;
                        }
                    }
                    buf.write_all(&frame[start..])?;
                    Ok(())
                }
            }
        }
    }
}
