use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::Ordering;
use std::time::Duration;

use anyhow::{Context, Result};
use tokio::net::TcpListener;

use crate::runtime::BaselineRuntime;

pub struct TeaSpeakTcpTransportServer {
    pub server_id: u32,
    pub bind_addr: String,
    pub runtime: Arc<Mutex<BaselineRuntime>>,
}

impl TeaSpeakTcpTransportServer {
    pub fn bind_with_shared_runtime(
        server_id: u32,
        runtime: Arc<Mutex<BaselineRuntime>>,
        bind_addr: &str,
    ) -> Result<Self> {
        Ok(Self {
            server_id,
            bind_addr: bind_addr.to_string(),
            runtime,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr> {
        self.bind_addr.parse().context("invalid bind address")
    }

    pub fn run(self, should_stop: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Result<()> {
        let bind_addr_str = self.bind_addr.clone();
        
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async move {
            let mut v4_addr = bind_addr_str.clone();
            let mut v6_addr = bind_addr_str.replace("0.0.0.0:", "[::]:");
            
            let listener_v4 = TcpListener::bind(&v4_addr).await;
            let listener_v6 = TcpListener::bind(&v6_addr).await;
            
            if listener_v4.is_err() && listener_v6.is_err() {
                println!("teaspeak tcp: failed to bind both IPv4 and IPv6");
                return Ok::<(), anyhow::Error>(());
            }
            
            let should_stop_v4 = Arc::clone(&should_stop);
            let should_stop_v6 = Arc::clone(&should_stop);
            
            let handle_v4 = tokio::spawn(async move {
                if let Ok(listener) = listener_v4 {
                    println!("virtual server 1 teaspeak tcp transport listening on tcp {}", v4_addr);
                    loop {
                        if should_stop_v4.load(Ordering::Relaxed) {
                            break;
                        }
                        let result = tokio::time::timeout(Duration::from_millis(500), listener.accept()).await;
                        if let Ok(Ok((mut stream, addr))) = result {
                            println!("teaspeak tcp (v4) accepted connection from {}", addr);
                            tokio::spawn(async move {
                                use tokio::io::AsyncReadExt;
                                let mut buf = [0u8; 1024];
                                loop {
                                    match stream.read(&mut buf).await {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            println!("teaspeak tcp (v4) received {} bytes from {}: {:?}", n, addr, &buf[..n]);
                                        }
                                        Err(_) => break,
                                    }
                                }
                            });
                        }
                    }
                }
            });
            
            let handle_v6 = tokio::spawn(async move {
                if let Ok(listener) = listener_v6 {
                    println!("virtual server 1 teaspeak tcp transport listening on tcp {}", v6_addr);
                    loop {
                        if should_stop_v6.load(Ordering::Relaxed) {
                            break;
                        }
                        let result = tokio::time::timeout(Duration::from_millis(500), listener.accept()).await;
                        if let Ok(Ok((mut stream, addr))) = result {
                            println!("teaspeak tcp (v6) accepted connection from {}", addr);
                            tokio::spawn(async move {
                                use tokio::io::AsyncReadExt;
                                let mut buf = [0u8; 1024];
                                loop {
                                    match stream.read(&mut buf).await {
                                        Ok(0) => break,
                                        Ok(n) => {
                                            println!("teaspeak tcp (v6) received {} bytes from {}: {:?}", n, addr, &buf[..n]);
                                        }
                                        Err(_) => break,
                                    }
                                }
                            });
                        }
                    }
                }
            });
            
            let _ = tokio::join!(handle_v4, handle_v6);
            Ok(())
        })
    }
}
