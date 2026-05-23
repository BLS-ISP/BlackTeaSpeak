export function parseTs3Response(response) {
  const lines = response.split('\n');
  const parsedRows = [];
  let currentCommand = "unknown";

  for (let line of lines) {
    line = line.trim();
    if (!line) continue;
    
    const rows = line.split('|');
    for (let row of rows) {
      row = row.trim();
      if (!row) continue;
      
      const parts = row.split(' ');
      let startIdx = 0;
      
      if (parts.length > 0 && !parts[0].includes('=')) {
        currentCommand = parts[0];
        startIdx = 1;
      }
      
      const args = {};
      
      for (let i = startIdx; i < parts.length; i++) {
        const part = parts[i];
        if (!part) continue;
        const equalsIdx = part.indexOf('=');
        if (equalsIdx !== -1) {
          const key = part.substring(0, equalsIdx);
          let value = part.substring(equalsIdx + 1);
          value = value.replace(/\\s/g, ' ').replace(/\\p/g, '|').replace(/\\n/g, '\n').replace(/\\r/g, '\r').replace(/\\\\/g, '\\').replace(/\\//g, '/');
          args[key] = value;
        } else {
          args[part] = "true";
        }
      }
      parsedRows.push({ command: currentCommand, args });
    }
  }
  return parsedRows;
}

try {
  const payload = `channellist channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_name=Default\\sChannel channel_order=0 channel_topic=Default\\sChannel\\shas\\sno\\stopic cid=1 pid=0 total_clients=4|channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_name=Music\\sLounge channel_order=1 channel_topic=Music\\sbot\\sstaging\\sarea cid=2 pid=0 total_clients=1\nerror id=0 msg=ok`;
  console.log("Parsing...");
  const res = parseTs3Response(payload);
  console.log("Parsed successfully!", res);
} catch (e) {
  console.error("ERROR:", e);
}
