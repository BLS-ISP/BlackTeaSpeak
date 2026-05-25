import React from 'react';

interface RichTextProps {
  text: string;
}

export function RichText({ text }: RichTextProps) {
  if (!text) return null;

  // Split out code blocks first so we don't format inside them
  const parts = text.split(/(```[\s\S]*?```)/g);

  return (
    <div className="rich-text" style={{ wordBreak: 'break-word', whiteSpace: 'pre-wrap' }}>
      {parts.map((part, index) => {
        if (part.startsWith('```') && part.endsWith('```')) {
          // Code block
          const code = part.substring(3, part.length - 3).trim();
          return (
            <pre key={index} style={{ background: '#1e1e1e', padding: '8px', borderRadius: '4px', overflowX: 'auto', border: '1px solid #333', marginTop: '4px', marginBottom: '4px' }}>
              <code style={{ fontFamily: 'monospace', fontSize: '13px', color: '#d4d4d4' }}>{code}</code>
            </pre>
          );
        }

        // Parse inline formatting
        return <InlineRichText key={index} text={part} />;
      })}
    </div>
  );
}

function InlineRichText({ text }: { text: string }) {
  // Regex to match URLs (ending in image extensions for inline images)
  const urlRegex = /(https?:\/\/[^\s]+)/g;
  
  const tokens = text.split(urlRegex);

  return (
    <>
      {tokens.map((token, i) => {
        if (token.match(urlRegex)) {
          // Check if it's an image
          if (token.match(/\.(jpeg|jpg|gif|png|webp)(\?.*)?$/i)) {
            return (
              <div key={i} style={{ marginTop: '8px', marginBottom: '8px' }}>
                <a href={token} target="_blank" rel="noreferrer">
                  <img 
                    src={token} 
                    alt="attachment" 
                    style={{ maxWidth: '100%', maxHeight: '300px', borderRadius: '4px', border: '1px solid #333' }} 
                    onError={(e) => { (e.target as HTMLImageElement).style.display = 'none'; }}
                  />
                </a>
              </div>
            );
          }
          // Normal link
          return <a key={i} href={token} target="_blank" rel="noreferrer" style={{ color: 'var(--accent-color)', textDecoration: 'none' }}>{token}</a>;
        }

        // Process BBCode
        return <span key={i} dangerouslySetInnerHTML={{ __html: parseBBCode(token) }} />;
      })}
    </>
  );
}

function parseBBCode(text: string): string {
  let html = escapeHtml(text);
  
  // Basic formatting
  html = html.replace(/\[b\](.*?)\[\/b\]/gi, '<strong>$1</strong>');
  html = html.replace(/\[i\](.*?)\[\/i\]/gi, '<em>$1</em>');
  html = html.replace(/\[u\](.*?)\[\/u\]/gi, '<u>$1</u>');
  html = html.replace(/\[s\](.*?)\[\/s\]/gi, '<del>$1</del>');
  
  // Color and Size
  html = html.replace(/\[color=(.*?)\](.*?)\[\/color\]/gi, '<span style="color: $1;">$2</span>');
  html = html.replace(/\[size=(.*?)\](.*?)\[\/size\]/gi, '<span style="font-size: $1px;">$2</span>');

  // Links
  html = html.replace(/\[url=(.*?)\](.*?)\[\/url\]/gi, '<a href="$1" target="_blank" rel="noreferrer" style="color: var(--accent-color); text-decoration: none;">$2</a>');
  html = html.replace(/\[url\](.*?)\[\/url\]/gi, '<a href="$1" target="_blank" rel="noreferrer" style="color: var(--accent-color); text-decoration: none;">$1</a>');

  // Images
  html = html.replace(/\[img\](.*?)\[\/img\]/gi, '<img src="$1" alt="image" style="max-width: 100%; max-height: 300px; border-radius: 4px; border: 1px solid #333;" />');
  
  // Maintain backward compatibility for inline code (markdown style) as Teamspeak users sometimes use it
  html = html.replace(/`(.*?)`/g, '<code style="background: #2a2a2a; padding: 2px 4px; border-radius: 3px; font-family: monospace; font-size: 0.9em; color: #ff9d00;">$1</code>');
  
  return html;
}

function escapeHtml(unsafe: string) {
  return unsafe
       .replace(/&/g, "&amp;")
       .replace(/</g, "&lt;")
       .replace(/>/g, "&gt;")
       .replace(/"/g, "&quot;")
       .replace(/'/g, "&#039;");
}
