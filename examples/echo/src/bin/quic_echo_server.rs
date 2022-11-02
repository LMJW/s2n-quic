// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use core::task::Poll;
use once_cell::sync::OnceCell;
use s2n_quic::provider::tls::s2n_tls::{ClientHelloCallback, Connection};
use s2n_quic::Server;
use s2n_tls::{
    callbacks::{ConfigResolver, ConnectionFuture},
    config::Config,
    error::Error as S2nError,
};
use std::error::Error;
use std::pin::Pin;

pub struct MyClientHelloHandler {}

impl ClientHelloCallback for MyClientHelloHandler {
    fn on_client_hello(
        &self,
        conn: &mut Connection,
    ) -> Result<Option<Pin<Box<dyn ConnectionFuture>>>, S2nError> {
        let sni = conn.server_name().unwrap();
        println!("trying");
        let fut = async move {
            let err = S2nError::application(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "oh no",
            )));
            println!("error is retryable: {}", err.is_retryable());
            Err(err)
        };

        Ok(Some(Box::pin(ConfigResolver::new(fut))))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let tls = s2n_quic::provider::tls::s2n_tls::Server::builder()
        //.with_certificate(CERT_PEM, KEY_PEM)?
        //.with_io("127.0.0.1:4433")?
        .with_client_hello_handler(MyClientHelloHandler {})?
        .build()?;

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
