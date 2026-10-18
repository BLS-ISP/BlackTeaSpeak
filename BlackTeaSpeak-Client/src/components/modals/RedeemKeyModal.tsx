import React, { useState } from 'react';
import { KeyRound, ShieldCheck, Sparkles, AlertCircle, X } from 'lucide-react';
import {
  blackTeaStore,
  getClientCapabilities,
  useBlackTeaStore,
} from '../../store/useBlackTeaStore';

export const RedeemKeyModal: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const state = useBlackTeaStore();
  const caps = getClientCapabilities(state);
  const [tokenInput, setTokenInput] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = tokenInput.trim();
    if (!trimmed) {
      setErrorMsg('Bitte gib einen Berechtigungsschlüssel (Privilege Key) ein.');
      return;
    }
    setSubmitting(true);
    setErrorMsg(null);
    try {
      await blackTeaStore.useToken(trimmed);
      setTokenInput('');
      onClose();
    } catch (err: any) {
      setErrorMsg(err?.message || 'Berechtigungsschlüssel ungültig oder bereits verbraucht.');
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="modal-card redeem-key-modal">
      <div className="modal-header">
        <div className="modal-title">
          <KeyRound size={18} className="accent-amber" />
          <span>Berechtigungsschlüssel benutzen (Privilege Key)</span>
        </div>
        <button className="btn-icon-sm" onClick={onClose} title="Schließen">
          <X size={16} />
        </button>
      </div>

      <div className="modal-body">
        <div className="privilege-hero-banner">
          <div className="privilege-hero-icon">
            {caps.isAdmin ? <ShieldCheck size={24} /> : <Sparkles size={24} />}
          </div>
          <div className="privilege-hero-content">
            <strong>
              {caps.isAdmin
                ? 'Du besitzt bereits Server-Admin-Berechtigungen'
                : 'Erweiterte Server-Rechte freischalten'}
            </strong>
            <p>
              {caps.isAdmin
                ? `Deine aktuelle Rolle ist "${caps.roleLabel}". Du kannst dennoch einen weiteren Schlüssel für zusätzliche Server- oder Channelgruppen einlösen.`
                : 'Gib hier deinen einmaligen ServerAdmin- oder Gruppen-Berechtigungsschlüssel ein, um Administrations-, Moderations- und Rechte-Werkzeuge in Echtzeit freizuschalten.'}
            </p>
          </div>
        </div>

        <form className="stack-form" onSubmit={handleSubmit}>
          <div className="form-field">
            <label>Berechtigungsschlüssel / Token</label>
            <input
              type="text"
              className="token-code-input"
              placeholder="z.B. compat00000001000000006ad2ca69"
              value={tokenInput}
              onChange={(e) => {
                setTokenInput(e.target.value);
                if (errorMsg) setErrorMsg(null);
              }}
              autoFocus
              disabled={submitting || state.connectionState !== 'connected'}
            />
          </div>

          {errorMsg && (
            <div className="modal-error-box">
              <AlertCircle size={15} />
              <span>{errorMsg}</span>
            </div>
          )}

          <div className="modal-footer-actions">
            <button type="button" className="btn-secondary-sm" onClick={onClose}>
              Abbrechen
            </button>
            <button
              type="submit"
              className="btn-primary-sm"
              disabled={submitting || !tokenInput.trim() || state.connectionState !== 'connected'}
            >
              <KeyRound size={14} />
              <span>{submitting ? 'Wird geprüft...' : 'Schlüssel einlösen'}</span>
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
