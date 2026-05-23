import re

filepath = r"d:\projekt\BlackTeaSpeak\BlackTeaSpeak-Server\src\web_transport.rs"
with open(filepath, "r", encoding="utf-8") as f:
    content = f.read()

# Add wtransport_session field
content = re.sub(
    r"struct RegisteredBlackTeaWebSession \{\n\s*presence: BlackTeaWebPresence,\n\s*client_database_id: u64,\n\s*visible_channel_ids: BTreeSet<u32>,\n\s*pending_frames: SharedPendingFrames,\n\}",
    r"struct RegisteredBlackTeaWebSession {\n    presence: BlackTeaWebPresence,\n    client_database_id: u64,\n    visible_channel_ids: BTreeSet<u32>,\n    pending_frames: SharedPendingFrames,\n    pub wtransport_session: Option<wtransport::Connection>,\n}",
    content,
    count=1
)

# Modify register_or_update_session to preserve wtransport_session
old_func = """fn register_or_update_session(
    sessions: &SharedBlackTeaWebSessions,
    presence: BlackTeaWebPresence,
    client_database_id: u64,
    visible_channel_ids: BTreeSet<u32>,
    pending_frames: SharedPendingFrames,
) -> Result<()> {
    sessions
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?
        .insert(
            presence.client_id,
            RegisteredBlackTeaWebSession {
                presence,
                client_database_id,
                visible_channel_ids,
                pending_frames,
            },
        );
    Ok(())
}"""

new_func = """fn register_or_update_session(
    sessions: &SharedBlackTeaWebSessions,
    presence: BlackTeaWebPresence,
    client_database_id: u64,
    visible_channel_ids: BTreeSet<u32>,
    pending_frames: SharedPendingFrames,
) -> Result<()> {
    let mut lock = sessions
        .lock()
        .map_err(|_| io::Error::other("BlackTeaWeb session registry lock poisoned"))?;
    
    let existing_wtransport = lock.get(&presence.client_id).and_then(|s| s.wtransport_session.clone());
    
    lock.insert(
        presence.client_id,
        RegisteredBlackTeaWebSession {
            presence,
            client_database_id,
            visible_channel_ids,
            pending_frames,
            wtransport_session: existing_wtransport,
        },
    );
    Ok(())
}"""

content = content.replace(old_func, new_func)

# Also need to assign wtransport_session in handle_wtransport_client
inject_wtransport = """
            if let Some(query_session) = handler.web_query_session() {
                if let Ok(mut lock) = sessions.lock() {
                    if let Some(registered) = lock.get_mut(&query_session.client_id) {
                        registered.wtransport_session = Some(wtransport_session.clone());
                    }
                }
            }
"""

content = content.replace(
    "                handler.apply_server_command(&command);",
    "                handler.apply_server_command(&command);\n" + inject_wtransport
)

with open(filepath, "w", encoding="utf-8") as f:
    f.write(content)
print("done")
