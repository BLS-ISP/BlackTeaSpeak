import re

with open('src/main.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Replace QueryTransportServer imports
content = re.sub(r'use blackteaspeak_server::transport::\{DEFAULT_QUERY_BIND, QueryTransportServer\};\n', 
                 'use blackteaspeak_server::ssh_query::SshQueryServer;\n', content)

# Remove print_info lines for query_bind_default
content = re.sub(r'    println!\("query_bind_default=\{DEFAULT_QUERY_BIND\}"\);\n', '', content)

# In run_serve, replace query server logic
serve_old = r'''fn run_serve\(workspace_root: &std::path::Path, args: &\[String\]\) -> Result<\(\)> \{
    let bind_addr = resolve_bind_address\(args, DEFAULT_QUERY_BIND\);
    let server = QueryTransportServer::bind\(workspace_root, &bind_addr\)\?;
    let local_addr = server.local_addr\(\)\?;

    println!\("query transport listening on \{\}", local_addr\);
    println!\("press Ctrl\+C to stop"\);

    server.run\(\)
\}'''

serve_new = r'''fn run_serve(workspace_root: &std::path::Path, args: &[String]) -> Result<()> {
    let bind_addr = resolve_bind_address(args, "0.0.0.0:10022");
    
    println!("ssh query transport listening on {}", bind_addr);
    println!("press Ctrl+C to stop");

    let runtime = Arc::new(Mutex::new(create_baseline_runtime(workspace_root)?));
    
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async {
        let mut config = russh::server::Config {
            connection_timeout: Some(std::time::Duration::from_secs(60)),
            auth_rejection_time: std::time::Duration::from_secs(1),
            keys: vec![ssh_key::PrivateKey::random(&mut rand_core::OsRng, ssh_key::Algorithm::Ed25519).unwrap()],
            ..Default::default()
        };
        let server = SshQueryServer { runtime };
        russh::server::run(std::sync::Arc::new(config), bind_addr, server).await
    })?;
    Ok(())
}'''
content = re.sub(serve_old, serve_new, content)

# In run_serve_all, remove QueryTransportServer setup
content = re.sub(r'    let query_bind_addr =\s*resolve_bind_address_for\(args, &\["--query-bind", "--bind"\], DEFAULT_QUERY_BIND\);\n', '', content)
content = re.sub(r'    let mut query_server =\s*QueryTransportServer::bind_with_shared_runtime\(Arc::clone\(&runtime\), &query_bind_addr\)\?;\n', '', content)
content = re.sub(r'    let query_local_addr = query_server\.local_addr\(\)\?;\n', '', content)
content = re.sub(r'    println!\("query transport listening on \{\}", query_local_addr\);\n', '', content)

# In run_serve_all, spawn SSH server instead of query_server
thread_old = r'''    thread::spawn\(move \|\| \{
        if let Err\(error\) = query_server.run\(\) \{
            eprintln!\("query transport terminated: \{error:#\}"\);
        \}
    \}\);'''

thread_new = r'''    let ssh_runtime = Arc::clone(&runtime);
    let ssh_bind_addr = resolve_bind_address_for(args, &["--query-bind"], "0.0.0.0:10022");
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let config = russh::server::Config {
                connection_timeout: Some(std::time::Duration::from_secs(60)),
                auth_rejection_time: std::time::Duration::from_secs(1),
                keys: vec![ssh_key::PrivateKey::random(&mut rand_core::OsRng, ssh_key::Algorithm::Ed25519).unwrap()],
                ..Default::default()
            };
            let server = SshQueryServer { runtime: ssh_runtime };
            println!("ssh query transport listening on {}", ssh_bind_addr);
            if let Err(e) = russh::server::run(Arc::new(config), ssh_bind_addr, server).await {
                eprintln!("ssh server error: {e}");
            }
        });
    });'''
content = re.sub(thread_old, thread_new, content)

with open('src/main.rs', 'w', encoding='utf-8') as f:
    f.write(content)
