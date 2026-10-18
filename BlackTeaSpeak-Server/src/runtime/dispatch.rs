use super::*;
use crate::query::{CommandRequest, QueryResponse};

impl BaselineRuntime {
    pub fn execute_request(
        &mut self,
        request: CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        self.prune_expired_active_bans();

        let command_name = request.command.clone();
        if let Some(server_id) = session.selected_virtual_server_id
            && let Some(config) = self.anti_flood_config_for_server(server_id)
        {
            let mut ignore_antiflood = false;
            if let Ok((_actor, actor_permissions)) = self.query_actor_effective_permissions(session) {
                if check_required_permission(
                    &actor_permissions,
                    &["b_client_ignore_antiflood"],
                    "b_client_ignore_antiflood",
                )
                .is_ok()
                {
                    ignore_antiflood = true;
                }
            }

            if !ignore_antiflood {
                let ip_str = session.connection_ip.trim();
                let connection_ip = if ip_str.is_empty() { "127.0.0.1" } else { ip_str };
                let now_millis = current_unix_timestamp_millis();
                let points_to_add = antiflood_command_cost(&command_name);
                if self.shared_ip_antiflood_rejected(
                    config,
                    server_id,
                    connection_ip,
                    points_to_add,
                    now_millis,
                    false,
                ) {
                    return QueryResponse::error(ERROR_CLIENT_IS_FLOODING, "client is flooding");
                }
            }
        }

        let before_session = session.clone();
        let candidate_cleanup_cid = session.selected_virtual_server_id.and_then(|server_id| {
            match command_name.as_str() {
                "clientmove" | "clientkick" | "banclient" => {
                    let target_clid = request
                        .named_args
                        .get("clid")
                        .and_then(|v| v.parse::<u64>().ok())
                        .filter(|&id| id != 0)
                        .unwrap_or(session.client_id);
                    self.store
                        .online_clients
                        .get(&target_clid)
                        .filter(|c| c.server_id == server_id)
                        .map(|c| (server_id, c.channel_id))
                        .or_else(|| session.current_channel_id.map(|cid| (server_id, cid)))
                }
                "musicbotdelete" => request
                    .named_args
                    .get("botid")
                    .or_else(|| request.named_args.get("bot_id"))
                    .and_then(|v| v.parse::<u64>().ok())
                    .and_then(|bot_id| self.music_bot_client_snapshot_by_identifier(server_id, bot_id))
                    .map(|snap| (server_id, snap.channel_id)),
                _ => None,
            }
        });
        let response = self.dispatch(request, session);
        self.sync_session_snapshot(&before_session, session);
        self.sync_session_client(session, &command_name);
        if response.error_id == 0
            && let Some((server_id, old_cid)) = candidate_cleanup_cid
            && let Some(cleanup) = self.cleanup_temporary_channel(server_id, old_cid)
        {
            self.pending_temp_cleanups.push(cleanup);
        }
        self.persist_state_best_effort();
        response
    }

    pub(crate) fn dispatch(
        &mut self,
        request: CommandRequest,
        session: &mut QuerySessionState,
    ) -> QueryResponse {
        match request.command.as_str() {
            "help" => self.handle_help(&request),
            "login" => self.handle_login(&request, session),
            "logout" => self.handle_logout(session),
            "quit" => self.handle_quit(),
            "servernotifyregister" => self.handle_servernotifyregister(&request, session),
            "servernotifyunregister" => self.handle_servernotifyunregister(session),
            "sendtextmessage" => self.handle_sendtextmessage(&request, session),
            "clientpoke" => self.handle_clientpoke(&request, session),
            "clientkick" => self.handle_clientkick(&request, session),
            "banclient" => self.handle_banclient(&request, session),
            "banlist" => self.handle_banlist(session),
            "banadd" => self.handle_banadd(&request, session),
            "bandel" => self.handle_bandel(&request, session),
            "bandelall" => self.handle_bandelall(session),
            "querylist" => self.handle_querylist(&request, session),
            "clientfind" => self.handle_clientfind(&request, session),
            "clientgetids" => self.handle_clientgetids(&request, session),
            "clientgetdbidfromuid" => self.handle_clientgetdbidfromuid(&request, session),
            "clientgetnamefromdbid" => self.handle_clientgetnamefromdbid(&request, session),
            "clientgetnamefromuid" => self.handle_clientgetnamefromuid(&request, session),
            "clientgetuidfromclid" => self.handle_clientgetuidfromclid(&request, session),
            "clientlist" => self.handle_clientlist(&request, session),
            "clientinfo" => self.handle_clientinfo(&request, session),
            "clientupdate" => self.handle_clientupdate(&request, session),
            "clientedit" => self.handle_clientedit(&request, session),
            "serveredit" => self.handle_serveredit(&request, session),
            "servercreate" => self.handle_servercreate(&request, session),
            "serverdelete" => self.handle_serverdelete(&request, session),
            "serverstart" => self.handle_serverstart(&request, session),
            "serverstop" => self.handle_serverstop(&request, session),
            "serverprocessstop" => self.handle_serverprocessstop(&request, session),
            "serversnapshotcreate" => self.handle_serversnapshotcreate(&request, session),
            "serversnapshotdeploy" => self.handle_serversnapshotdeploy(&request, session),
            "instanceedit" => self.handle_instanceedit(&request, session),
            "gm" => self.handle_gm(&request, session),
            "logadd" => self.handle_logadd(&request, session),
            "logview" | "logquery" => self.handle_logview(&request, session),
            "bantriggerlist" => self.handle_bantriggerlist(&request, session),
            "conversationfetch" => self.handle_conversationfetch(&request, session),
            "conversationhistory" => self.handle_conversationhistory(&request, session),
            "servertemppasswordadd" => self.handle_servertemppasswordadd(&request, session),
            "servertemppassworddel" => self.handle_servertemppassworddel(&request, session),
            "servertemppasswordlist" => self.handle_servertemppasswordlist(session),
            "clientaddperm" => self.handle_clientaddperm(&request, session),
            "clientdelperm" => self.handle_clientdelperm(&request, session),
            "clientpermlist" => self.handle_clientpermlist(&request, session),
            "clientmove" => self.handle_clientmove(&request, session),
            "custominfo" => self.handle_custominfo(&request, session),
            "customsearch" => self.handle_customsearch(&request, session),
            "customset" | "clientcustomset" => self.handle_customset(&request, session),
            "customdelete" | "clientcustomdelete" => self.handle_customdelete(&request, session),
            "clientdblist" => self.handle_clientdblist(&request, session),
            "clientdbinfo" => self.handle_clientdbinfo(&request, session),
            "clientdbedit" => self.handle_clientdbedit(&request, session),
            "clientdbdelete" => self.handle_clientdbdelete(&request, session),
            "clientdbfind" => self.handle_clientdbfind(&request, session),
            "channelfind" => self.handle_channelfind(&request, session),
            "messageadd" => self.handle_messageadd(&request, session),
            "messagedel" => self.handle_messagedel(&request, session),
            "messageget" => self.handle_messageget(&request, session),
            "messagelist" => self.handle_messagelist(session),
            "messageupdateflag" => self.handle_messageupdateflag(&request, session),
            "complainadd" => self.handle_complainadd(&request, session),
            "complaindel" => self.handle_complaindel(&request, session),
            "complaindelall" => self.handle_complaindelall(&request, session),
            "complainlist" => self.handle_complainlist(&request, session),
            "ftinitupload" => self.handle_ftinitupload(&request, session),
            "ftinitdownload" => self.handle_ftinitdownload(&request, session),
            "ftgetfilelist" => self.handle_ftgetfilelist(&request, session),
            "ftlist" => {
                if request.named_args.contains_key("cid")
                    || request.named_args.contains_key("path")
                    || request
                        .option_groups
                        .iter()
                        .any(|g| g.contains_key("cid") || g.contains_key("path"))
                {
                    self.handle_ftgetfilelist(&request, session)
                } else {
                    self.handle_ftlist(session)
                }
            }
            "ftstop" => self.handle_ftstop(&request, session),
            "ftcreatedir" => self.handle_ftcreatedir(&request, session),
            "ftdeletefile" | "ftdelete" => self.handle_ftdeletefile(&request, session),
            "ftrenamefile" => self.handle_ftrenamefile(&request, session),
            "ftgetfileinfo" => self.handle_ftgetfileinfo(&request, session),
            "permfind" => self.handle_permfind(&request, session),
            "permget" => self.handle_permget(&request, session),
            "permidgetbyname" => self.handle_permidgetbyname(&request, session),
            "permissionlist" => self.handle_permissionlist(session),
            "permoverview" => self.handle_permoverview(&request, session),
            "permreset" => self.handle_permreset(session),
            "desktopgetmyperms" => self.handle_desktopgetmyperms(session),
            "channelclientaddperm" => self.handle_channelclientaddperm(&request, session),
            "channelclientdelperm" => self.handle_channelclientdelperm(&request, session),
            "channelclientpermlist" => self.handle_channelclientpermlist(&request, session),
            "channeladdperm" => self.handle_channeladdperm(&request, session),
            "channeldelperm" => self.handle_channeldelperm(&request, session),
            "channelinfo" => self.handle_channelinfo(&request, session),
            "channelpermlist" => self.handle_channelpermlist(&request, session),
            "channelcreate" => self.handle_channelcreate(&request, session),
            "channeldelete" => self.handle_channeldelete(&request, session),
            "channeledit" => self.handle_channeledit(&request, session),
            "channelmove" => self.handle_channelmove(&request, session),
            "channelsubscribe" => self.handle_channelsubscribe(&request, session),
            "channelunsubscribe" => self.handle_channelunsubscribe(&request, session),
            "channelsubscribeall" => self.handle_channelsubscribeall(session),
            "channelunsubscribeall" => self.handle_channelunsubscribeall(session),
            "channelgroupadd" => self.handle_channelgroupadd(&request, session),
            "channelgroupaddperm" => self.handle_channelgroupaddperm(&request, session),
            "channelgroupclientlist" => self.handle_channelgroupclientlist(&request, session),
            "channelgroupcopy" => self.handle_channelgroupcopy(&request, session),
            "channelgroupdel" => self.handle_channelgroupdel(&request, session),
            "channelgroupdelperm" => self.handle_channelgroupdelperm(&request, session),
            "channelgrouplist" => self.handle_channelgrouplist(session),
            "channelgrouppermlist" => self.handle_channelgrouppermlist(&request, session),
            "channelgrouprename" => self.handle_channelgrouprename(&request, session),
            "servergroupaddclient" => self.handle_servergroupaddclient(&request, session),
            "servergroupadd" => self.handle_servergroupadd(&request, session),
            "servergroupaddperm" => self.handle_servergroupaddperm(&request, session),
            "servergroupautoaddperm" => self.handle_servergroupautoaddperm(&request, session),
            "servergroupautodelperm" => self.handle_servergroupautodelperm(&request, session),
            "servergroupclientlist" => self.handle_servergroupclientlist(&request, session),
            "servergroupcopy" => self.handle_servergroupcopy(&request, session),
            "servergroupdel" => self.handle_servergroupdel(&request, session),
            "servergroupdelclient" => self.handle_servergroupdelclient(&request, session),
            "servergroupdelperm" => self.handle_servergroupdelperm(&request, session),
            "servergrouplist" => self.handle_servergrouplist(session),
            "servergrouppermlist" => self.handle_servergrouppermlist(&request, session),
            "servergrouprename" => self.handle_servergrouprename(&request, session),
            "servergroupsbyclientid" => self.handle_servergroupsbyclientid(&request, session),
            "privilegekeyadd" => self.handle_tokenadd(&request, session),
            "privilegekeydelete" => self.handle_tokendelete(&request, session),
            "tokenadd" => self.handle_tokenadd(&request, session),
            "tokendelete" => self.handle_tokendelete(&request, session),
            "tokenedit" => self.handle_tokenedit(&request, session),
            "tokenactionlist" => self.handle_tokenactionlist(&request, session),
            "tokenlist" => self.handle_tokenlist(&request, session),
            "tokenuse" => self.handle_tokenuse(&request, session),
            "privilegekeylist" => self.handle_privilegekeylist(&request, session),
            "privilegekeyuse" => self.handle_tokenuse(&request, session),
            "use" => self.handle_use(&request, session),
            "serverrequestconnectioninfo" | "serverconnectioninfo" => {
                self.handle_serverrequestconnectioninfo(session)
            }
            "serveridgetbyport" => self.handle_serveridgetbyport(&request, session),
            "hostinfo" => self.handle_hostinfo(session),
            "instanceinfo" => self.handle_instanceinfo(session),
            "listfeaturesupport" | "featurelist" => self.handle_listfeaturesupport(),
            "bindinglist" => self.handle_bindinglist(&request, session),
            "propertylist" => self.handle_propertylist(&request),
            "serverlist" => self.handle_serverlist(&request, session),
            "version" => self.handle_version(),
            "whoami" => self.handle_whoami(session),
            "serverinfo" => self.handle_serverinfo(session),
            "servergetvariables" => self.handle_servergetvariables(session),
            "clientgetvariables" => self.handle_clientgetvariables(&request, session),
            "channellist" => self.handle_channellist(&request, session),
            "musicbotlist" | "mbotlist" => self.handle_musicbotlist(session),
            "musicbotcreate" | "mbotcreate" => self.handle_musicbotcreate(&request, session),
            "musicbotdelete" | "mbotdelete" => self.handle_musicbotdelete(&request, session),
            "musicbotqueueadd" | "mbotstream" => self.handle_musicbotqueueadd(&request, session),
            "musicbotqueuelist" => self.handle_musicbotqueuelist(&request, session),
            "musicbotqueueremove" => self.handle_musicbotqueueremove(&request, session),
            "musicbotqueuereorder" => self.handle_musicbotqueuereorder(&request, session),
            "musicbotplayeraction" | "mbotplayer" => self.handle_musicbotplayeraction(&request, session),
            "musicbotplayerinfo" => self.handle_musicbotplayerinfo(&request, session),
            "musicbotsetsubscription" => self.handle_musicbotsetsubscription(&request, session),
            "musicbotplaylistassign" => self.handle_musicbotplaylistassign(&request, session),
            "playlistcreate" => self.handle_playlistcreate(&request, session),
            "playlistdelete" => self.handle_playlistdelete(&request, session),
            "playlistaddperm" => self.handle_playlistaddperm(&request, session),
            "playlistdelperm" => self.handle_playlistdelperm(&request, session),
            "playlistclientaddperm" => self.handle_playlistclientaddperm(&request, session),
            "playlistdelclientperm" | "playlistclientdelperm" => {
                self.handle_playlistdelclientperm(&request, session)
            }
            "playlistclientlist" => self.handle_playlistclientlist(&request, session),
            "playlistclientpermlist" => self.handle_playlistclientpermlist(&request, session),
            "playlistedit" => self.handle_playlistedit(&request, session),
            "playlistlist" => self.handle_playlistlist(&request, session),
            "playlistinfo" => self.handle_playlistinfo(&request, session),
            "playlistpermlist" => self.handle_playlistpermlist(&request, session),
            "playlistsetsubscription" => self.handle_playlistsetsubscription(&request, session),
            "playlistsonglist" => self.handle_playlistsonglist(&request, session),
            "playlistsongadd" => self.handle_playlistsongadd(&request, session),
            "playlistsongremove" => self.handle_playlistsongremove(&request, session),
            "playlistsongreorder" => self.handle_playlistsongreorder(&request, session),
            "playlistsongsetcurrent" => self.handle_playlistsongsetcurrent(&request, session),
            "apikeyadd" => self.handle_apikeyadd(&request, session),
            "apikeylist" => self.handle_apikeylist(&request, session),
            "apikeydel" => self.handle_apikeydel(&request, session),
            "querycreate" => self.handle_querycreate(&request, session),
            "queryrename" => self.handle_queryrename(&request, session),
            "querychangepassword" => self.handle_querychangepassword(&request, session),
            "querydelete" => self.handle_querydelete(&request, session),
            "setclientchannelgroup" => self.handle_setclientchannelgroup(&request, session),
            "clientsetserverquerylogin" => self.handle_clientsetserverquerylogin(&request, session),
            "tsdnsadd" => self.handle_tsdnsadd(&request, session),
            "tsdnsdel" => self.handle_tsdnsdel(&request, session),
            "tsdnslist" => self.handle_tsdnslist(&request, session),
            "tsdnsresolve" => self.handle_tsdnsresolve(&request, session),
            "conversationmessagedelete" => self.handle_conversationmessagedelete(&request, session),
            "handshakebegin" => self.handle_handshakebegin(&request, session),
            "handshakeidentityproof" | "handshakeindentityproof" => {
                self.handle_handshakeidentityproof(&request, session)
            }
            "rtcsessiondescribe" => self.handle_rtcsessiondescribe(&request, session),
            "rtcsessionreset" => self.handle_rtcsessionreset(session),
            "rtcicecandidate" => self.handle_rtcicecandidate(&request, session),
            "broadcastvideo" => self.handle_broadcastvideo(&request, session),
            "broadcastvideoconfigure" => QueryResponse::ok(),
            "broadcastaudio" => self.handle_broadcastaudio(&request, session),
            "broadcastvideojoin" => self.handle_broadcastvideojoin(&request, session),
            "broadcastaudiojoin" => self.handle_broadcastaudiojoin(&request, session),
            "broadcastleave" | "broadcastvideoleave" | "broadcastaudioleave" => {
                self.handle_broadcastleave(&request, session)
            }
            "broadcastlist" => self.handle_broadcastlist(&request, session),
            "whispersessioninitialize" => self.handle_whispersessioninitialize(&request, session),
            "whispersessionreset" => self.handle_whispersessionreset(session),
            other => QueryResponse::error(
                259,
                format!("command {} not implemented in baseline", other),
            ),
        }
    }

    pub(crate) fn is_command_implemented(&self, name: &str) -> bool {
        matches!(
            name,
            "help"
                | "login"
                | "logout"
                | "quit"
                | "servernotifyregister"
                | "servernotifyunregister"
                | "sendtextmessage"
                | "querylist"
                | "clientfind"
                | "clientgetids"
                | "clientgetdbidfromuid"
                | "clientgetnamefromdbid"
                | "clientgetnamefromuid"
                | "clientgetuidfromclid"
                | "clientlist"
                | "clientinfo"
                | "clientupdate"
                | "clientedit"
                | "clientgetvariables"
                | "custominfo"
                | "customsearch"
                | "customset"
                | "clientcustomset"
                | "customdelete"
                | "clientcustomdelete"
                | "clientdblist"
                | "clientdbinfo"
                | "clientdbedit"
                | "clientdbdelete"
                | "clientdbfind"
                | "channelfind"
                | "messageadd"
                | "messagedel"
                | "messageget"
                | "messagelist"
                | "messageupdateflag"
                | "complainadd"
                | "complaindel"
                | "complaindelall"
                | "complainlist"
                | "serveredit"
                | "servercreate"
                | "serverdelete"
                | "serverstart"
                | "serverstop"
                | "serverprocessstop"
                | "serversnapshotcreate"
                | "serversnapshotdeploy"
                | "instanceedit"
                | "gm"
                | "logadd"
                | "logview"
                | "logquery"
                | "bantriggerlist"
                | "conversationfetch"
                | "conversationhistory"
                | "conversationmessagedelete"
                | "servertemppasswordadd"
                | "servertemppassworddel"
                | "servertemppasswordlist"
                | "clientpoke"
                | "clientkick"
                | "clientaddperm"
                | "clientdelperm"
                | "clientpermlist"
                | "banclient"
                | "banlist"
                | "banadd"
                | "bandel"
                | "bandelall"
                | "clientmove"
                | "permoverview"
                | "permreset"
                | "channelclientaddperm"
                | "channelclientdelperm"
                | "channelclientpermlist"
                | "channeladdperm"
                | "channeldelperm"
                | "channelinfo"
                | "channelpermlist"
                | "permfind"
                | "permget"
                | "permidgetbyname"
                | "permissionlist"
                | "channelcreate"
                | "channeldelete"
                | "channeledit"
                | "channelmove"
                | "channelsubscribe"
                | "channelunsubscribe"
                | "channelsubscribeall"
                | "channelunsubscribeall"
                | "channelgroupadd"
                | "channelgroupaddperm"
                | "channelgroupclientlist"
                | "channelgroupcopy"
                | "channelgroupdel"
                | "channelgroupdelperm"
                | "channelgrouplist"
                | "channelgrouppermlist"
                | "channelgrouprename"
                | "servergroupadd"
                | "servergroupaddclient"
                | "servergroupaddperm"
                | "servergroupautoaddperm"
                | "servergroupautodelperm"
                | "servergroupdel"
                | "servergroupclientlist"
                | "servergroupcopy"
                | "servergroupdelclient"
                | "servergroupdelperm"
                | "servergrouplist"
                | "servergrouppermlist"
                | "servergrouprename"
                | "servergroupsbyclientid"
                | "privilegekeyadd"
                | "privilegekeydelete"
                | "tokenadd"
                | "tokendelete"
                | "tokenedit"
                | "tokenactionlist"
                | "tokenlist"
                | "tokenuse"
                | "privilegekeylist"
                | "privilegekeyuse"
                | "setclientchannelgroup"
                | "use"
                | "serverrequestconnectioninfo"
                | "serverconnectioninfo"
                | "serveridgetbyport"
                | "hostinfo"
                | "instanceinfo"
                | "listfeaturesupport"
                | "featurelist"
                | "bindinglist"
                | "propertylist"
                | "serverlist"
                | "version"
                | "whoami"
                | "serverinfo"
                | "servergetvariables"
                | "channellist"
                | "musicbotlist"
                | "musicbotplayeraction"
                | "musicbotcreate"
                | "musicbotdelete"
                | "musicbotqueueadd"
                | "musicbotqueuelist"
                | "musicbotqueueremove"
                | "musicbotqueuereorder"
                | "musicbotplayerinfo"
                | "musicbotsetsubscription"
                | "musicbotplaylistassign"
                | "playlistcreate"
                | "playlistdelete"
                | "playlistaddperm"
                | "playlistdelperm"
                | "playlistclientaddperm"
                | "playlistdelclientperm"
                | "playlistclientdelperm"
                | "playlistclientlist"
                | "playlistclientpermlist"
                | "playlistedit"
                | "playlistlist"
                | "playlistinfo"
                | "playlistpermlist"
                | "playlistsetsubscription"
                | "playlistsonglist"
                | "playlistsongadd"
                | "playlistsongremove"
                | "playlistsongreorder"
                | "playlistsongsetcurrent"
                | "apikeyadd"
                | "apikeylist"
                | "apikeydel"
                | "ftgetfilelist"
                | "ftlist"
                | "ftstop"
                | "ftdeletefile"
                | "ftdelete"
                | "ftinitupload"
                | "ftinitdownload"
                | "ftcreatedir"
                | "ftrenamefile"
                | "ftgetfileinfo"
                | "querycreate"
                | "queryrename"
                | "querychangepassword"
                | "querydelete"
                | "clientsetserverquerylogin"
                | "tsdnsadd"
                | "tsdnsdel"
                | "tsdnslist"
                | "tsdnsresolve"
                | "handshakebegin"
                | "handshakeidentityproof"
                | "handshakeindentityproof"
                | "rtcsessiondescribe"
                | "rtcsessionreset"
                | "rtcicecandidate"
                | "broadcastvideo"
                | "broadcastaudio"
                | "broadcastvideojoin"
                | "broadcastaudiojoin"
                | "broadcastleave"
                | "broadcastlist"
                | "whispersessioninitialize"
                | "whispersessionreset"
        )
    }

}

