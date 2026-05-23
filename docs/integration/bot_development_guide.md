# Bot & Plugin Developer Guide

BlackTeaSpeak fully supports legacy ServerQuery bots, but mandates modern secure connections and encourages the custom BTEA protocol for media bots.

## 1. Administrative Bots (Text, Roles, Kicks)
If your bot just needs to send text messages, move clients, or assign server groups, you should use the **SSH Query Transport**.

**Connecting via SSH**:
Connect to `10022` using `serveradmin` as the username and your generated query password. 

```bash
ssh serveradmin@127.0.0.1 -p 10022
```
Once connected, issue `help` to list commands. Issue `servernotifyregister event=server` to subscribe to event feeds.

## 2. Music and Media Bots
For bots streaming audio (e.g., TS3AudioBot alternatives), legacy voice connection involves extreme complexity (cryptography, puzzle solving, fake hardware IDs).

**Use the BTEA Protocol instead!**
BTEA skips puzzle solving and provides an X25519 ECDH handshake taking milliseconds. After handshake, you can blast native Opus packets via UDP and the server distributes it to both Desktop and Web clients.
(See `docs/btea_protocol.md` for specs).
