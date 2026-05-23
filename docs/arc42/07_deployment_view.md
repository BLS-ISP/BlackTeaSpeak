# 7. Deployment View

BlackTeaSpeak runs as a single static binary. It does not require a complex multi-container setup unless scaling out media routing is strictly required.

```mermaid
flowchart TD
    subgraph HostMachine [Host Machine]
        Bin[blackteaspeak_server.exe]
        DB[(blackteaspeak.db)]
        TLS[(TLS Certificates)]
        Avatars[(File Transfer Cache)]
    end

    Bin --> DB
    Bin --> TLS
    Bin --> Avatars
```

**Required Ports**:
- `9987/UDP`: Legacy Voice and BTEA Media Protocol.
- `9987/TCP`: Fallback TCP connections.
- `8080/TCP`: HTTP BlackTeaWeb Server Client.
- `10022/TCP`: SSH ServerQuery Interface.
- `30303/TCP`: Avatar & Icon Transfers.
