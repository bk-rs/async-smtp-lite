/*
cargo run -p async-smtp-lite-demo-smol --bin aws_workmail us-west-2 foo@example.com '123456'
*/

// https://aws.amazon.com/premiumsupport/knowledge-center/workmail-on-premises-multifunction/

use std::env;
use std::io;
use std::net::{TcpStream, ToSocketAddrs};

use async_io::Async;
use blocking::block_on;

use async_smtp_lite::lettre::{ClientId, Credentials, Message, DEFAULT_MECHANISMS};
use async_smtp_lite::{AsyncClient, AsyncConnection, AsyncTlsClientTlsUpgrader};

fn main() -> io::Result<()> {
    block_on(run())
}

async fn run() -> io::Result<()> {
    let region = env::args()
        .nth(1)
        .unwrap_or_else(|| env::var("REGION").unwrap_or_else(|_| "us-west-2".to_owned()));
    let username = env::args()
        .nth(2)
        .unwrap_or_else(|| env::var("USERNAME").unwrap_or_else(|_| "foo@example.com".to_owned()));
    let password = env::args()
        .nth(3)
        .unwrap_or_else(|| env::var("PASSWORD").unwrap_or_else(|_| "123456".to_owned()));

    let is_smtps = true;
    let hello_name = ClientId::new(username.clone());
    let mechanisms = DEFAULT_MECHANISMS;
    let credentials = Credentials::new(username.clone(), password.clone());

    //
    let endpoint = format!("smtp.mail.{}.awsapps.com", region);
    let port: u16 = 465;

    println!("endpoint: {}", endpoint);
    let addr = format!("{}:{}", endpoint.clone(), port)
        .to_socket_addrs()
        .unwrap()
        .next()
        .unwrap();

    println!("addr: {}", addr);
    let stream = Async::<TcpStream>::connect(addr).await?;

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

    println!("done");

    Ok(())
}
