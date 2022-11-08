// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use s2n_quic::Server;
use std::error::Error;
use s2n_quic::provider::tls::s2n_tls::{self as s2n_quic_tls, ClientHelloCallback, Connection, ConfigLoader, ConnectionContext};
use core::{task::Poll, sync::atomic::{AtomicBool, Ordering}};
use std::sync::Arc;

/// NOTE: this certificate is to be used for demonstration purposes only!
pub static CERT_PEM: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../quic/s2n-quic-core/certs/cert.pem"
));
/// NOTE: this certificate is to be used for demonstration purposes only!
pub static KEY_PEM: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../quic/s2n-quic-core/certs/key.pem"
));

#[derive(Clone)]
pub struct MyClientHelloHandler{}

impl ClientHelloCallback for MyClientHelloHandler {
    fn poll_client_hello(&self, conn: &mut Connection) -> core::task::Poll<Result<(),s2n_tls::error::Error>> {
        let mut config = s2n_tls::config::Builder::new();

        config.enable_quic().unwrap(); 
        config.set_security_policy(&s2n_tls::security::DEFAULT_TLS13).unwrap(); 
        config.set_application_protocol_preference([b"h3"]).unwrap(); 
        config.set_client_hello_callback(self.clone());

        config.load_pem(CERT_PEM.as_bytes(), KEY_PEM.as_bytes()).unwrap();
        let config = config.build().unwrap();

        conn.set_config(config).unwrap();

        println!("Set config done");
        conn.server_name_extension_used(); // If I use sni to config the connection, I need to call this.
        Poll::Ready(Ok(()))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let tls = s2n_quic::provider::tls::s2n_tls::Server::builder()
        //.with_certificate(CERT_PEM, KEY_PEM)?
        //.with_io("127.0.0.1:4433")?
        .with_client_hello_handler(MyClientHelloHandler{})?
        .build()?;

    // let tls = s2n_quic::provider::tls::s2n_tls::Server::from_loader(MyLoader{});

    let mut server = Server::builder()
        // .with_tls((CERT_PEM, KEY_PEM))?    
        .with_tls(tls)?
        .with_io("127.0.0.1:4433")?
        .start()?;

    while let Some(mut connection) = server.accept().await {
        // spawn a new task for the connection
        tokio::spawn(async move {
            eprintln!("Connection accepted from {:?}", connection.remote_addr());

            while let Ok(Some(mut stream)) = connection.accept_bidirectional_stream().await {
                // spawn a new task for the stream
                tokio::spawn(async move {
                    eprintln!("Stream opened from {:?}", stream.connection().remote_addr());

                    // echo any data back to the stream
                    while let Ok(Some(data)) = stream.receive().await {
                        stream.send(data).await.expect("stream should be open");
                    }
                });
            }
        });
    }

    Ok(())
}
