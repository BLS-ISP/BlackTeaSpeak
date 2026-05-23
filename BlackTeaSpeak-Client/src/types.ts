export type Channel = {
  cid: string;
  pid: string;
  channel_name: string;
  channel_topic?: string;
  channel_description?: string;
  total_clients?: string;
};

export type Client = {
  clid: string;
  cid: string;
  client_nickname: string;
  client_type: string;
  client_input_muted?: boolean;
  client_output_muted?: boolean;
  is_talking?: boolean;
  client_version?: string;
  client_platform?: string;
  client_created?: string;
  connection_connected_time?: string;
};

export type ChatMessage = {
  id: string;
  timestamp: number;
  senderName: string;
  senderId: string;
  targetMode: number; // 1 = Private, 2 = Channel, 3 = Server
  message: string;
};

export type FileEntry = {
  name: string;
  size: number;
  datetime: number;
  type: number; // 1 = File, 0 = Directory
  empty: boolean;
};

export type ServerGroup = {
  sgid: string;
  name: string;
  type: string;
  iconid: string;
  savedb: string;
};

export type ChannelGroup = {
  cgid: string;
  name: string;
  type: string;
  iconid: string;
  savedb: string;
};

export type Permission = {
  permid: string;
  permname: string;
  permvalue: string;
  permskip?: boolean;
  permnegated?: boolean;
};

export type Ban = {
  banid: string;
  ip: string;
  name: string;
  uid: string;
  created: string;
  duration: string;
  invokername: string;
  reason: string;
};

export type Token = {
  token: string;
  type: string;
  id1: string;
  id2: string;
  created: string;
  description: string;
};
