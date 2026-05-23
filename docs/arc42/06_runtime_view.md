# 6. Runtime View

## 6.1 Server Startup Sequence
```mermaid
sequenceDiagram
    participant Main
    participant DB
    participant Runtime
    participant WebTransport
    participant LegacyUDP as Legacy UDP
    participant SSH

    Main->>DB: Check/Init Schema
    Main->>Runtime: Create BaselineRuntime
    Runtime->>DB: Load Configurations (Virtual Servers)
    Runtime->>Runtime: Build Event Pipeline (Broadcast Channels)
    Main->>WebTransport: bind(9987)
    Main->>LegacyUDP: bind(9987)
    Main->>SSH: bind(10022)
    Note over Main: Server is ready to accept connections
```

## 6.2 Permission Evaluation Flow
Whenever a client executes a command:
1. `ssh_query` / `desktop_transport` / `web_transport` converts the payload into `CommandRequest`.
2. `BaselineRuntime::dispatch()` matches the command.
3. Call `BaselineRuntime::has_permission(client_id, PERM_ID)`.
   - Checks Client-Specific Permissions.
   - If inherited: Checks Channel Group Permissions.
   - If inherited: Checks Server Group Permissions.
4. Returns boolean. If true, process command. If false, return `QueryResponse::error(insufficient_client_permissions)`.
