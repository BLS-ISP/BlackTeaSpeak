import re

with open('src/main.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# For run_serve
serve_old = r'''    let rt = tokio::runtime::Runtime::new()\?\;
    rt\.block_on\(async \{
        let mut config = russh::server::Config \{
            auth_rejection_time: std::time::Duration::from_secs\(1\),
            keys: vec!\[ssh_key::PrivateKey::random\(&mut rand_core::OsRng, ssh_key::Algorithm::Ed25519\)\.unwrap\(\)\],
            \.\.Default::default\(\)
        \};
        let server = SshQueryServer \{ runtime \};
        russh::server::run\(std::sync::Arc::clone\(&config\), bind_addr, server\)\.await
    \}\)\?;'''

serve_new = r'''    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let config = russh::server::Config {
            auth_rejection_time: std::time::Duration::from_secs(1),
            keys: vec![ssh_key::PrivateKey::random(&mut rand_core::OsRng, ssh_key::Algorithm::Ed25519).unwrap()],
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
    })?;'''
content = re.sub(serve_old, serve_new, content)

# For run_serve_all
thread_old = r'''    let ssh_runtime = Arc::clone\(&runtime\);
    let ssh_bind_addr = resolve_bind_address_for\(args, &\["--query-bind"\], "0\.0\.0\.0:10022"\);
    thread::spawn\(move \|\| \{
        let rt = tokio::runtime::Runtime::new\(\)\.unwrap\(\);
        rt\.block_on\(async \{
            let config = russh::server::Config \{
                auth_rejection_time: std::time::Duration::from_secs\(1\),
                keys: vec!\[ssh_key::PrivateKey::random\(&mut rand_core::OsRng, ssh_key::Algorithm::Ed25519\)\.unwrap\(\)\],
                \.\.Default::default\(\)
            \};
            let server = SshQueryServer \{ runtime: ssh_runtime \};
            println!\("ssh query transport listening on \{\}", ssh_bind_addr\);
            if let Err\(e\) = run\(Arc::new\(config\), ssh_bind_addr, server\)\.await \{
                eprintln!\("ssh server error: \{e\}"\);
            \}
        \}\);
    \}\);'''

thread_new = r'''    let ssh_runtime = Arc::clone(&runtime);
    let ssh_bind_addr = resolve_bind_address_for(args, &["--query-bind"], "0.0.0.0:10022");
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = russh::server::Config {
                auth_rejection_time: std::time::Duration::from_secs(1),
                keys: vec![ssh_key::PrivateKey::random(&mut rand_core::OsRng, ssh_key::Algorithm::Ed25519).unwrap()],
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
    });'''
content = re.sub(thread_old, thread_new, content)

with open('src/main.rs', 'w', encoding='utf-8') as f:
    f.write(content)
