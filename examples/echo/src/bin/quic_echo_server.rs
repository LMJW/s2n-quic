// Copyright Amazon.com, Inc. or its affiliates. All Rights Reserved.
// SPDX-License-Identifier: Apache-2.0

use s2n_quic::Server;
use std::error::Error;
use s2n_quic::provider::tls::s2n_tls::{self as s2n_quic_tls, ClientHelloCallback, Connection, ConfigLoader, ConnectionContext};
use core::{task::{Context, Poll, Waker}, sync::atomic::{AtomicBool, Ordering}};
use std::{
    pin::Pin,
    sync::{Arc, Mutex},
    thread,
};
use std::ops::DerefMut;
use tokio::time::{sleep, Duration};
use futures::Future;
use std::collections::HashMap;

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

#[derive(Debug)]
pub struct MyClientHelloHandler{
    state: Mutex<HashMap<String, Pin<Box<MyTimeoutFuture>>>>,
}


impl MyClientHelloHandler{
    fn new() -> Self{
        Self{
            state: Mutex::new(HashMap::new()),
        }
    }
}

#[derive(Debug)]
pub struct MyTimeoutFuture{
    shared_state: Arc<Mutex<SharedState>>,
}

#[derive(Debug)]
struct SharedState{
    completed: bool,
    waker: Option<Waker>,
}

impl Future for MyTimeoutFuture {
    type Output = Result<(Vec<u8>, Vec<u8>), ()>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut shared_state = self.shared_state.lock().unwrap();
        if shared_state.completed {
            Poll::Ready(Ok((vec![], vec![])))
        }else {
            shared_state.waker = Some(cx.waker().clone());
            Poll::Pending
        }
    }
}

impl MyTimeoutFuture {
    pub fn new(duration: Duration, waker: Option<Waker>) -> Self {
        println!("Creating new MyTimeoutFuture");
        let shared_state = Arc::new(Mutex::new(SharedState {
            completed: false,
            waker,
        }));

        // Spawn the new thread
        let thread_shared_state = shared_state.clone();
        tokio::spawn(async move {
            //println!("sleep started");
            tokio::time::sleep(duration).await;
            //println!("sleep completed");
            let mut shared_state = thread_shared_state.lock().unwrap();
            // Signal that the timer has completed and wake up the last
            // task on which the future was polled, if one exists.
            shared_state.completed = true;
            if let Some(waker) = shared_state.waker.take() {
                //println!("Call wake");
                waker.wake()
            }
        });

        Self { shared_state }
    }
}

impl ClientHelloCallback for MyClientHelloHandler {
    fn poll_client_hello(&self, conn: &mut Connection) -> core::task::Poll<Result<(),s2n_tls::error::Error>> {
        //println!("Poll MyClientHelloHandler..");

        if let Some(sni) = conn.server_name() {
            let waker = conn.waker().unwrap();
            let mut ctx = Context::from_waker(waker);
            let mut map = self.state.lock().unwrap();

            let mut fut = map.remove(sni).unwrap_or_else(||Box::pin(MyTimeoutFuture::new(Duration::from_secs(6), Some(waker.clone()))));

            match fut.as_mut().poll(&mut ctx) {
                Poll::Pending => {
                    map.insert(sni.to_string(), fut);
                    Poll::Pending
                },
                Poll::Ready(v) => {
                    //println!("{:?}", v);
                    let mut config = s2n_tls::config::Builder::new();

                    config.enable_quic().unwrap(); 
                    config.set_security_policy(&s2n_tls::security::DEFAULT_TLS13).unwrap(); 
                    config.set_application_protocol_preference([b"h3"]).unwrap(); 
                    //config.set_client_hello_callback(self.clone());

                    config.load_pem(CERT_PEM.as_bytes(), KEY_PEM.as_bytes()).unwrap();
                    let config = config.build().unwrap();

                    conn.set_config(config).unwrap();

                    println!("Set config done");
                    conn.server_name_extension_used();
                    Poll::Ready(Ok(()))
                }
            }

        }else{
            Poll::Ready(Err(todo!()))
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let tls = s2n_quic::provider::tls::s2n_tls::Server::builder()
        //.with_certificate(CERT_PEM, KEY_PEM)?
        //.with_io("127.0.0.1:4433")?
        .with_client_hello_handler(MyClientHelloHandler::new())?
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
            // eprintln!("Connection accepted from {:?}", connection.remote_addr());

            while let Ok(Some(mut stream)) = connection.accept_bidirectional_stream().await {
                // spawn a new task for the stream
                tokio::spawn(async move {
                    // eprintln!("Stream opened from {:?}", stream.connection().remote_addr());

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
