use std::env;
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::thread;

use anyhow::Result;
use blackteaspeak_server::desktop_capture::{
    DEFAULT_DESKTOP_CAPTURE_BIND, DEFAULT_DESKTOP_COMPAT_MAX_BYTES, DesktopCaptureServer,
    DesktopResponder, DesktopResponseAction, DesktopUdpServer,
};
use blackteaspeak_server::desktop_transport::DesktopTransportServer;
use blackteaspeak_server::file_transfer::{DEFAULT_FILE_TRANSFER_BIND, FileTransferServer};
use blackteaspeak_server::runtime::{QuerySessionState, create_baseline_runtime};
use blackteaspeak_server::specs::FoundationSpecs;
use blackteaspeak_server::ssh_query::SshQueryServer;
use blackteaspeak_server::web_client::{DEFAULT_WEB_CLIENT_BIND, WebClientServer};
use blackteaspeak_server::web_transport::{
    DEFAULT_TEAWEB_BIND, BlackTeaWebTransportServer, generate_localhost_tls_assets,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let workspace_root = FoundationSpecs::discover_workspace_root(&env::current_dir()?)?;
    let mut args = env::args().skip(1).collect::<Vec<_>>();

    match args.first().map(String::as_str) {
        None | Some("info") => print_info(&workspace_root),
        Some("repl") => run_repl(&workspace_root),
        Some("serve") => {
            args.remove(0);
            run_serve(&workspace_root, &args)
        }
        Some("serve-all") => {
            args.remove(0);
            run_serve_all(&workspace_root, &args)
        }
        Some("serve-web") => {
            args.remove(0);
            run_serve_web(&workspace_root, &args)
        }
        Some("serve-desktop") => {
            args.remove(0);
            run_serve_desktop(&args)
        }
        Some("capture-desktop") => {
            args.remove(0);
            run_capture_desktop(&args)
        }
        Some("generate-web-cert") => {
            args.remove(0);
            run_generate_web_cert(&workspace_root, &args)
        }
        Some("exec") => {
            args.remove(0);
            run_exec(&workspace_root, &args.join(" "))
        }
        Some("help") | Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        Some(_) => run_exec(&workspace_root, &args.join(" ")),
    }
}

fn print_info(workspace_root: &std::path::Path) -> Result<()> {
    let specs = FoundationSpecs::load(workspace_root)?;

    println!("workspace={}", workspace_root.display());
    println!("commands={}", specs.commands.len());
    println!("permission_groups={}", specs.permission_groups.len());
    println!("subsystems={}", specs.subsystems.len());
    println!(
        "baseline_commands={}",
        specs.baseline_profile.essential_commands.len()
    );
    println!("binary={}", specs.binary_manifest.binary.path.display());
    println!("version={}", specs.build_version.build_version);
    println!("blackteaweb_bind_default={DEFAULT_TEAWEB_BIND}");
    println!("file_transfer_bind_default={DEFAULT_FILE_TRANSFER_BIND}");
    println!("desktop_capture_bind_default={DEFAULT_DESKTOP_CAPTURE_BIND}");
    Ok(())
}

fn run_exec(workspace_root: &std::path::Path, line: &str) -> Result<()> {
    let mut runtime = create_baseline_runtime(workspace_root)?;
    let mut session = QuerySessionState::default();
    println!("{}", runtime.execute(line, &mut session));
    Ok(())
}

fn run_repl(workspace_root: &std::path::Path) -> Result<()> {
    let mut runtime = create_baseline_runtime(workspace_root)?;
    let mut session = QuerySessionState::default();
    let stdin = io::stdin();

    println!("BlackTeaSpeak compatibility baseline REPL");
    println!("Type help, version, login serveradmin serveradmin, use sid=1, serverinfo or exit.");

    loop {
        print!("query> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if matches!(trimmed, "exit" | "quit") {
            break;
        }

        println!("{}", runtime.execute(trimmed, &mut session));
    }

    Ok(())
}

fn run_serve(workspace_root: &std::path::Path, args: &[String]) -> Result<()> {
    let bind_addr = resolve_bind_address(args, "0.0.0.0:10022");
    
    println!("ssh query transport listening on {}", bind_addr);
    println!("press Ctrl+C to stop");

    let runtime = Arc::new(Mutex::new(create_baseline_runtime(workspace_root)?));
    
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let config = russh::server::Config {
            auth_rejection_time: std::time::Duration::from_secs(1),
            keys: vec![russh::keys::PrivateKey::from_openssh(b"-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABBMysm/AA\nww+NKFkm+obdbCAAAAGAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAINYhFILjCe6TjZA2\nllBEHEe6j0lpuPvyfzjyUwTXWFm+AAAAoDW/2jvvNIrDLkmgR3m91yqRQRoH5n/G1dU1F7\npKU4NPsm6DDvwVz8R/naBMR6xhrbBpQDT+HEpeZukq4e0CBfnF9P6/eEsc7E3gCoEgzIP7\nLBniw65nd6szWsj6AUDCiVeaXIeswML1gvpbkdhBk8jMsZotNRcgEnQe4No36Hhe2avFgy\nx9UivfDLNFzIHOhH9jI8CyGPU2iAbkJhzDFcw=\n-----END OPENSSH PRIVATE KEY-----\n").unwrap()],
            ..Default::default()
        };
        let config = std::sync::Arc::new(config);
        let mut server = SshQueryServer { runtime };
        let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
        while let Ok((stream, addr)) = listener.accept().await {
            use russh::server::Server;
            let handler = server.new_client(Some(addr));
            let cfg = std::sync::Arc::clone(&config);
            tokio::spawn(async move {
                let _ = russh::server::run_stream(cfg, stream, handler).await;
            });
        }
        Ok::<(), anyhow::Error>(())
    })?;
    Ok(())
}

fn run_serve_all(workspace_root: &std::path::Path, args: &[String]) -> Result<()> {
    let web_bind_addr = resolve_bind_address_for(args, &["--web-bind"], DEFAULT_TEAWEB_BIND);
    let web_client_bind_addr = resolve_bind_address_for(args, &["--web-client-bind"], DEFAULT_WEB_CLIENT_BIND);
    let file_bind_addr = resolve_bind_address_for(args, &["--file-bind"], DEFAULT_FILE_TRANSFER_BIND);
    let desktop_bind_addr = resolve_desktop_bind_addr(args, &web_bind_addr);
    let desktop_max_bytes = resolve_usize_option(
        args,
        "--desktop-max-bytes",
        DEFAULT_DESKTOP_COMPAT_MAX_BYTES,
    );
    let desktop_responder = desktop_bind_addr
        .as_ref()
        .map(|_| resolve_desktop_responder(args))
        .transpose()?;
    let (cert_path, key_path) = resolve_blackteaweb_tls_paths(workspace_root, args);
    let runtime = Arc::new(Mutex::new(create_baseline_runtime(workspace_root)?));
    let file_transfers = {
        let runtime_guard = runtime
            .lock()
            .map_err(|_| io::Error::other("runtime lock poisoned"))?;
        blackteaspeak_server::file_transfer::FileTransferRegistry::new(workspace_root)
    };
    let file_transfers = Arc::new(file_transfers);
    let (music_download_tx, music_download_rx) = std::sync::mpsc::channel();
    {
        let mut rt = runtime.lock().unwrap();
        rt.set_file_transfer_registry(Arc::clone(&file_transfers));
        rt.music_download_tx = Some(music_download_tx);
    }
    let dl_runtime = Arc::clone(&runtime);
    let dl_file_transfers = Arc::clone(&file_transfers);
    thread::spawn(move || {
        for (bot_id, song_id, url) in music_download_rx {
            let output_path = match dl_file_transfers.music_download_path(&format!("{song_id}.mp3")) {
                Ok(path) => path,
                Err(error) => {
                    eprintln!("failed to resolve music download path for {song_id}: {error}");
                    continue;
                }
            };
            if blackteaspeak_server::runtime::BaselineRuntime::download_youtube_track(&url, &output_path) {
                if let Ok(mut rt) = dl_runtime.lock() {
                    rt.update_downloaded_music_track(bot_id, song_id, "ffmpeg", &format!("/music/{song_id}.mp3"));
                }
            }
        }
    });

    let tracker_rt = Arc::clone(&runtime);
    file_transfers.add_notifier(Arc::new(move |event| {
        use blackteaspeak_server::file_transfer::FileTransferEvent;
        if let FileTransferEvent::Progress { client_id, network_bytes_received, network_bytes_send, .. } = event {
            if let Ok(mut rt) = tracker_rt.lock() {
                rt.track_bandwidth(*client_id, *network_bytes_received, *network_bytes_send);
            }
        }
    }));

    let mut web_server = BlackTeaWebTransportServer::bind_with_shared_runtime(
        Arc::clone(&runtime),
        &web_bind_addr,
        &cert_path,
        &key_path,
        Arc::clone(&file_transfers),
    )?;
    let file_server = FileTransferServer::bind(
        Arc::clone(&file_transfers),
        &file_bind_addr,
        &cert_path,
        &key_path,
    )?;
    
    let blackteaweb_dist_path = workspace_root.join("BlackTeaWeb").join("dist");
    let web_client_server = WebClientServer::bind(
        &web_client_bind_addr,
        blackteaweb_dist_path,
        &cert_path,
        &key_path,
    )?;
    let desktop_shared_secrets = Arc::new(Mutex::new(HashMap::<u64, Vec<u8>>::new()));
    let (lifecycle_tx, lifecycle_rx) = std::sync::mpsc::channel();
    runtime.lock().unwrap().lifecycle_tx = Some(lifecycle_tx.clone());

    // Start initial servers
    {
        let rt = runtime.lock().unwrap();
        let servers = rt.db.load_virtual_servers().unwrap_or_default();
        for server in servers.values() {
            let _ = lifecycle_tx.send(blackteaspeak_server::runtime::LifecycleAction::StartVirtualServer {
                server_id: server.id(),
                port: server.port(),
            });
        }
    }

    let lifecycle_runtime = Arc::clone(&runtime);
    let lifecycle_secrets = Arc::clone(&desktop_shared_secrets);
    thread::spawn(move || {
        let mut running_udp_servers = HashMap::<u32, Arc<std::sync::atomic::AtomicBool>>::new();
        let mut running_tcp_servers = HashMap::<u32, Arc<std::sync::atomic::AtomicBool>>::new();
        
        while let Ok(action) = lifecycle_rx.recv() {
            match action {
                blackteaspeak_server::runtime::LifecycleAction::StartVirtualServer { server_id, port } => {
                    let bind_addr = format!("0.0.0.0:{}", port);
                    
                    // Start UDP
                    if let Ok(server) = DesktopTransportServer::bind_with_shared_runtime(
                        server_id,
                        Arc::clone(&lifecycle_runtime),
                        &bind_addr,
                        Arc::clone(&lifecycle_secrets)
                    ) {
                        println!("virtual server {} desktop transport listening on udp {}", server_id, port);
                        let should_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                        running_udp_servers.insert(server_id, Arc::clone(&should_stop));
                        thread::spawn(move || {
                            if let Err(e) = server.run(should_stop) {
                                eprintln!("virtual server {} udp terminated: {}", server_id, e);
                            }
                        });
                    }
                    
                    // Start TCP
                    if let Ok(server) = blackteaspeak_server::desktop_tcp_transport::DesktopTcpTransportServer::bind_with_shared_runtime(
                        server_id,
                        Arc::clone(&lifecycle_runtime),
                        &bind_addr,
                        Arc::clone(&lifecycle_secrets)
                    ) {
                        println!("virtual server {} desktop tcp transport listening on tcp {}", server_id, port);
                        let should_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
                        running_tcp_servers.insert(server_id, Arc::clone(&should_stop));
                        thread::spawn(move || {
                            if let Err(e) = server.run(should_stop) {
                                eprintln!("virtual server {} tcp terminated: {}", server_id, e);
                            }
                        });
                    }
                }
                blackteaspeak_server::runtime::LifecycleAction::StopVirtualServer { server_id } => {
                    if let Some(should_stop) = running_udp_servers.remove(&server_id) {
                        should_stop.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    if let Some(should_stop) = running_tcp_servers.remove(&server_id) {
                        should_stop.store(true, std::sync::atomic::Ordering::SeqCst);
                    }
                    println!("virtual server {} stopped", server_id);
                }
            }
        }
    });

    let web_local_addr = web_server.local_addr()?;
    let web_client_local_addr = web_client_server.local_addr()?;
    let file_local_addr = file_server.local_addr()?;
    


    println!("blackteaweb transport listening on {}", web_local_addr);
    println!("web client listening on https://{}", web_client_local_addr);
    println!("file transfer listening on {}", file_local_addr);

    println!("certificate={}", cert_path.display());
    println!("private_key={}", key_path.display());
    if let Some(responder) = desktop_responder.as_ref() {
        println!("desktop responder={}", responder.describe());
    }
    println!("mode=serve-all (shared runtime + live query/web bridge)");
    println!("press Ctrl+C to stop");

    let housekeeping_runtime = Arc::clone(&runtime);
    thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(10));
            if let Ok(mut rt) = housekeeping_runtime.lock() {
                rt.run_housekeeping();
                rt.persist_state_best_effort();
            }
        }
    });

    let ssh_runtime = Arc::clone(&runtime);
    let ssh_bind_addr = resolve_bind_address_for(args, &["--query-bind"], "0.0.0.0:10022");
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = russh::server::Config {
                auth_rejection_time: std::time::Duration::from_secs(1),
                keys: vec![russh::keys::PrivateKey::from_openssh(b"-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABBMysm/AA\nww+NKFkm+obdbCAAAAGAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAINYhFILjCe6TjZA2\nllBEHEe6j0lpuPvyfzjyUwTXWFm+AAAAoDW/2jvvNIrDLkmgR3m91yqRQRoH5n/G1dU1F7\npKU4NPsm6DDvwVz8R/naBMR6xhrbBpQDT+HEpeZukq4e0CBfnF9P6/eEsc7E3gCoEgzIP7\nLBniw65nd6szWsj6AUDCiVeaXIeswML1gvpbkdhBk8jMsZotNRcgEnQe4No36Hhe2avFgy\nx9UivfDLNFzIHOhH9jI8CyGPU2iAbkJhzDFcw=\n-----END OPENSSH PRIVATE KEY-----\n").unwrap()],
                ..Default::default()
            };
            let config = std::sync::Arc::new(config);
            let mut server = SshQueryServer { runtime: ssh_runtime };
            println!("ssh query transport listening on {}", ssh_bind_addr);
            if let Ok(listener) = tokio::net::TcpListener::bind(&ssh_bind_addr).await {
                while let Ok((stream, addr)) = listener.accept().await {
                    use russh::server::Server;
                    let handler = server.new_client(Some(addr));
                    let cfg = std::sync::Arc::clone(&config);
                    tokio::spawn(async move {
                        let _ = russh::server::run_stream(cfg, stream, handler).await;
                    });
                }
            } else {
                eprintln!("failed to bind ssh query transport on {}", ssh_bind_addr);
            }
        });
    });
    thread::spawn(move || {
        if let Err(error) = file_server.run() {
            eprintln!("file transfer terminated: {error:#}");
        }
    });
    thread::spawn(move || {
        if let Err(error) = web_client_server.run() {
            eprintln!("web client terminated: {error:#}");
        }
    });
    web_server.run()
}

fn run_serve_web(workspace_root: &std::path::Path, args: &[String]) -> Result<()> {
    let bind_addr = resolve_bind_address(args, DEFAULT_TEAWEB_BIND);
    let web_client_bind_addr = resolve_bind_address_for(args, &["--web-client-bind"], DEFAULT_WEB_CLIENT_BIND);
    let file_bind_addr = resolve_bind_address_for(args, &["--file-bind"], DEFAULT_FILE_TRANSFER_BIND);
    let desktop_bind_addr = resolve_desktop_bind_addr(args, &bind_addr);
    let desktop_max_bytes = resolve_usize_option(
        args,
        "--desktop-max-bytes",
        DEFAULT_DESKTOP_COMPAT_MAX_BYTES,
    );
    let desktop_responder = desktop_bind_addr
        .as_ref()
        .map(|_| resolve_desktop_responder(args))
        .transpose()?;
    let (cert_path, key_path) = resolve_blackteaweb_tls_paths(workspace_root, args);
    let runtime = Arc::new(Mutex::new(create_baseline_runtime(workspace_root)?));
    let file_transfers = Arc::new(blackteaspeak_server::file_transfer::FileTransferRegistry::new(workspace_root));
    let (music_download_tx, music_download_rx) = std::sync::mpsc::channel();
    {
        let mut rt = runtime.lock().unwrap();
        rt.set_file_transfer_registry(Arc::clone(&file_transfers));
        rt.music_download_tx = Some(music_download_tx);
    }
    let dl_runtime = Arc::clone(&runtime);
    let dl_file_transfers = Arc::clone(&file_transfers);
    thread::spawn(move || {
        for (bot_id, song_id, url) in music_download_rx {
            let output_path = match dl_file_transfers.music_download_path(&format!("{song_id}.mp3")) {
                Ok(path) => path,
                Err(error) => {
                    eprintln!("failed to resolve music download path for {song_id}: {error}");
                    continue;
                }
            };
            if blackteaspeak_server::runtime::BaselineRuntime::download_youtube_track(&url, &output_path) {
                if let Ok(mut rt) = dl_runtime.lock() {
                    rt.update_downloaded_music_track(bot_id, song_id, "ffmpeg", &format!("/music/{song_id}.mp3"));
                }
            }
        }
    });

    let tracker_rt = Arc::clone(&runtime);
    file_transfers.add_notifier(Arc::new(move |event| {
        use blackteaspeak_server::file_transfer::FileTransferEvent;
        if let FileTransferEvent::Progress { client_id, network_bytes_received, network_bytes_send, .. } = event {
            if let Ok(mut rt) = tracker_rt.lock() {
                rt.track_bandwidth(*client_id, *network_bytes_received, *network_bytes_send);
            }
        }
    }));
    let server = BlackTeaWebTransportServer::bind_with_shared_runtime(
        Arc::clone(&runtime),
        &bind_addr,
        &cert_path,
        &key_path,
        Arc::clone(&file_transfers),
    )?;
    let file_server = FileTransferServer::bind(
        Arc::clone(&file_transfers),
        &file_bind_addr,
        &cert_path,
        &key_path,
    )?;
    
    let blackteaweb_dist_path = workspace_root.join("BlackTeaWeb").join("dist");
    let web_client_server = WebClientServer::bind(
        &web_client_bind_addr,
        blackteaweb_dist_path,
        &cert_path,
        &key_path,
    )?;
    let desktop_shared_secrets = Arc::new(Mutex::new(HashMap::<u64, Vec<u8>>::new()));
    let desktop_server = desktop_bind_addr
        .as_ref()
        .map(|bind_addr| {
            DesktopTransportServer::bind_with_shared_runtime(1, Arc::clone(&runtime), bind_addr, Arc::clone(&desktop_shared_secrets))
        })
        .transpose()?;
    let local_addr = server.local_addr()?;
    let web_client_local_addr = web_client_server.local_addr()?;
    let file_local_addr = file_server.local_addr()?;
    let desktop_local_addr = desktop_server
        .as_ref()
        .map(DesktopTransportServer::local_addr)
        .transpose()?;

    println!("blackteaweb transport listening on {}", local_addr);
    println!("web client listening on https://{}", web_client_local_addr);
    println!("file transfer listening on {}", file_local_addr);
    if let Some(local_addr) = desktop_local_addr {
        println!("desktop transport listening on udp {}", local_addr);
    }
    println!("certificate={}", cert_path.display());
    println!("private_key={}", key_path.display());
    if let Some(responder) = desktop_responder.as_ref() {
        println!("desktop responder={}", responder.describe());
    }
    println!("press Ctrl+C to stop");

    thread::spawn(move || {
        if let Err(error) = file_server.run() {
            eprintln!("file transfer terminated: {error:#}");
        }
    });
    thread::spawn(move || {
        if let Err(error) = web_client_server.run() {
            eprintln!("web client terminated: {error:#}");
        }
    });
    if let Some(desktop_server) = desktop_server {
        thread::spawn(move || {
            let should_stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
            if let Err(error) = desktop_server.run(should_stop) {
                eprintln!("desktop transport terminated: {error:#}");
            }
        });
    }
    server.run()
}

fn run_serve_desktop(args: &[String]) -> Result<()> {
    let bind_addr = resolve_desktop_bind_addr(args, DEFAULT_TEAWEB_BIND)
        .unwrap_or_else(|| String::from(DEFAULT_TEAWEB_BIND));
    let max_bytes = resolve_usize_option(
        args,
        "--desktop-max-bytes",
        DEFAULT_DESKTOP_COMPAT_MAX_BYTES,
    );
    let responder = resolve_desktop_responder(args)?;
    let server = DesktopCaptureServer::bind_with_responder(&bind_addr, Some(max_bytes), responder.clone())?;
    let local_addr = server.local_addr()?;

    println!("desktop transport listening on udp {}", local_addr);
    println!("desktop responder={}", responder.describe());
    println!("press Ctrl+C to stop");

    server.run()
}

fn run_capture_desktop(args: &[String]) -> Result<()> {
    let bind_addr = resolve_bind_address(args, DEFAULT_DESKTOP_CAPTURE_BIND);
    let max_bytes = resolve_usize_option(args, "--max-bytes", 256);
    let responder = resolve_desktop_responder_from_default(args, DesktopResponder::default())?;
    let server = DesktopCaptureServer::bind_with_responder(&bind_addr, Some(max_bytes), responder.clone())?;
    let local_addr = server.local_addr()?;

    println!("desktop capture listening on {}", local_addr);
    println!("max_bytes={}", max_bytes);
    println!("responder={}", responder.describe());
    println!("press Ctrl+C to stop");

    server.run()
}

fn run_generate_web_cert(workspace_root: &std::path::Path, args: &[String]) -> Result<()> {
    let (cert_path, key_path) = resolve_generated_blackteaweb_tls_paths(workspace_root, args);
    generate_localhost_tls_assets(&cert_path, &key_path)?;

    println!(
        "generated BlackTeaWeb localhost certificate={}",
        cert_path.display()
    );
    println!(
        "generated BlackTeaWeb localhost private_key={}",
        key_path.display()
    );
    println!("to trust the certificate for the current Windows user, run:");
    println!(
        "  certutil -user -addstore Root \"{}\"",
        cert_path.display()
    );

    Ok(())
}

fn resolve_blackteaweb_tls_paths(
    workspace_root: &std::path::Path,
    args: &[String],
) -> (PathBuf, PathBuf) {
    let explicit_cert = resolve_path_option(args, "--cert");
    let explicit_key = resolve_path_option(args, "--key");

    let (generated_cert, generated_key) = resolve_generated_blackteaweb_tls_paths(workspace_root, &[]);
    let (default_cert, default_key) = if generated_cert.is_file() && generated_key.is_file() {
        (generated_cert, generated_key)
    } else {
        (
            workspace_root
                .join("BlackTeaSpeak-1.5.6")
                .join("certs")
                .join("default_certificate.pem"),
            workspace_root
                .join("BlackTeaSpeak-1.5.6")
                .join("certs")
                .join("default_privatekey.pem"),
        )
    };

    (
        explicit_cert.unwrap_or(default_cert),
        explicit_key.unwrap_or(default_key),
    )
}

fn resolve_generated_blackteaweb_tls_paths(
    workspace_root: &std::path::Path,
    args: &[String],
) -> (PathBuf, PathBuf) {
    let default_cert = workspace_root
        .join("data")
        .join("tls")
        .join("blackteaweb-localhost-cert.pem");
    let default_key = workspace_root
        .join("data")
        .join("tls")
        .join("blackteaweb-localhost-key.pem");

    (
        resolve_path_option(args, "--cert").unwrap_or(default_cert),
        resolve_path_option(args, "--key").unwrap_or(default_key),
    )
}

fn resolve_bind_address(args: &[String], default_bind: &str) -> String {
    resolve_bind_address_for(args, &["--bind"], default_bind)
}

fn resolve_bind_address_for(args: &[String], option_names: &[&str], default_bind: &str) -> String {
    let mut cursor = 0;
    while cursor < args.len() {
        let current = args[cursor].as_str();
        if option_names.contains(&current) && cursor + 1 < args.len() {
            return args[cursor + 1].clone();
        }
        if option_names.len() == 1 && option_names[0] == "--bind" && !current.starts_with('-') {
            return current.to_string();
        }
        cursor += 1;
    }
    String::from(default_bind)
}

fn resolve_path_option(args: &[String], option_name: &str) -> Option<PathBuf> {
    let mut cursor = 0;
    while cursor + 1 < args.len() {
        if args[cursor] == option_name {
            return Some(PathBuf::from(&args[cursor + 1]));
        }
        cursor += 1;
    }
    None
}

fn has_flag(args: &[String], option_name: &str) -> bool {
    args.iter().any(|arg| arg == option_name)
}

fn resolve_desktop_bind_addr(args: &[String], default_bind: &str) -> Option<String> {
    if has_flag(args, "--no-desktop") {
        return None;
    }
    Some(resolve_bind_address_for(args, &["--desktop-bind", "--bind"], default_bind))
}

fn resolve_desktop_responder(args: &[String]) -> Result<DesktopResponder> {
    resolve_desktop_responder_from_default(args, DesktopResponder::native_compat_default())
}

fn resolve_desktop_responder_from_default(
    args: &[String],
    default_responder: DesktopResponder,
) -> Result<DesktopResponder> {
    let mut responder = default_responder;

    if let Some(value) = resolve_string_option(args, "--reply-ts3init1") {
        responder = responder.with_ts3init1_action(DesktopResponseAction::from_cli_value(&value)?);
    }
    if let Some(value) = resolve_string_option(args, "--reply-bootstrap185") {
        responder = responder.with_bootstrap185_action(DesktopResponseAction::from_cli_value(&value)?);
    }
    if let Some(value) = resolve_string_option(args, "--reply-get-cookie") {
        responder = responder.with_get_cookie_action(Some(DesktopResponseAction::from_cli_value(&value)?));
    }
    if let Some(value) = resolve_string_option(args, "--reply-get-puzzle") {
        responder = responder.with_get_puzzle_action(Some(DesktopResponseAction::from_cli_value(&value)?));
    }
    if let Some(value) = resolve_string_option(args, "--reply-followup39") {
        responder = responder.with_followup39_action(Some(DesktopResponseAction::from_cli_value(&value)?));
    }
    if let Some(value) = resolve_string_option(args, "--reply-post-command4") {
        responder = responder.with_post_command4_action(Some(DesktopResponseAction::from_cli_value(&value)?));
    }

    Ok(responder)
}

fn resolve_usize_option(args: &[String], option_name: &str, default_value: usize) -> usize {
    let mut cursor = 0;
    while cursor + 1 < args.len() {
        if args[cursor] == option_name {
            return args[cursor + 1].parse::<usize>().unwrap_or(default_value);
        }
        cursor += 1;
    }
    default_value
}

fn resolve_string_option(args: &[String], option_name: &str) -> Option<String> {
    let mut cursor = 0;
    while cursor + 1 < args.len() {
        if args[cursor] == option_name {
            return Some(args[cursor + 1].clone());
        }
        cursor += 1;
    }
    None
}

fn print_usage() {
    println!("Usage:");
    println!("  blackteaspeak_server info");
    println!("  blackteaspeak_server repl");
    println!("  blackteaspeak_server serve [--bind <addr>]");
    println!(
        "  blackteaspeak_server serve-all [--query-bind <addr>] [--web-bind <addr>] [--file-bind <addr>] [--desktop-bind <addr>] [--desktop-max-bytes <count>] [--no-desktop] [--cert <path>] [--key <path>] [--reply-... <mode>]"
    );
    println!("  blackteaspeak_server serve-web [--bind <addr>] [--file-bind <addr>] [--desktop-bind <addr>] [--desktop-max-bytes <count>] [--no-desktop] [--cert <path>] [--key <path>] [--reply-... <mode>]");
    println!("  blackteaspeak_server serve-desktop [--bind <addr>] [--desktop-bind <addr>] [--desktop-max-bytes <count>] [--reply-... <mode>]");
    println!("  blackteaspeak_server capture-desktop [--bind <addr>] [--max-bytes <count>] [--reply-ts3init1 <ignore|echo|reset|set-cookie|set-cookie-compact|set-cookie-dual|set-cookie-error[:<u32|0xHEX|name>]|set-cookie-error-dual[:<u32|0xHEX|name>]|set-puzzle|set-puzzle-legacy|followup39-reply[:bootstrap-overlap23|bootstrap-blocka-blockb7|bootstrap-prefix8-overlap23|segment-prefix8-overlap23]|post-command4-replay[:packet-index-zero-bootstrap-blockab23|ack15-repeat-1302|ack15-1306-only|ack15-seq1-burst|ack15-seq12-burst|ack15-full-burst|ack15-seq12-then-seq3-50ms|ack15-seq12-then-seq3-250ms|bootstrap-head11|bootstrap-head35|bootstrap-head11-xor|bootstrap-head35-xor|bootstrap-head11-add|bootstrap-head35-add|bootstrap-tag-head11-add|alpha-prefix8|alpha-payload-head|bootstrap-tag|bootstrap-blockb16|bootstrap-blockab23|payload-byte:<offset>:<zero|add1|sub1|xor02>|payload-tail1-add1|payload-tail1-sub1|payload-tail1-xor02|payload-tail1-zero|payload-tail2-zero|payload-tail4-zero|payload-tail8-zero|payload-tail16-zero|payload-mid32-zero|payload-tail32-zero|payload-tail64-zero|bootstrap-overlap24|bootstrap-correlated]|hex:...>] [--reply-get-cookie <ignore|echo|reset|set-cookie|set-cookie-compact|set-cookie-dual|set-cookie-error[:<u32|0xHEX|name>]|set-cookie-error-dual[:<u32|0xHEX|name>]|set-puzzle|set-puzzle-legacy|followup39-reply[:bootstrap-overlap23|bootstrap-blocka-blockb7|bootstrap-prefix8-overlap23|segment-prefix8-overlap23]|post-command4-replay[:packet-index-zero-bootstrap-blockab23|ack15-repeat-1302|ack15-1306-only|ack15-seq1-burst|ack15-seq12-burst|ack15-full-burst|ack15-seq12-then-seq3-50ms|ack15-seq12-then-seq3-250ms|bootstrap-head11|bootstrap-head35|bootstrap-head11-xor|bootstrap-head35-xor|bootstrap-head11-add|bootstrap-head35-add|bootstrap-tag-head11-add|alpha-prefix8|alpha-payload-head|bootstrap-tag|bootstrap-blockb16|bootstrap-blockab23|payload-byte:<offset>:<zero|add1|sub1|xor02>|payload-tail1-add1|payload-tail1-sub1|payload-tail1-xor02|payload-tail1-zero|payload-tail2-zero|payload-tail4-zero|payload-tail8-zero|payload-tail16-zero|payload-mid32-zero|payload-tail32-zero|payload-tail64-zero|bootstrap-overlap24|bootstrap-correlated]|hex:...>] [--reply-get-puzzle <ignore|echo|reset|set-cookie|set-cookie-compact|set-cookie-dual|set-cookie-error[:<u32|0xHEX|name>]|set-cookie-error-dual[:<u32|0xHEX|name>]|set-puzzle|set-puzzle-legacy|followup39-reply[:bootstrap-overlap23|bootstrap-blocka-blockb7|bootstrap-prefix8-overlap23|segment-prefix8-overlap23]|post-command4-replay[:packet-index-zero-bootstrap-blockab23|ack15-repeat-1302|ack15-1306-only|ack15-seq1-burst|ack15-seq12-burst|ack15-full-burst|ack15-seq12-then-seq3-50ms|ack15-seq12-then-seq3-250ms|bootstrap-head11|bootstrap-head35|bootstrap-head11-xor|bootstrap-head35-xor|bootstrap-head11-add|bootstrap-head35-add|bootstrap-tag-head11-add|alpha-prefix8|alpha-payload-head|bootstrap-tag|bootstrap-blockb16|bootstrap-blockab23|payload-byte:<offset>:<zero|add1|sub1|xor02>|payload-tail1-add1|payload-tail1-sub1|payload-tail1-xor02|payload-tail1-zero|payload-tail2-zero|payload-tail4-zero|payload-tail8-zero|payload-tail16-zero|payload-mid32-zero|payload-tail32-zero|payload-tail64-zero|bootstrap-overlap24|bootstrap-correlated]|hex:...>] [--reply-bootstrap185 <ignore|echo|reset|set-cookie|set-cookie-compact|set-cookie-dual|set-cookie-error[:<u32|0xHEX|name>]|set-cookie-error-dual[:<u32|0xHEX|name>]|set-puzzle|set-puzzle-legacy|followup39-reply[:bootstrap-overlap23|bootstrap-blocka-blockb7|bootstrap-prefix8-overlap23|segment-prefix8-overlap23]|post-command4-replay[:packet-index-zero-bootstrap-blockab23|ack15-repeat-1302|ack15-1306-only|ack15-seq1-burst|ack15-seq12-burst|ack15-full-burst|ack15-seq12-then-seq3-50ms|ack15-seq12-then-seq3-250ms|bootstrap-head11|bootstrap-head35|bootstrap-head11-xor|bootstrap-head35-xor|bootstrap-head11-add|bootstrap-head35-add|bootstrap-tag-head11-add|alpha-prefix8|alpha-payload-head|bootstrap-tag|bootstrap-blockb16|bootstrap-blockab23|payload-byte:<offset>:<zero|add1|sub1|xor02>|payload-tail1-add1|payload-tail1-sub1|payload-tail1-xor02|payload-tail1-zero|payload-tail2-zero|payload-tail4-zero|payload-tail8-zero|payload-tail16-zero|payload-mid32-zero|payload-tail32-zero|payload-tail64-zero|bootstrap-overlap24|bootstrap-correlated]|hex:...>] [--reply-followup39 <ignore|echo|reset|set-cookie|set-cookie-compact|set-cookie-dual|set-cookie-error[:<u32|0xHEX|name>]|set-cookie-error-dual[:<u32|0xHEX|name>]|set-puzzle|set-puzzle-legacy|followup39-reply[:bootstrap-overlap23|bootstrap-blocka-blockb7|bootstrap-prefix8-overlap23|segment-prefix8-overlap23]|post-command4-replay[:packet-index-zero-bootstrap-blockab23|ack15-repeat-1302|ack15-1306-only|ack15-seq1-burst|ack15-seq12-burst|ack15-full-burst|ack15-seq12-then-seq3-50ms|ack15-seq12-then-seq3-250ms|bootstrap-head11|bootstrap-head35|bootstrap-head11-xor|bootstrap-head35-xor|bootstrap-head11-add|bootstrap-head35-add|bootstrap-tag-head11-add|alpha-prefix8|alpha-payload-head|bootstrap-tag|bootstrap-blockb16|bootstrap-blockab23|payload-byte:<offset>:<zero|add1|sub1|xor02>|payload-tail1-add1|payload-tail1-sub1|payload-tail1-xor02|payload-tail1-zero|payload-tail2-zero|payload-tail4-zero|payload-tail8-zero|payload-tail16-zero|payload-mid32-zero|payload-tail32-zero|payload-tail64-zero|bootstrap-overlap24|bootstrap-correlated]|hex:...>] [--reply-post-command4 <ignore|echo|reset|set-cookie|set-cookie-compact|set-cookie-dual|set-cookie-error[:<u32|0xHEX|name>]|set-cookie-error-dual[:<u32|0xHEX|name>]|set-puzzle|set-puzzle-legacy|followup39-reply[:bootstrap-overlap23|bootstrap-blocka-blockb7|bootstrap-prefix8-overlap23|segment-prefix8-overlap23]|post-command4-replay[:packet-index-zero-bootstrap-blockab23|ack15-repeat-1302|ack15-1306-only|ack15-seq1-burst|ack15-seq12-burst|ack15-full-burst|ack15-seq12-then-seq3-50ms|ack15-seq12-then-seq3-250ms|bootstrap-head11|bootstrap-head35|bootstrap-head11-xor|bootstrap-head35-xor|bootstrap-head11-add|bootstrap-head35-add|bootstrap-tag-head11-add|alpha-prefix8|alpha-payload-head|bootstrap-tag|bootstrap-blockb16|bootstrap-blockab23|payload-byte:<offset>:<zero|add1|sub1|xor02>|payload-tail1-add1|payload-tail1-sub1|payload-tail1-xor02|payload-tail1-zero|payload-tail2-zero|payload-tail4-zero|payload-tail8-zero|payload-tail16-zero|payload-mid32-zero|payload-tail32-zero|payload-tail64-zero|bootstrap-overlap24|bootstrap-correlated]|hex:...>]");
    println!("  blackteaspeak_server generate-web-cert [--cert <path>] [--key <path>]");
    println!("  blackteaspeak_server exec <query line>");
    println!("  blackteaspeak_server <query line>");
}
