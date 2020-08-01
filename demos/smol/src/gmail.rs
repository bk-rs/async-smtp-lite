/*
cargo run -p async-smtp-lite-demo-smol --bin gmail xxx@gmail.com '123456'
*/

// https://support.google.com/mail/answer/6386757
// https://support.google.com/mail/answer/7126229

// Enable IMAP
// https://mail.google.com/mail/u/0/#settings/fwdandpop

// Allow less secure apps: ON
// https://myaccount.google.com/u/0/lesssecureapps

// Allow
// https://accounts.google.com/b/0/DisplayUnlockCaptcha

use std::env;
use std::io;

use async_net::TcpStream;
use futures_lite::future::block_on;

use async_smtp_lite::lettre::{ClientId, Credentials, Message, DEFAULT_MECHANISMS};
use async_smtp_lite::{AsyncClient, AsyncConnection, AsyncTlsClientTlsUpgrader};

fn main() -> io::Result<()> {
    block_on(run())
}

async fn run() -> io::Result<()> {
    let username = env::args()
        .nth(1)
        .unwrap_or_else(|| env::var("USERNAME").unwrap_or_else(|_| "xxx@gmail.com".to_owned()));
    let password = env::args()
        .nth(2)
        .unwrap_or_else(|| env::var("PASSWORD").unwrap_or_else(|_| "123456".to_owned()));

    //
    for port in [465_u16, 587].iter() {
        let is_smtps = port == &465;
        let hello_name = ClientId::new("lettre".to_owned());
        let credentials = Credentials::new(username.clone(), password.clone());
        let mechanisms = DEFAULT_MECHANISMS;

        let endpoint = "smtp.gmail.com".to_owned();
        let addr = format!("{}:{}", endpoint.clone(), port);
        println!("addr: {}", addr);

        let stream = TcpStream::connect(addr).await?;

        let connection = AsyncConnection::with_async_tls_upgrader(
            stream,
            AsyncTlsClientTlsUpgrader::new(Default::default(), endpoint.clone()),
        );
        let mut client = AsyncClient::new(connection);

        client
            .handshake(is_smtps, hello_name)
            .await
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        let mut session = client
            .auth(mechanisms, &credentials)
            .await
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;

        println!("server_info: {}", session.connection.server_info());

        let email = Message::builder()
            .from(username.parse().unwrap())
            .to(username.parse().unwrap())
            .subject("test async-smtp-lite")
            .body("foo")
            .unwrap();

        session
            .send(&email)
            .await
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
    }

    println!("done");

    Ok(())
}
