import React, { useEffect, useRef, useState } from 'react';
import { Send, Terminal, MessageSquare, Hash, Server, Zap, HelpCircle } from 'lucide-react';
import {
  blackTeaStore,
  useBlackTeaStore,
} from '../../store/useBlackTeaStore';

const SUBMENU_PILLS = [
  { cmd: '/?', label: '🧭 Übersicht (/?)' },
  { cmd: '/? bot', label: '🎵 MusikBot' },
  { cmd: '/? queue', label: '📋 Queue & Loop' },
  { cmd: '/? files', label: '📂 Lokale Dateien' },
  { cmd: '/? channel', label: '💬 Channel' },
  { cmd: '/? user', label: '👤 Benutzer' },
  { cmd: '/? admin', label: '🛡️ Admin & Key' },
  { cmd: '/? server', label: '🖥️ Server' },
];

const SLASH_SUGGESTIONS = [
  { cmd: '/?', desc: 'Generelle Befehlsübersicht & alle Submenüs anzeigen', immediate: true },
  { cmd: '/? bot', desc: 'Submenü: MusikBot-Steuerung (Spawn, Play, Seek, Vol)', immediate: true },
  { cmd: '/? queue', desc: 'Submenü: Warteschlange, Loop/Repeat, Shuffle & Move', immediate: true },
  { cmd: '/? files', desc: 'Submenü: Lokale Server-Audiodateien durchsuchen & starten', immediate: true },
  { cmd: '/? channel', desc: 'Submenü: Channel- & Raumsteuerung (Info, Join, Create)', immediate: true },
  { cmd: '/? user', desc: 'Submenü: Benutzer & Eigensteuerung (Whoami, Nick, Away, Poke)', immediate: true },
  { cmd: '/? admin', desc: 'Submenü: Moderation, Servergruppen & Privilege-Keys', immediate: true },
  { cmd: '/? server', desc: 'Submenü: Server-Informationen & aktive Ports', immediate: true },
  { cmd: '/play ', desc: 'YouTube-Link, Stream-URL oder lokale Datei (#Nr) abspielen', immediate: false },
  { cmd: '/add ', desc: 'Track oder lokale Datei (#Nr) zur Warteschlange hinzufügen', immediate: false },
  { cmd: '/queue', desc: 'Aktuelle MusikBot-Warteschlange anzeigen', immediate: true },
  { cmd: '/files', desc: 'Verfügbare lokale Audiodateien auflisten', immediate: true },
  { cmd: '/pause', desc: 'MusikBot-Wiedergabe pausieren', immediate: true },
  { cmd: '/resume', desc: 'MusikBot-Wiedergabe fortsetzen', immediate: true },
  { cmd: '/skip', desc: 'Zum nächsten Titel in der Warteschlange springen', immediate: true },
  { cmd: '/seek ', desc: 'Im aktuellen Track spulen (z.B. /seek 1:30)', immediate: false },
  { cmd: '/vol ', desc: 'MusikBot-Lautstärke setzen (0-100)', immediate: false },
  { cmd: '/loop ', desc: 'Wiederholungsmodus setzen (off | all | one)', immediate: false },
  { cmd: '/np', desc: 'Aktuellen Song, Fortschritt & Bot-Status anzeigen', immediate: true },
  { cmd: '/token ', desc: 'Berechtigungsschlüssel (Privilege Key) einlösen', immediate: false },
  { cmd: '/whoami', desc: 'Eigene Sitzungsdaten, CLID, DBID & Gruppen anzeigen', immediate: true },
  { cmd: '/online', desc: 'Alle verbundenen Clients & Bots anzeigen', immediate: true },
  { cmd: '/ch list', desc: 'Alle Räume auf dem Server auflisten', immediate: true },
];

function renderInlineFormatting(
  segment: string,
  onCommandClick: (cmdText: string) => void
): React.ReactNode[] {
  const tokenRegex = /(\[b\].*?\[\/b\]|\[i\].*?\[\/i\]|https?:\/\/[^\s[\]]+)/gi;
  const parts = segment.split(tokenRegex);

  return parts.map((part, idx) => {
    const lower = part.toLowerCase();
    if (lower.startsWith('[b]') && lower.endsWith('[/b]')) {
      const inner = part.slice(3, -4);
      const trimmedInner = inner.trim();
      if (trimmedInner.startsWith('/') || trimmedInner.startsWith('!')) {
        return (
          <button
            key={idx}
            type="button"
            onClick={() => onCommandClick(trimmedInner)}
            style={{
              display: 'inline-block',
              background: 'rgba(56, 189, 248, 0.14)',
              color: '#38bdf8',
              border: '1px solid rgba(56, 189, 248, 0.35)',
              borderRadius: '4px',
              padding: '0px 5px',
              margin: '1px 2px',
              fontFamily: 'var(--font-mono)',
              fontSize: '11.5px',
              fontWeight: 600,
              cursor: 'pointer',
            }}
            title={`Klicken zum Ausführen / Einfügen: ${trimmedInner}`}
          >
            {inner}
          </button>
        );
      }
      return <strong key={idx} style={{ color: '#f8fafc' }}>{inner}</strong>;
    }
    if (lower.startsWith('[i]') && lower.endsWith('[/i]')) {
      return <em key={idx} style={{ color: '#94a3b8' }}>{part.slice(3, -4)}</em>;
    }
    if (part.match(/^https?:\/\/[^\s[\]]+$/i)) {
      return (
        <a
          key={idx}
          href={part}
          target="_blank"
          rel="noreferrer"
          className="chat-link"
        >
          {part}
        </a>
      );
    }
    return <span key={idx}>{part}</span>;
  });
}

export const RichChatDock: React.FC = () => {
  const state = useBlackTeaStore();
  const [input, setInput] = useState('');
  const bottomRef = useRef<HTMLDivElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const filteredMessages = state.chatMessages.filter((m) => {
    if (m.isSystem) return true;
    if (state.activeChatTab === 'server') return m.targetmode === 3;
    if (state.activeChatTab === 'channel') return m.targetmode === 2 || m.targetmode === 3;
    return m.targetmode === 1;
  });

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [filteredMessages.length]);

  const handleCommandTrigger = async (cmdText: string) => {
    if (state.connectionState !== 'connected') return;
    // If command contains placeholder like `<URL>` or `[Name]`, prefill the base command in the input
    const angleIdx = cmdText.search(/[<[]/);
    if (angleIdx > 0) {
      const prefix = cmdText.slice(0, angleIdx).trimEnd() + ' ';
      setInput(prefix);
      inputRef.current?.focus();
      return;
    }
    await blackTeaStore.sendChatMessage(cmdText);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!input.trim()) return;
    const text = input;
    setInput('');
    await blackTeaStore.sendChatMessage(text);
  };

  const privateTargetClient = state.clients.find(
    (c) => c.clid === (state.privateChatTargetClientId ?? state.selectedClientId)
  );
  const currentChannel = state.channels.find((c) => c.cid === state.currentChannelId);

  const trimmedInput = input.trim();
  const matchingSuggestions =
    trimmedInput.startsWith('/') || trimmedInput.startsWith('!')
      ? SLASH_SUGGESTIONS.filter((s) =>
          s.cmd.toLowerCase().startsWith(trimmedInput.replace(/^!/, '/').toLowerCase())
        ).slice(0, 8)
      : [];

  return (
    <section className="rich-chat-dock">
      {state.pokeAlert && (
        <div className="poke-alert-bar">
          <div className="poke-alert-text">
            <Zap size={15} />
            <strong>{state.pokeAlert.invokerName} hat dich angestupst:</strong>{' '}
            {state.pokeAlert.message}
          </div>
          <button
            className="btn-xs"
            onClick={() => blackTeaStore.patchState({ pokeAlert: null })}
          >
            OK
          </button>
        </div>
      )}

      <div className="chat-tabs-bar" style={{ flexWrap: 'wrap', gap: '4px' }}>
        <button
          className={`chat-tab ${state.activeChatTab === 'channel' ? 'active' : ''}`}
          onClick={() => blackTeaStore.patchState({ activeChatTab: 'channel' })}
        >
          <Hash size={13} />
          <span>{currentChannel ? currentChannel.channel_name : 'Channel-Chat'}</span>
        </button>

        <button
          className={`chat-tab ${state.activeChatTab === 'server' ? 'active' : ''}`}
          onClick={() => blackTeaStore.patchState({ activeChatTab: 'server' })}
        >
          <Server size={13} />
          <span>Server-Ereignisse</span>
        </button>

        <button
          className={`chat-tab ${state.activeChatTab === 'private' ? 'active' : ''}`}
          onClick={() => blackTeaStore.patchState({ activeChatTab: 'private' })}
        >
          <MessageSquare size={13} />
          <span>
            Privat-Chat{privateTargetClient ? `: ${privateTargetClient.client_nickname}` : ''}
          </span>
        </button>

        <div className="chat-hint-right" style={{ display: 'flex', alignItems: 'center', gap: '4px', marginLeft: 'auto', flexWrap: 'wrap' }}>
          {SUBMENU_PILLS.map((pill) => (
            <button
              key={pill.cmd}
              type="button"
              onClick={() => handleCommandTrigger(pill.cmd)}
              disabled={state.connectionState !== 'connected'}
              style={{
                background: 'rgba(30, 41, 59, 0.85)',
                color: '#cbd5e1',
                border: '1px solid rgba(148, 163, 184, 0.25)',
                borderRadius: '4px',
                padding: '2px 7px',
                fontSize: '11px',
                cursor: state.connectionState === 'connected' ? 'pointer' : 'not-allowed',
              }}
              title={`Submenü öffnen: ${pill.cmd}`}
            >
              {pill.label}
            </button>
          ))}
        </div>
      </div>

      <div className="chat-messages-scroll">
        {filteredMessages.map((msg) => {
          const timeStr = new Date(msg.timestamp).toLocaleTimeString('de-DE', {
            hour: '2-digit',
            minute: '2-digit',
            second: '2-digit',
          });
          return (
            <div
              key={msg.id}
              className={`chat-message-row ${msg.isSystem ? 'system-msg' : ''}`}
            >
              <span className="chat-time">{timeStr}</span>
              <strong className="chat-author">{msg.invokername}</strong>
              <span className="chat-body" style={{ whiteSpace: 'pre-wrap' }}>
                {renderInlineFormatting(msg.msg, handleCommandTrigger)}
              </span>
            </div>
          );
        })}
        <div ref={bottomRef} />
      </div>

      {matchingSuggestions.length > 0 && (
        <div
          style={{
            background: 'rgba(15, 23, 42, 0.97)',
            borderTop: '1px solid rgba(56, 189, 248, 0.3)',
            padding: '6px 10px',
            display: 'flex',
            flexWrap: 'wrap',
            gap: '6px',
          }}
        >
          {matchingSuggestions.map((s) => (
            <button
              key={s.cmd}
              type="button"
              onClick={() => {
                if (s.immediate) {
                  setInput('');
                  blackTeaStore.sendChatMessage(s.cmd);
                } else {
                  setInput(s.cmd);
                  inputRef.current?.focus();
                }
              }}
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: '6px',
                background: 'rgba(30, 41, 59, 0.9)',
                border: '1px solid rgba(56, 189, 248, 0.3)',
                borderRadius: '5px',
                padding: '3px 8px',
                color: '#e2e8f0',
                fontSize: '11.5px',
                cursor: 'pointer',
              }}
            >
              <code style={{ color: '#38bdf8', fontWeight: 700 }}>{s.cmd.trim()}</code>
              <span style={{ color: '#94a3b8' }}>{s.desc}</span>
            </button>
          ))}
        </div>
      )}

      <form className="chat-input-bar" onSubmit={handleSubmit}>
        <button
          type="button"
          className="btn-xs"
          disabled={state.connectionState !== 'connected'}
          onClick={() => handleCommandTrigger('/?')}
          title="Befehlsübersicht (/?) anzeigen"
          style={{ display: 'flex', alignItems: 'center', gap: '4px', flexShrink: 0 }}
        >
          <HelpCircle size={13} /> <code>/?</code>
        </button>
        <input
          ref={inputRef}
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          disabled={state.connectionState !== 'connected'}
          placeholder={
            state.connectionState !== 'connected'
              ? 'Verbinde dich mit dem Server, um Nachrichten zu schreiben...'
              : state.activeChatTab === 'private'
              ? `Privatnachricht oder Befehl (/? für Übersicht)...`
              : `Nachricht oder Befehl eingeben (z.B. /? · /? bot · /play <URL>)...`
          }
        />
        <button
          type="submit"
          className="btn-primary-sm"
          disabled={state.connectionState !== 'connected' || !input.trim()}
        >
          <Send size={14} /> Senden
        </button>
      </form>
    </section>
  );
};
