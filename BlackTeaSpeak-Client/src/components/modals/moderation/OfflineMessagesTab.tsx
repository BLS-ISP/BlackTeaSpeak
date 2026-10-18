import React, { useState } from 'react';
import { Eye, Plus, Trash2, X } from 'lucide-react';
import { blackTeaStore, BlackTeaState } from '../../../store/useBlackTeaStore';

export const OfflineMessagesTab: React.FC<{ state: BlackTeaState }> = ({
  state,
}) => {
  const [msgUid, setMsgUid] = useState('');
  const [msgSubject, setMsgSubject] = useState('');
  const [msgBody, setMsgBody] = useState('');
  const [openedMessage, setOpenedMessage] = useState<{
    msgid: number;
    subject: string;
    cluid: string;
    body: string;
  } | null>(null);

  return (
    <>
      <form
        className="perm-assign-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (msgUid.trim() && msgSubject.trim()) {
            void blackTeaStore.sendOfflineMessage(
              msgUid.trim(),
              msgSubject.trim(),
              msgBody
            );
            setMsgSubject('');
            setMsgBody('');
          }
        }}
      >
        <select
          value=""
          onChange={(e) => {
            if (e.target.value) setMsgUid(e.target.value);
          }}
        >
          <option value="">Online-Client wählen...</option>
          {state.clients.map((cl) => (
            <option key={cl.clid} value={cl.client_unique_identifier}>
              {cl.client_nickname}
            </option>
          ))}
        </select>
        <input
          type="text"
          placeholder="Empfänger Unique ID (cluid)"
          value={msgUid}
          onChange={(e) => setMsgUid(e.target.value)}
        />
        <input
          type="text"
          placeholder="Betreff"
          value={msgSubject}
          onChange={(e) => setMsgSubject(e.target.value)}
        />
        <input
          type="text"
          placeholder="Nachrichtentext..."
          value={msgBody}
          onChange={(e) => setMsgBody(e.target.value)}
        />
        <button type="submit" className="btn-primary-sm">
          <Plus size={14} /> Senden
        </button>
      </form>

      {openedMessage && (
        <div
          style={{
            padding: '12px',
            marginBottom: '12px',
            borderRadius: '8px',
            background: 'rgba(56, 189, 248, 0.08)',
            border: '1px solid rgba(56, 189, 248, 0.3)',
          }}
        >
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              marginBottom: '6px',
            }}
          >
            <strong>
              📩 {openedMessage.subject} (Absender: {openedMessage.cluid})
            </strong>
            <button
              className="btn-icon-xs"
              onClick={() => setOpenedMessage(null)}
            >
              <X size={13} />
            </button>
          </div>
          <div style={{ fontSize: '13px', whiteSpace: 'pre-wrap' }}>
            {openedMessage.body || '(Kein Nachrichtentext)'}
          </div>
        </div>
      )}

      <div className="list-box">
        {state.offlineMessages.length === 0 ? (
          <div className="muted-note">Posteingang ist leer.</div>
        ) : (
          state.offlineMessages.map((m) => (
            <div key={m.msgid} className="list-item-row">
              <div>
                <strong>
                  {!m.flag_read && (
                    <span style={{ color: '#38bdf8', marginRight: 6 }}>●</span>
                  )}
                  {m.subject}
                </strong>
                <span className="sub-id">
                  Nachricht #{m.msgid} • Von UID: {m.cluid}
                </span>
              </div>
              <div style={{ display: 'flex', gap: '6px' }}>
                <button
                  className="btn-secondary-xs"
                  onClick={async () => {
                    const body = await blackTeaStore.readOfflineMessage(m.msgid);
                    setOpenedMessage({
                      msgid: m.msgid,
                      subject: m.subject,
                      cluid: m.cluid,
                      body,
                    });
                  }}
                >
                  <Eye size={12} /> Lesen
                </button>
                <button
                  className="btn-icon-xs danger"
                  onClick={() => void blackTeaStore.deleteOfflineMessage(m.msgid)}
                  title="Offline-Nachricht löschen"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </>
  );
};
