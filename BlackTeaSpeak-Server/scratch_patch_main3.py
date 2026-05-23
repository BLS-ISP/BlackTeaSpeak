import re

with open('src/main.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Replace run_serve
serve_old = r'''fn run_serve\(workspace_root: &std::path::Path, args: &\[String\]\) -> Result<\(\)> \{
    let bind_addr = resolve_bind_address\(args, "0\.0\.0\.0:10022"\);
.*?
    \}\)\?;
    Ok\(\(\)\)
\}'''

hardcoded_key = 'russh::keys::PrivateKey::from_openssh(b"-----BEGIN OPENSSH PRIVATE KEY-----\\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW\\nMjU1MTkAAAAgxGsmgW4Vz7VpB+1p8Wl8R/q1Fv5vFp8Wl8R/q1Fv5vEAAACkhv8A\\nZob/AGYAAAALc3NoLWVkMjU1MTkAAAAgxGsmgW4Vz7VpB+1p8Wl8R/q1Fv5vFp8Wl8R/\\nq1Fv5vEAAAAgJ2aRbhXPtWkH7WnxaXxH+rUW/m8WnxaXxH+rUW/m8cTEayaBbhXPtWkH\\n7WnxaXxH+rUW/m8WnxaXxH+rUW/m8QAAAANhYmMBAgMEBQYH\\n-----END OPENSSH PRIVATE KEY-----\\n").unwrap()'

serve_new = f'''fn run_serve(workspace_root: &std::path::Path, args: &[String]) -> Result<()> {{
    let bind_addr = resolve_bind_address(args, "0.0.0.0:10022");
    
    println!("ssh query transport listening on {{}}", bind_addr);
    println!("press Ctrl+C to stop");

    let runtime = Arc::new(Mutex::new(create_baseline_runtime(workspace_root)?));
    
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {{
        let config = russh::server::Config {{
            auth_rejection_time: std::time::Duration::from_secs(1),
            keys: vec![{hardcoded_key}],
            ..Default::default()
        }};
        let config = std::sync::Arc::new(config);
        let mut server = SshQueryServer {{ runtime }};
        let listener = tokio::net::TcpListener::bind(&bind_addr).await.unwrap();
        while let Ok((stream, addr)) = listener.accept().await {{
            use russh::server::Server;
            let handler = server.new_client(Some(addr));
            let cfg = std::sync::Arc::clone(&config);
            tokio::spawn(async move {{
                let _ = russh::server::run_stream(cfg, stream, handler).await;
            }});
        }}
        Ok::<(), anyhow::Error>(())
    }})?;
    Ok(())
}}'''
content = re.sub(serve_old, serve_new, content, flags=re.DOTALL)

# Replace run_serve_all thread
thread_old = r'''    let ssh_runtime = Arc::clone\(&runtime\);
    let ssh_bind_addr = resolve_bind_address_for\(args, &\["--query-bind"\], "0\.0\.0\.0:10022"\);
    thread::spawn\(move \|\| \{
        let rt = tokio::runtime::Runtime::new\(\)\.unwrap\(\);
        rt\.block_on\(async \{.*?\}\);
    \}\);'''

thread_new = f'''    let ssh_runtime = Arc::clone(&runtime);
    let ssh_bind_addr = resolve_bind_address_for(args, &["--query-bind"], "0.0.0.0:10022");
    thread::spawn(move || {{
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {{
            let config = russh::server::Config {{
                auth_rejection_time: std::time::Duration::from_secs(1),
                keys: vec![{hardcoded_key}],
                ..Default::default()
            }};
            let config = std::sync::Arc::new(config);
            let mut server = SshQueryServer {{ runtime: ssh_runtime }};
            println!("ssh query transport listening on {{}}", ssh_bind_addr);
            if let Ok(listener) = tokio::net::TcpListener::bind(&ssh_bind_addr).await {{
                while let Ok((stream, addr)) = listener.accept().await {{
                    use russh::server::Server;
                    let handler = server.new_client(Some(addr));
                    let cfg = std::sync::Arc::clone(&config);
                    tokio::spawn(async move {{
                        let _ = russh::server::run_stream(cfg, stream, handler).await;
                    }});
                }}
            }} else {{
                eprintln!("failed to bind ssh query transport on {{}}", ssh_bind_addr);
            }}
        }});
    }});'''
content = re.sub(thread_old, thread_new, content, flags=re.DOTALL)

with open('src/main.rs', 'w', encoding='utf-8') as f:
    f.write(content)
