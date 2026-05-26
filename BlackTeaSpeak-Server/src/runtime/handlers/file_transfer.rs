use crate::runtime::BaselineRuntime;
use crate::query::{CommandRequest, QueryResponse};
use crate::runtime::QuerySessionState;
use crate::runtime::*;

impl BaselineRuntime {
    pub(crate) fn handle_ftinitupload(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        let server_id = match session.selected_virtual_server_id {
            Some(id) => id,
            None => return QueryResponse::error(1024, "invalid serverID"),
        };
        
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };

        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        let size = row.get("size").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        
        let transfer_id = self.next_upload_id;
        self.next_upload_id += 1;
        
        let transfer_id_str = transfer_id.to_string();
        match registry.prepare_upload(cid, "/", name, size, false, false, None, Some(transfer_id_str.as_str()), None) {
            Ok(transfer) => {
                let mut resp = BTreeMap::new();
                resp.insert("clientftfid".to_string(), transfer_id.to_string());
                resp.insert("serverftfid".to_string(), transfer.server_transfer_id.to_string());
                resp.insert("ftkey".to_string(), transfer.transfer_key);
                resp.insert("port".to_string(), transfer.port.to_string());
                resp.insert("size".to_string(), transfer.size.to_string());
                resp.insert("proto".to_string(), "0".to_string());
                QueryResponse::ok_row(resp)
            }
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
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
        
        let registry = match &self.file_transfer_registry {
            Some(r) => r,
            None => return QueryResponse::error(256, "file transfer not enabled"),
        };
        
        let row = match request.option_groups.first() {
            Some(r) => r,
            None if !request.named_args.is_empty() => &request.named_args,
            _ => return QueryResponse::error(1538, "invalid parameter"),
        };

        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        
        let transfer_id = self.next_download_id;
        self.next_download_id += 1;
        
        let transfer_id_str = transfer_id.to_string();
        match registry.prepare_download(cid, "/", name, 0, false, None, Some(transfer_id_str.as_str()), None) {
            Ok(transfer) => {
                let mut resp = BTreeMap::new();
                resp.insert("clientftfid".to_string(), transfer_id.to_string());
                resp.insert("serverftfid".to_string(), transfer.server_transfer_id.to_string());
                resp.insert("ftkey".to_string(), transfer.transfer_key);
                resp.insert("port".to_string(), transfer.port.to_string());
                resp.insert("size".to_string(), transfer.size.to_string());
                resp.insert("proto".to_string(), "0".to_string());
                QueryResponse::ok_row(resp)
            }
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    pub(crate) fn handle_ftgetfilelist(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
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
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let path = row.get("path").map(|v| v.as_str()).unwrap_or("/");
        
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
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    pub(crate) fn handle_ftcreatedir(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
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
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let dirname = row.get("dirname").map(|v| v.as_str()).unwrap_or("");
        
        match registry.create_directory(cid, dirname) {
            Ok(_) => QueryResponse::ok(),
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    pub(crate) fn handle_ftdeletefile(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
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
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        
        match registry.delete_entry(cid, "/", name, None) {
            Ok(_) => QueryResponse::ok(),
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    pub(crate) fn handle_ftrenamefile(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
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
        let tcid = row.get("tcid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let oldname = row.get("oldname").map(|v| v.as_str()).unwrap_or("");
        let newname = row.get("newname").map(|v| v.as_str()).unwrap_or("");
        
        match registry.rename_entry(cid, oldname, tcid, newname) {
            Ok(_) => QueryResponse::ok(),
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

    pub(crate) fn handle_ftgetfileinfo(
        &mut self,
        request: &CommandRequest,
        session: &mut QuerySessionState,
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
        let cid = row.get("cid").and_then(|v| v.parse::<u32>().ok()).unwrap_or(0);
        let name = row.get("name").map(|v| v.as_str()).unwrap_or("");
        
        match registry.stat_entry(cid, name, None) {
            Ok(entry) => {
                let mut r = BTreeMap::new();
                r.insert("cid".to_string(), cid.to_string());
                r.insert("name".to_string(), entry.name);
                r.insert("size".to_string(), entry.size.to_string());
                r.insert("datetime".to_string(), entry.datetime.to_string());
                QueryResponse::ok_row(r)
            }
            Err(_) => QueryResponse::error(1538, "invalid parameter"),
        }
    }

}
