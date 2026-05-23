export function parseTs3Response(response: string) {
  const lines = response.split('\n');
  const parsedRows: any[] = [];
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
      
      const args: Record<string, string> = {};
      
      for (let i = startIdx; i < parts.length; i++) {
        const part = parts[i];
        if (!part) continue;
        const equalsIdx = part.indexOf('=');
        if (equalsIdx !== -1) {
          const key = part.substring(0, equalsIdx);
          let value = part.substring(equalsIdx + 1);
          value = value
            .replaceAll('\\s', ' ')
            .replaceAll('\\p', '|')
            .replaceAll('\\n', '\n')
            .replaceAll('\\r', '\r')
            .replaceAll('\\/', '/')
            .replaceAll('\\\\', '\\');
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

export function escapeTs3String(str: string): string {
  if (!str) return '';
  return str
    .replaceAll('\\', '\\\\')
    .replaceAll('/', '\\/')
    .replaceAll(' ', '\\s')
    .replaceAll('|', '\\p')
    .replaceAll('\n', '\\n')
    .replaceAll('\r', '\\r');
}
