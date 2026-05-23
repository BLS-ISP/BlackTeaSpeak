import os
import re

def fix_mermaid(content):
    content = content.replace('participant Legacy UDP', 'participant LegacyUDP as Legacy UDP')
    content = content.replace('Legacy UDP->', 'LegacyUDP->')
    content = content.replace('->>Legacy UDP:', '->>LegacyUDP:')
    
    # Fix brackets with newlines to use quotes and <br/>
    # For example [Web Browser Client\nWebTransport] -> ["Web Browser Client<br/>WebTransport"]
    def bracket_replacer(m):
        inner = m.group(1)
        if '\\n' in inner or '/' in inner:
            inner = inner.replace('\\n', '<br/>')
            return f'["{inner}"]'
        return m.group(0)

    content = re.sub(r'\[(.*?)\]', bracket_replacer, content)
    
    # Fix Server (Desktop Transport) to Server as Server (Desktop Transport)
    content = content.replace('participant Server (Desktop Transport)', 'participant Server as Server (Desktop Transport)')
    content = content.replace('Server (Desktop Transport)->', 'Server->')
    content = content.replace('->>Server (Desktop Transport):', '->>Server:')
    content = content.replace('Note over BTEA Client, Server (Desktop Transport):', 'Note over BTEA Client, Server:')
    
    return content

for root, _, files in os.walk('docs'):
    for file in files:
        if file.endswith('.md'):
            path = os.path.join(root, file)
            with open(path, 'r', encoding='utf-8') as f:
                content = f.read()
            new_content = fix_mermaid(content)
            if new_content != content:
                with open(path, 'w', encoding='utf-8') as f:
                    f.write(new_content)
                print(f'Fixed {path}')
