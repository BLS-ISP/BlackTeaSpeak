# SQLite Database Schema Reference\n\nAll persistent configurations are stored in `blackteaspeak.db`. Integrators should generally use SSH Query, but read-only queries can safely access the database.\n\n## Table: `servers`
- `server_id INTEGER PRIMARY KEY`
- `server_name TEXT NOT NULL`
- `server_welcomemessage TEXT NOT NULL DEFAULT ''`
- `server_password TEXT NOT NULL DEFAULT ''`
- `server_maxclients INTEGER NOT NULL DEFAULT 32`
- `server_port INTEGER NOT NULL DEFAULT 9987`
- `server_autostart INTEGER NOT NULL DEFAULT 1`

## Table: `channels`
- `channel_id INTEGER PRIMARY KEY AUTOINCREMENT`
- `server_id INTEGER NOT NULL`
- `parent_channel_id INTEGER NOT NULL DEFAULT 0`
- `channel_order INTEGER NOT NULL DEFAULT 0`
- `channel_name TEXT NOT NULL`
- `channel_topic TEXT NOT NULL DEFAULT ''`
- `channel_description TEXT NOT NULL DEFAULT ''`
- `channel_password TEXT NOT NULL DEFAULT ''`
- `channel_codec INTEGER NOT NULL DEFAULT 4`
- `channel_codec_quality INTEGER NOT NULL DEFAULT 10`
- `channel_maxclients INTEGER NOT NULL DEFAULT -1`
- `channel_maxfamilyclients INTEGER NOT NULL DEFAULT -1`
- `channel_flag_default INTEGER NOT NULL DEFAULT 0`
- `channel_flag_password INTEGER NOT NULL DEFAULT 0`
- `channel_flag_permanent INTEGER NOT NULL DEFAULT 1`
- `channel_flag_semi_permanent INTEGER NOT NULL DEFAULT 0`

## Table: `clients`
- `client_id INTEGER PRIMARY KEY AUTOINCREMENT`
- `client_unique_id TEXT UNIQUE NOT NULL`
- `client_nickname TEXT NOT NULL`
- `client_description TEXT NOT NULL DEFAULT ''`
- `client_created INTEGER NOT NULL DEFAULT 0`
- `client_lastconnected INTEGER NOT NULL DEFAULT 0`
- `client_totalconnections INTEGER NOT NULL DEFAULT 0`
- `client_month_bytes_uploaded INTEGER NOT NULL DEFAULT 0`
- `client_month_bytes_downloaded INTEGER NOT NULL DEFAULT 0`
- `client_total_bytes_uploaded INTEGER NOT NULL DEFAULT 0`
- `client_total_bytes_downloaded INTEGER NOT NULL DEFAULT 0`

## Table: `server_groups`
- `group_id INTEGER PRIMARY KEY AUTOINCREMENT`
- `server_id INTEGER NOT NULL`
- `name TEXT NOT NULL`
- `type INTEGER NOT NULL DEFAULT 1`
- `iconid INTEGER NOT NULL DEFAULT 0`
- `savedb INTEGER NOT NULL DEFAULT 1`
- `sortid INTEGER NOT NULL DEFAULT 0`
- `name_mode INTEGER NOT NULL DEFAULT 0`
- `modify_power INTEGER NOT NULL DEFAULT 75`
- `member_add_power INTEGER NOT NULL DEFAULT 75`
- `member_remove_power INTEGER NOT NULL DEFAULT 75`

## Table: `channel_groups`
- `group_id INTEGER PRIMARY KEY AUTOINCREMENT`
- `server_id INTEGER NOT NULL`
- `name TEXT NOT NULL`
- `type INTEGER NOT NULL DEFAULT 1`
- `iconid INTEGER NOT NULL DEFAULT 0`
- `savedb INTEGER NOT NULL DEFAULT 1`
- `sortid INTEGER NOT NULL DEFAULT 0`
- `name_mode INTEGER NOT NULL DEFAULT 0`
- `modify_power INTEGER NOT NULL DEFAULT 75`
- `member_add_power INTEGER NOT NULL DEFAULT 75`
- `member_remove_power INTEGER NOT NULL DEFAULT 75`

## Table: `group_server_permissions`
- `group_id INTEGER NOT NULL`
- `server_id INTEGER NOT NULL`
- `permission_id TEXT NOT NULL`
- `permission_value INTEGER NOT NULL`
- `permission_negated INTEGER NOT NULL DEFAULT 0`
- `permission_skip INTEGER NOT NULL DEFAULT 0`
- `server_id`
- `permission_id`

## Table: `group_channel_permissions`
- `group_id INTEGER NOT NULL`
- `server_id INTEGER NOT NULL`
- `permission_id TEXT NOT NULL`
- `permission_value INTEGER NOT NULL`
- `permission_negated INTEGER NOT NULL DEFAULT 0`
- `permission_skip INTEGER NOT NULL DEFAULT 0`
- `server_id`
- `permission_id`

## Table: `client_permissions`
- `client_id INTEGER NOT NULL`
- `server_id INTEGER NOT NULL`
- `permission_id TEXT NOT NULL`
- `permission_value INTEGER NOT NULL`
- `permission_negated INTEGER NOT NULL DEFAULT 0`
- `permission_skip INTEGER NOT NULL DEFAULT 0`
- `server_id`
- `permission_id`

## Table: `server_group_members`
- `server_id INTEGER NOT NULL`
- `group_id INTEGER NOT NULL`
- `client_id INTEGER NOT NULL`
- `group_id`
- `client_id`

## Table: `channel_group_members`
- `server_id INTEGER NOT NULL`
- `channel_id INTEGER NOT NULL`
- `client_id INTEGER NOT NULL`
- `group_id INTEGER NOT NULL`
- `channel_id`
- `client_id`

## Table: `bans`
- `ban_id INTEGER PRIMARY KEY AUTOINCREMENT`
- `server_id INTEGER NOT NULL`
- `ip TEXT`
- `name TEXT`
- `uid TEXT`
- `mytsid TEXT`
- `invoker_client_id INTEGER NOT NULL`
- `invoker_uid TEXT NOT NULL`
- `invoker_name TEXT NOT NULL`
- `created INTEGER NOT NULL`
- `duration INTEGER NOT NULL`
- `reason TEXT NOT NULL DEFAULT ''`
- `enforcements INTEGER NOT NULL DEFAULT 0`

## Table: `tokens`
- `token_id INTEGER PRIMARY KEY AUTOINCREMENT`
- `server_id INTEGER NOT NULL`
- `token TEXT NOT NULL UNIQUE`
- `description TEXT NOT NULL DEFAULT ''`
- `max_uses INTEGER NOT NULL DEFAULT 1`
- `uses INTEGER NOT NULL DEFAULT 0`
- `created INTEGER NOT NULL`
- `owner_login TEXT NOT NULL DEFAULT ''`
- `expired INTEGER`

## Table: `token_actions`
- `action_id INTEGER PRIMARY KEY AUTOINCREMENT`
- `token_id INTEGER NOT NULL`
- `action_type INTEGER NOT NULL`
- `action_id1 INTEGER NOT NULL DEFAULT 0`
- `action_id2 INTEGER NOT NULL DEFAULT 0`
- `action_text TEXT NOT NULL DEFAULT ''`

