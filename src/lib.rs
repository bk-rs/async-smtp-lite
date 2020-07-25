pub mod lettre {
    pub use ::lettre::transport::smtp::{
        authentication::{Credentials, Mechanism, DEFAULT_MECHANISMS},
        extension::ClientId,
    };

    pub use ::lettre::message::Message;
}

mod client;
mod connection;
mod session;

pub use client::AsyncClient;
pub use connection::AsyncConnection;
pub use session::AsyncSession;

#[cfg(feature = "async_native_tls")]
pub use connection::AsyncNativeTlsClientTlsUpgrader;
#[cfg(feature = "async_tls")]
pub use connection::AsyncTlsClientTlsUpgrader;
