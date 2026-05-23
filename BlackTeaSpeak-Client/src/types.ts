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
