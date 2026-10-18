use std::io::{self, BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::protocols::tsdns::resolver::resolve_tsdns_hostname;
use crate::runtime::BaselineRuntime;

pub const DEFAULT_TSDNS_BIND: &str = "0.0.0.0:41144";

pub struct TsdnsTcpServer {
    listener: TcpListener,
    runtime: Arc<Mutex<BaselineRuntime>>,
}

impl TsdnsTcpServer {
    pub fn bind_with_shared_runtime(
        runtime: Arc<Mutex<BaselineRuntime>>,
        bind_addr: &str,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(bind_addr)?;
        listener.set_nonblocking(true)?;
        Ok(Self { listener, runtime })
    }

    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.listener.local_addr()
    }

    pub fn run(&self, should_stop: Arc<AtomicBool>) -> io::Result<()> {
        while !should_stop.load(Ordering::SeqCst) {
            match self.listener.accept() {
                Ok((stream, _peer)) => {
                    let rt = Arc::clone(&self.runtime);
                    std::thread::spawn(move || {
                        let _ = handle_tsdns_connection(stream, rt);
                    });
                }
                Err(err) if err.kind() == io::ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(15));
                }
                Err(err) => return Err(err),
            }
        }
        Ok(())
    }
}

fn handle_tsdns_connection(
    mut stream: TcpStream,
    runtime: Arc<Mutex<BaselineRuntime>>,
) -> io::Result<()> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));

    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line)?;
    if bytes_read == 0 {
        return Ok(());
    }

    let response = if let Ok(rt) = runtime.lock() {
        resolve_tsdns_hostname(&rt, &line).unwrap_or_else(|| String::from("404"))
    } else {
        String::from("404")
    };

    stream.write_all(format!("{response}\r\n").as_bytes())?;
    stream.flush()?;
    Ok(())
}
