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
            let connection_ip = session.connection_ip.trim();
            if !connection_ip.is_empty() {
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
        let response = self.dispatch(request, session);
        self.sync_session_snapshot(&before_session, session);
        self.sync_session_client(session, &command_name);
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
            "serveredit" => self.handle_serveredit(&request, session),
            "clientaddperm" => self.handle_clientaddperm(&request, session),
            "clientdelperm" => self.handle_clientdelperm(&request, session),
            "clientpermlist" => self.handle_clientpermlist(&request, session),
            "clientmove" => self.handle_clientmove(&request, session),
            "ftinitupload" => self.handle_ftinitupload(&request, session),
            "ftinitdownload" => self.handle_ftinitdownload(&request, session),
            "ftgetfilelist" | "ftlist" => self.handle_ftgetfilelist(&request, session),
            "ftcreatedir" => self.handle_ftcreatedir(&request, session),
            "ftdeletefile" | "ftdelete" => self.handle_ftdeletefile(&request, session),
            "ftrenamefile" => self.handle_ftrenamefile(&request, session),
            "ftgetfileinfo" => self.handle_ftgetfileinfo(&request, session),
            "permfind" => self.handle_permfind(&request, session),
            "permget" => self.handle_permget(&request, session),
            "permidgetbyname" => self.handle_permidgetbyname(&request, session),
            "permissionlist" => self.handle_permissionlist(session),
            "permoverview" => self.handle_permoverview(&request, session),
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
            "serverrequestconnectioninfo" => self.handle_serverrequestconnectioninfo(session),
            "serveridgetbyport" => self.handle_serveridgetbyport(&request, session),
            "hostinfo" => self.handle_hostinfo(session),
            "instanceinfo" => self.handle_instanceinfo(session),
            "listfeaturesupport" => self.handle_listfeaturesupport(),
            "bindinglist" => self.handle_bindinglist(&request, session),
            "propertylist" => self.handle_propertylist(&request),
            "serverlist" => self.handle_serverlist(&request, session),
            "version" => self.handle_version(),
            "whoami" => self.handle_whoami(session),
            "serverinfo" => self.handle_serverinfo(session),
            "channellist" => self.handle_channellist(session),
            "musicbotlist" => self.handle_musicbotlist(session),
            "musicbotcreate" => self.handle_musicbotcreate(&request, session),
            "musicbotdelete" => self.handle_musicbotdelete(&request, session),
            "musicbotqueueadd" => self.handle_musicbotqueueadd(&request, session),
            "musicbotqueuelist" => self.handle_musicbotqueuelist(&request, session),
            "musicbotqueueremove" => self.handle_musicbotqueueremove(&request, session),
            "musicbotqueuereorder" => self.handle_musicbotqueuereorder(&request, session),
            "musicbotplayeraction" => self.handle_musicbotplayeraction(&request, session),
            "musicbotplayerinfo" => self.handle_musicbotplayerinfo(&request, session),
            "musicbotsetsubscription" => self.handle_musicbotsetsubscription(&request, session),
            "playlistaddperm" => self.handle_playlistaddperm(&request, session),
            "playlistclientaddperm" => self.handle_playlistclientaddperm(&request, session),
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
            "querycreate" => self.handle_querycreate(&request, session),
            "queryrename" => self.handle_queryrename(&request, session),
            "querychangepassword" => self.handle_querychangepassword(&request, session),
            "querydelete" => self.handle_querydelete(&request, session),
            "setclientchannelgroup" => self.handle_setclientchannelgroup(&request, session),
            "clientsetserverquerylogin" => self.handle_clientsetserverquerylogin(&request, session),
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
                | "serveridgetbyport"
                | "hostinfo"
                | "instanceinfo"
                | "listfeaturesupport"
                | "bindinglist"
                | "propertylist"
                | "serverlist"
                | "version"
                | "whoami"
                | "serverinfo"
                | "channellist"
                | "musicbotlist"
                | "musicbotplayeraction"
                | "musicbotcreate"
                | "musicbotdelete"
                | "ftgetfilelist"
                | "ftlist"
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
        )
    }

}
