use crate::file_transfer::{FileTransferDirection, FileTransferError};
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::BaselineRuntime;
use crate::runtime::QuerySessionState;
use crate::runtime::*;
use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;

fn map_ft_error(error: FileTransferError) -> QueryResponse {
    match error {
        FileTransferError::NotFound => QueryResponse::error(2051, "file not found"),
        FileTransferError::AlreadyExists => QueryResponse::error(2050, "file already exists"),
        FileTransferError::InvalidPath => QueryResponse::error(2054, "invalid file path"),
        FileTransferError::InvalidPayload => QueryResponse::error(1538, "invalid parameter"),
        FileTransferError::Io => QueryResponse::error(2052, "file input/output error"),
    }
}

fn encode_avatar_id_bytes(bytes: &[u8]) -> String {
    let mut avatar_id = String::with_capacity(bytes.len().saturating_mul(2));
    for &byte in bytes {
        let high = (byte >> 4) & 0x0f;
        let low = byte & 0x0f;
        avatar_id.push(char::from(b'a' + high));
        avatar_id.push(char::from(b'a' + low));
    }
    avatar_id
}

impl BaselineRuntime {
    pub(crate) fn resolve_actor_avatar_id(
        &self,
        session: &QuerySessionState,
        server_id: u32,
    ) -> Option<String> {
        let uid = if let Some(online) = self
            .store
            .online_clients
            .get(&session.client_id)
            .filter(|c| c.server_id == server_id)
        {
            Some(online.unique_identifier.clone())
        } else if let Some(cldbid) = session.actor_client_database_id_override {
            self.store
                .clients
                .get(&cldbid)
                .map(|c| c.unique_identifier.clone())
                .or_else(|| {
                    self.online_client_by_cldbid(server_id, cldbid)
                        .map(|c| c.unique_identifier.clone())
                })
        } else if let Some(login) = session.authenticated_login.as_ref() {
            Some(login.clone())
        } else {
            None
        }?;

        if let Ok(decoded) = BASE64_STANDARD.decode(&uid)
            && !decoded.is_empty()
        {
            Some(encode_avatar_id_bytes(&decoded))
        } else {
            Some(encode_avatar_id_bytes(uid.as_bytes()))
        }
    }

    fn set_actor_avatar_flag(
        &mut self,
        session: &QuerySessionState,
        server_id: u32,
        avatar_hash: String,
    ) {
        let cldbid_opt = self
            .store
            .online_clients
            .get(&session.client_id)
            .filter(|c| c.server_id == server_id)
            .map(|c| c.database_id)
            .or(session.actor_client_database_id_override)
            .or_else(|| {
                session
                    .authenticated_login
                    .as_ref()
                    .and_then(|l| self.store.query_accounts.get(l))
                    .and_then(|a| a.client_database_id)
            });

        if let Some(cldbid) = cldbid_opt {
            if let Some(client) = self.store.clients.get_mut(&cldbid) {
                client.client_flag_avatar = avatar_hash.clone();
                let _ = self.db.save_client(client);
            }
            for online in self.store.online_clients.values_mut() {
                if online.server_id == server_id && online.database_id == cldbid {
                    online
                        .extra_properties
                        .insert(String::from("client_flag_avatar"), avatar_hash.clone());
                }
            }
        }
    }

    fn check_ft_access(
        &self,
        session: &QuerySessionState,
        server_id: u32,
        cid: u32,
        path: &str,
        name: &str,
        cpw: Option<&str>,
        power_perm: &str,
        needed_perm: &str,
        is_mutation: bool,
        upload_size: Option<u64>,
    ) -> std::result::Result<(), QueryResponse> {
        if cid > 0 {
            let Some(channel) = self.channel_by_id(server_id, cid) else {
                return Err(QueryResponse::error(768, "invalid channelID"));
            };

            if session_has_permission_actor(session) {
                let (actor, global_perms) = self.query_actor_effective_permissions(session)?;
                let perms = self
                    .effective_permissions_for_client_in_channel_context(
                        server_id,
                        cid,
                        actor.client_database_id,
                    )
                    .unwrap_or(global_perms);

                if !channel.password.is_empty() && session.current_channel_id != Some(cid) {
                    let ignore_pw = permission_value_or_default(
                        &perms,
                        &["b_ft_ignore_password", "b_channel_join_ignore_password"],
                    ) > 0;
                    if !ignore_pw {
                        let provided = cpw.unwrap_or("");
                        if provided != channel.password {
                            return Err(QueryResponse::error(781, "invalid channel password"));
                        }
                    }
                }

                let actor_power = permission_power_max_or_default(&perms, &[power_perm]);
                let needed_power = permission_value_or_default(&perms, &[needed_perm]).max(
                    channel
                        .permissions
                        .get(needed_perm)
                        .map(|a| a.value)
                        .unwrap_or(0),
                );
                let explicitly_denied = perms.get(power_perm).is_some_and(|a| a.value < 0);
                if explicitly_denied || actor_power < needed_power {
                    return Err(self.insufficient_permission_response(power_perm));
                }
            }
        } else if session_has_permission_actor(session) {
            let (_actor, perms) = self.query_actor_effective_permissions(session)?;
            let is_icon = path.starts_with("/icon")
                || name.starts_with("/icon")
                || name.starts_with("icon_");
            let is_avatar = name == "/avatar"
                || name.starts_with("/avatar_")
                || name.starts_with("avatar");

            if is_icon && is_mutation {
                if permission_value_or_default(&perms, &["b_icon_manage"]) < 1 {
                    return Err(self.insufficient_permission_response("b_icon_manage"));
                }
                if let Some(size) = upload_size
                    && let Some(max_icon) = perms.get("i_icon_filesize")
                    && max_icon.value >= 0
                    && (size as i64) > max_icon.value
                {
                    return Err(self.insufficient_permission_response("i_icon_filesize"));
                }
            } else if is_avatar && is_mutation {
                if let Some(size) = upload_size
                    && let Some(max_avatar) = perms.get("i_client_max_avatar_filesize")
                    && max_avatar.value >= 0
                    && (size as i64) > max_avatar.value
                {
                    return Err(
                        self.insufficient_permission_response("i_client_max_avatar_filesize")
                    );
                }
            }
        }
        Ok(())
    }

    pub(crate) fn handle_ftinitupload(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = match session.selected_virtual_server_id {
            Some(id) => id,
            None => return QueryResponse::error(1024, "invalid serverID"),
        };

        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };

        let cid = row
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let path = row.get("path").map(|v| v.as_str()).unwrap_or("/");
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        if name.trim().is_empty() {
            return QueryResponse::error(1538, "invalid parameter");
        }
        let size = row
            .get("size")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let overwrite = row
            .get("overwrite")
            .and_then(|v| parse_query_bool(v))
            .unwrap_or(true);
        let resume = row
            .get("resume")
            .and_then(|v| parse_query_bool(v))
            .unwrap_or(false);
        if overwrite && resume && row.contains_key("overwrite") && row.contains_key("resume") {
            return QueryResponse::error(2056, "overwrite excludes resume");
        }
        let cpw = row.get("cpw").map(|v| v.as_str());

        if let Err(resp) = self.check_ft_access(
            session,
            server_id,
            cid,
            path,
            name,
            cpw,
            "i_ft_file_upload_power",
            "i_ft_needed_file_upload_power",
            true,
            Some(size),
        ) {
            return resp;
        }

        let actor_avatar_id = self.resolve_actor_avatar_id(session, server_id);
        let transfer_id = if let Some(id) = row.get("clientftfid").and_then(|v| v.parse::<u64>().ok())
        {
            id
        } else {
            let id = self.next_upload_id;
            self.next_upload_id += 1;
            id
        };
        let transfer_id_str = transfer_id.to_string();

        let registry = match &self.file_transfer_registry {
            Some(r) => Arc::clone(r),
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };

        match registry.prepare_upload(
            cid,
            path,
            name,
            size,
            overwrite,
            false,
            Some(session.client_id),
            Some(transfer_id_str.as_str()),
            actor_avatar_id.as_deref(),
        ) {
            Ok(transfer) => {
                if cid == 0 && (name == "/avatar" || name.starts_with("/avatar")) {
                    let hash = actor_avatar_id
                        .clone()
                        .unwrap_or_else(|| format!("avatar_{}_{}", session.client_id, size));
                    self.set_actor_avatar_flag(session, server_id, hash);
                }
                let mut resp = BTreeMap::new();
                resp.insert("clientftfid".to_string(), transfer_id.to_string());
                resp.insert(
                    "serverftfid".to_string(),
                    transfer.server_transfer_id.to_string(),
                );
                resp.insert("ftkey".to_string(), transfer.transfer_key);
                resp.insert("port".to_string(), transfer.port.to_string());
                resp.insert("seekpos".to_string(), transfer.seek_position.to_string());
                resp.insert("size".to_string(), transfer.size.to_string());
                resp.insert("proto".to_string(), "0".to_string());
                if let Some(ip) = transfer.ip {
                    resp.insert("ip".to_string(), ip);
                }
                QueryResponse::ok_row(resp)
            }
            Err(err) => map_ft_error(err),
        }
    }

    pub(crate) fn handle_ftinitdownload(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = match session.selected_virtual_server_id {
            Some(id) => id,
            None => return QueryResponse::error(1024, "invalid serverID"),
        };

        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };

        let cid = row
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let path = row.get("path").map(|v| v.as_str()).unwrap_or("/");
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        if name.trim().is_empty() {
            return QueryResponse::error(1538, "invalid parameter");
        }
        let seekpos = row
            .get("seekpos")
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        let cpw = row.get("cpw").map(|v| v.as_str());

        if let Err(resp) = self.check_ft_access(
            session,
            server_id,
            cid,
            path,
            name,
            cpw,
            "i_ft_file_download_power",
            "i_ft_needed_file_download_power",
            false,
            None,
        ) {
            return resp;
        }

        let actor_avatar_id = self.resolve_actor_avatar_id(session, server_id);
        let transfer_id = if let Some(id) = row.get("clientftfid").and_then(|v| v.parse::<u64>().ok())
        {
            id
        } else {
            let id = self.next_download_id;
            self.next_download_id += 1;
            id
        };
        let transfer_id_str = transfer_id.to_string();

        let registry = match &self.file_transfer_registry {
            Some(r) => Arc::clone(r),
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };

        match registry.prepare_download(
            cid,
            path,
            name,
            seekpos,
            false,
            Some(session.client_id),
            Some(transfer_id_str.as_str()),
            actor_avatar_id.as_deref(),
        ) {
            Ok(transfer) => {
                let mut resp = BTreeMap::new();
                resp.insert("clientftfid".to_string(), transfer_id.to_string());
                resp.insert(
                    "serverftfid".to_string(),
                    transfer.server_transfer_id.to_string(),
                );
                resp.insert("ftkey".to_string(), transfer.transfer_key);
                resp.insert("port".to_string(), transfer.port.to_string());
                resp.insert("size".to_string(), transfer.size.to_string());
                resp.insert("proto".to_string(), "0".to_string());
                if let Some(ip) = transfer.ip {
                    resp.insert("ip".to_string(), ip);
                }
                QueryResponse::ok_row(resp)
            }
            Err(err) => map_ft_error(err),
        }
    }

    pub(crate) fn handle_ftgetfilelist(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = session.selected_virtual_server_id.unwrap_or(1);
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let cid = row
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let path = row.get("path").map(|v| v.as_str()).unwrap_or("/");
        let cpw = row.get("cpw").map(|v| v.as_str());

        if let Err(resp) = self.check_ft_access(
            session,
            server_id,
            cid,
            path,
            "",
            cpw,
            "i_ft_file_browse_power",
            "i_ft_needed_file_browse_power",
            false,
            None,
        ) {
            return resp;
        }

        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };

        match registry.list_entries(cid, path) {
            Ok(entries) => {
                let mut out_rows = Vec::new();
                for entry in entries {
                    let mut r = BTreeMap::new();
                    r.insert("cid".to_string(), cid.to_string());
                    r.insert("path".to_string(), path.to_string());
                    r.insert("name".to_string(), entry.name);
                    r.insert("size".to_string(), entry.size.to_string());
                    r.insert("datetime".to_string(), entry.datetime.to_string());
                    r.insert("type".to_string(), entry.entry_type.to_string());
                    out_rows.push(r);
                }
                if out_rows.is_empty() {
                    QueryResponse::ok()
                } else {
                    QueryResponse::ok_rows(out_rows)
                }
            }
            Err(err) => map_ft_error(err),
        }
    }

    pub(crate) fn handle_ftlist(&self, session: &QuerySessionState) -> QueryResponse {
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        let transfers = match registry.transfers.lock() {
            Ok(guard) => guard,
            Err(_) => return QueryResponse::error(2052, "file transfer lock error"),
        };
        let mut rows = Vec::new();
        for transfer in transfers.values() {
            let mut r = BTreeMap::new();
            r.insert(
                "clid".to_string(),
                transfer.client_id.unwrap_or(session.client_id).to_string(),
            );
            let parent_path = transfer
                .file_path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
                .unwrap_or("/");
            let file_name = transfer
                .file_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            r.insert("path".to_string(), parent_path.to_string());
            r.insert("name".to_string(), file_name.to_string());
            r.insert("size".to_string(), transfer.size.to_string());
            r.insert("sizedone".to_string(), transfer.seek_position.to_string());
            r.insert(
                "clientftfid".to_string(),
                transfer
                    .client_transfer_id
                    .clone()
                    .unwrap_or_else(|| String::from("0")),
            );
            r.insert(
                "serverftfid".to_string(),
                transfer.server_transfer_id.to_string(),
            );
            r.insert(
                "sender".to_string(),
                if transfer.direction == FileTransferDirection::Upload {
                    String::from("0")
                } else {
                    String::from("1")
                },
            );
            r.insert("status".to_string(), String::from("1"));
            r.insert("current_speed".to_string(), String::from("0.0"));
            r.insert("average_speed".to_string(), String::from("0.0"));
            r.insert("runtime".to_string(), String::from("0"));
            rows.push(r);
        }
        rows.sort_by_key(|r| {
            r.get("serverftfid")
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0)
        });
        QueryResponse::ok_rows(rows)
    }

    pub(crate) fn handle_ftstop(
        &mut self,
        request: &CommandRequest,
        _session: &QuerySessionState,
    ) -> QueryResponse {
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let Some(serverftfid) = row.get("serverftfid").and_then(|v| v.parse::<u64>().ok()) else {
            return QueryResponse::error(1538, "serverftfid is required");
        };
        let delete = row
            .get("delete")
            .and_then(|v| parse_query_bool(v))
            .unwrap_or(false);

        let removed = {
            let Ok(mut transfers) = registry.transfers.lock() else {
                return QueryResponse::error(2052, "file transfer lock error");
            };
            let key_opt = transfers
                .iter()
                .find_map(|(k, t)| (t.server_transfer_id == serverftfid).then(|| k.clone()));
            key_opt.and_then(|k| transfers.remove(&k))
        };

        let Some(transfer) = removed else {
            return QueryResponse::error(2053, "invalid file transfer ID");
        };

        if delete && transfer.direction == FileTransferDirection::Upload {
            let _ = std::fs::remove_file(&transfer.file_path);
        }

        QueryResponse::ok()
    }

    pub(crate) fn handle_ftcreatedir(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = session.selected_virtual_server_id.unwrap_or(1);
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let cid = row
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let dirname = row.get("dirname").map(|v| v.as_str()).unwrap_or("");
        if dirname.trim().is_empty() {
            return QueryResponse::error(1538, "invalid parameter");
        }
        let cpw = row.get("cpw").map(|v| v.as_str());

        if let Err(resp) = self.check_ft_access(
            session,
            server_id,
            cid,
            "/",
            dirname,
            cpw,
            "i_ft_directory_create_power",
            "i_ft_needed_directory_create_power",
            true,
            None,
        ) {
            return resp;
        }

        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };

        match registry.create_directory(cid, dirname) {
            Ok(_) => QueryResponse::ok(),
            Err(err) => map_ft_error(err),
        }
    }

    pub(crate) fn handle_ftdeletefile(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = session.selected_virtual_server_id.unwrap_or(1);
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let cid = request
            .named_args
            .get("cid")
            .or_else(|| row.get("cid"))
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let path = request
            .named_args
            .get("path")
            .or_else(|| row.get("path"))
            .map(|v| v.as_str())
            .unwrap_or("/");
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        if name.trim().is_empty() {
            return QueryResponse::error(1538, "invalid parameter");
        }
        let cpw = request
            .named_args
            .get("cpw")
            .or_else(|| row.get("cpw"))
            .map(|v| v.as_str());

        if let Err(resp) = self.check_ft_access(
            session,
            server_id,
            cid,
            path,
            name,
            cpw,
            "i_ft_file_delete_power",
            "i_ft_needed_file_delete_power",
            true,
            None,
        ) {
            return resp;
        }

        let actor_avatar_id = self.resolve_actor_avatar_id(session, server_id);
        let registry = match &self.file_transfer_registry {
            Some(r) => Arc::clone(r),
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };

        match registry.delete_entry(cid, path, name, actor_avatar_id.as_deref()) {
            Ok(_) => {
                if cid == 0 && (name == "/avatar" || name.starts_with("/avatar")) {
                    self.set_actor_avatar_flag(session, server_id, String::new());
                }
                QueryResponse::ok()
            }
            Err(err) => map_ft_error(err),
        }
    }

    pub(crate) fn handle_ftrenamefile(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = session.selected_virtual_server_id.unwrap_or(1);
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let cid = row
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let tcid = row
            .get("tcid")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(cid);
        let oldname = row.get("oldname").map(|v| v.as_str()).unwrap_or("");
        let newname = row.get("newname").map(|v| v.as_str()).unwrap_or("");
        if oldname.trim().is_empty() || newname.trim().is_empty() {
            return QueryResponse::error(1538, "invalid parameter");
        }
        let cpw = row.get("cpw").map(|v| v.as_str());
        let tcpw = row.get("tcpw").map(|v| v.as_str()).or(cpw);

        if let Err(resp) = self.check_ft_access(
            session,
            server_id,
            cid,
            "/",
            oldname,
            cpw,
            "i_ft_file_rename_power",
            "i_ft_needed_file_rename_power",
            true,
            None,
        ) {
            return resp;
        }
        if tcid != cid {
            if let Err(resp) = self.check_ft_access(
                session,
                server_id,
                tcid,
                "/",
                newname,
                tcpw,
                "i_ft_file_rename_power",
                "i_ft_needed_file_rename_power",
                true,
                None,
            ) {
                return resp;
            }
        }

        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };

        match registry.rename_entry(cid, oldname, tcid, newname) {
            Ok(_) => QueryResponse::ok(),
            Err(err) => map_ft_error(err),
        }
    }

    pub(crate) fn handle_ftgetfileinfo(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = session.selected_virtual_server_id.unwrap_or(1);
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };
        let cid = row
            .get("cid")
            .and_then(|v| v.parse::<u32>().ok())
            .unwrap_or(0);
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        if name.trim().is_empty() {
            return QueryResponse::error(1538, "invalid parameter");
        }
        let cpw = row.get("cpw").map(|v| v.as_str());

        if let Err(resp) = self.check_ft_access(
            session,
            server_id,
            cid,
            "/",
            name,
            cpw,
            "i_ft_file_browse_power",
            "i_ft_needed_file_browse_power",
            false,
            None,
        ) {
            return resp;
        }

        let actor_avatar_id = self.resolve_actor_avatar_id(session, server_id);
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };

        match registry.stat_entry(cid, name, actor_avatar_id.as_deref()) {
            Ok(entry) => {
                let mut r = BTreeMap::new();
                r.insert("cid".to_string(), cid.to_string());
                r.insert("name".to_string(), entry.name);
                r.insert("size".to_string(), entry.size.to_string());
                r.insert("datetime".to_string(), entry.datetime.to_string());
                QueryResponse::ok_row(r)
            }
            Err(err) => map_ft_error(err),
        }
    }
}

