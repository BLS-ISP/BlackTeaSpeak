import React, { useState } from 'react';
import { CheckCircle, Copy, Key, Plus, Trash2 } from 'lucide-react';
import { blackTeaStore, BlackTeaState } from '../../../store/useBlackTeaStore';

export const PrivilegeKeysPane: React.FC<{ state: BlackTeaState }> = ({
  state,
}) => {
  const [redeemInput, setRedeemInput] = useState('');
  const [tokenType, setTokenType] = useState<0 | 1>(0);
  const [tokenGroupId, setTokenGroupId] = useState<number>(
    state.serverGroups.find((g) => g.sgid === 6)?.sgid ??
      state.serverGroups[0]?.sgid ??
      6
  );
  const [tokenChannelId, setTokenChannelId] = useState<number>(
    state.currentChannelId || state.channels[0]?.cid || 1
  );
  const [tokenDesc, setTokenDesc] = useState('Admin Privilege Key');

  const handleTokenTypeChange = (nextType: 0 | 1) => {
    setTokenType(nextType);
    if (nextType === 0) {
      setTokenGroupId(
        state.serverGroups.find((g) => g.sgid === 6)?.sgid ??
          state.serverGroups[0]?.sgid ??
          6
      );
    } else {
      setTokenGroupId(
        state.channelGroups.find((g) => g.cgid === 5)?.cgid ??
          state.channelGroups[0]?.cgid ??
          5
      );
    }
  };

  return (
    <div className="modal-pane">
      <h4>
        <Key size={15} /> Privilege Key einlösen / generieren
      </h4>

      <form
        className="inline-form"
        onSubmit={(e) => {
          e.preventDefault();
          if (redeemInput.trim()) {
            void blackTeaStore.useToken(redeemInput.trim());
            setRedeemInput('');
          }
        }}
      >
        <input
          type="text"
          placeholder="Privilege Key / Token hier einfügen..."
          value={redeemInput}
          onChange={(e) => setRedeemInput(e.target.value)}
        />
        <button type="submit" className="btn-primary-sm">
          <CheckCircle size={14} /> Einlösen
        </button>
      </form>

      <form
        className="stack-form"
        onSubmit={(e) => {
          e.preventDefault();
          void blackTeaStore.createToken(
            tokenType,
            tokenGroupId,
            tokenType === 1 ? tokenChannelId : 0,
            tokenDesc
          );
        }}
      >
        <div className="form-row-2">
          <select
            value={tokenType}
            onChange={(e) =>
              handleTokenTypeChange(Number(e.target.value) as 0 | 1)
            }
          >
            <option value={0}>Typ 0: Servergruppe</option>
            <option value={1}>Typ 1: Channelgruppe</option>
          </select>

          <select
            value={tokenGroupId}
            onChange={(e) => setTokenGroupId(Number(e.target.value))}
          >
            {tokenType === 0
              ? state.serverGroups.map((sg) => (
                  <option key={sg.sgid} value={sg.sgid}>
                    {sg.name} (SGID #{sg.sgid})
                  </option>
                ))
              : state.channelGroups.map((cg) => (
                  <option key={cg.cgid} value={cg.cgid}>
                    {cg.name} (CGID #{cg.cgid})
                  </option>
                ))}
          </select>
        </div>

        {tokenType === 1 && (
          <div className="form-field">
            <label>Ziel-Channel für Channelgruppen-Key</label>
            <select
              value={tokenChannelId}
              onChange={(e) => setTokenChannelId(Number(e.target.value))}
            >
              {state.channels.map((ch) => (
                <option key={ch.cid} value={ch.cid}>
                  {ch.channel_name} (#{ch.cid})
                </option>
              ))}
            </select>
          </div>
        )}

        <div className="inline-form">
          <input
            type="text"
            value={tokenDesc}
            onChange={(e) => setTokenDesc(e.target.value)}
            placeholder="Beschreibung..."
          />
          <button type="submit" className="btn-secondary-sm">
            <Plus size={14} /> Key erzeugen
          </button>
        </div>
      </form>

      <div className="list-box">
        {state.tokens.length === 0 ? (
          <div className="muted-note">Keine offenen Privilege Keys vorhanden.</div>
        ) : (
          state.tokens.map((t) => (
            <div key={t.token} className="list-item-row">
              <div>
                <code>{t.token}</code>
                <span className="sub-id">
                  {t.token_description ||
                    (t.token_type === 0
                      ? `Servergruppe #${t.token_id1}`
                      : `Channelgruppe #${t.token_id1} (Channel #${t.token_id2})`)}
                </span>
              </div>
              <div style={{ display: 'flex', gap: '5px' }}>
                <button
                  className="btn-icon-xs"
                  onClick={() => {
                    void navigator.clipboard?.writeText(t.token);
                    blackTeaStore.showToast(
                      'Privilege Key in Zwischenablage kopiert'
                    );
                  }}
                  title="In Zwischenablage kopieren"
                >
                  <Copy size={13} />
                </button>
                <button
                  className="btn-icon-xs danger"
                  onClick={() => void blackTeaStore.deleteToken(t.token)}
                  title="Key löschen"
                >
                  <Trash2 size={13} />
                </button>
              </div>
            </div>
          ))
        )}
      </div>
    </div>
  );
};
