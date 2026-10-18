import React, { useState } from 'react';
import { Save } from 'lucide-react';
import { blackTeaStore, BlackTeaState } from '../../../store/useBlackTeaStore';

export const VirtualServerSettingsTab: React.FC<{ state: BlackTeaState }> = ({
  state,
}) => {
  const [srvName, setSrvName] = useState(state.serverInfo.virtualserver_name);
  const [srvWelcome, setSrvWelcome] = useState(
    state.serverInfo.virtualserver_welcomemessage
  );
  const [srvMaxClients, setSrvMaxClients] = useState(
    state.serverInfo.virtualserver_maxclients
  );
  const [srvBannerUrl, setSrvBannerUrl] = useState(
    state.serverInfo.virtualserver_hostbanner_url || ''
  );
  const [srvBannerGfxUrl, setSrvBannerGfxUrl] = useState(
    state.serverInfo.virtualserver_hostbanner_gfx_url || ''
  );
  const [srvHostbuttonTooltip, setSrvHostbuttonTooltip] = useState(
    state.serverInfo.virtualserver_hostbutton_tooltip || ''
  );
  const [srvHostbuttonUrl, setSrvHostbuttonUrl] = useState(
    state.serverInfo.virtualserver_hostbutton_url || ''
  );
  const [srvHostmessage, setSrvHostmessage] = useState(
    state.serverInfo.virtualserver_hostmessage || ''
  );
  const [srvHostmessageMode, setSrvHostmessageMode] = useState(
    state.serverInfo.virtualserver_hostmessage_mode ?? 0
  );
  const [srvCodecEncryptionMode, setSrvCodecEncryptionMode] = useState(
    state.serverInfo.virtualserver_codec_encryption_mode ?? 2
  );
  const [srvDefaultSg, setSrvDefaultSg] = useState(
    state.serverInfo.virtualserver_default_server_group ?? 8
  );
  const [srvDefaultCg, setSrvDefaultCg] = useState(
    state.serverInfo.virtualserver_default_channel_group ?? 8
  );
  const [antifloodReduce, setAntifloodReduce] = useState(
    state.serverInfo.virtualserver_antiflood_points_tick_reduce ?? 5
  );
  const [antifloodCmdBlock, setAntifloodCmdBlock] = useState(
    state.serverInfo.virtualserver_antiflood_points_needed_command_block ?? 150
  );
  const [antifloodIpBlock, setAntifloodIpBlock] = useState(
    state.serverInfo.virtualserver_antiflood_points_needed_ip_block ?? 250
  );

  return (
    <form
      className="stack-form"
      onSubmit={(e) => {
        e.preventDefault();
        void blackTeaStore.updateVirtualServer({
          name: srvName,
          welcomeMessage: srvWelcome,
          maxClients: srvMaxClients,
          hostbannerUrl: srvBannerUrl,
          hostbannerGfxUrl: srvBannerGfxUrl,
          hostbuttonTooltip: srvHostbuttonTooltip,
          hostbuttonUrl: srvHostbuttonUrl,
          hostmessage: srvHostmessage,
          hostmessageMode: srvHostmessageMode,
          codecEncryptionMode: srvCodecEncryptionMode,
          defaultServerGroup: srvDefaultSg,
          defaultChannelGroup: srvDefaultCg,
          antifloodPointsTickReduce: antifloodReduce,
          antifloodPointsNeededCommandBlock: antifloodCmdBlock,
          antifloodPointsNeededIpBlock: antifloodIpBlock,
        });
      }}
    >
      <div className="form-row-2">
        <div className="form-field">
          <label>Servername (`virtualserver_name`)</label>
          <input
            type="text"
            value={srvName}
            onChange={(e) => setSrvName(e.target.value)}
          />
        </div>

        <div className="form-field">
          <label>Max. Clients (`virtualserver_maxclients`)</label>
          <input
            type="number"
            min={1}
            max={2048}
            value={srvMaxClients}
            onChange={(e) => setSrvMaxClients(Number(e.target.value))}
          />
        </div>
      </div>

      <div className="form-field">
        <label>Willkommensnachricht (`virtualserver_welcomemessage`)</label>
        <input
          type="text"
          value={srvWelcome}
          onChange={(e) => setSrvWelcome(e.target.value)}
        />
      </div>

      <div className="form-row-2">
        <div className="form-field">
          <label>Host-Nachricht (`virtualserver_hostmessage`)</label>
          <input
            type="text"
            value={srvHostmessage}
            onChange={(e) => setSrvHostmessage(e.target.value)}
            placeholder="Optionale Host-Nachricht beim Verbinden..."
          />
        </div>

        <div className="form-field">
          <label>Host-Nachrichten-Modus (`virtualserver_hostmessage_mode`)</label>
          <select
            value={srvHostmessageMode}
            onChange={(e) => setSrvHostmessageMode(Number(e.target.value))}
          >
            <option value={0}>0 — Keine Anzeige</option>
            <option value={1}>1 — Im Server-Log anzeigen</option>
            <option value={2}>2 — Als Modal-Dialog anzeigen</option>
            <option value={3}>3 — Modal anzeigen &amp; Verbindung trennen</option>
          </select>
        </div>
      </div>

      <div className="form-row-2">
        <div className="form-field">
          <label>Hostbanner Grafik-URL (`virtualserver_hostbanner_gfx_url`)</label>
          <input
            type="text"
            value={srvBannerGfxUrl}
            onChange={(e) => setSrvBannerGfxUrl(e.target.value)}
            placeholder="https://.../banner.png"
          />
        </div>

        <div className="form-field">
          <label>Hostbanner Klick-URL (`virtualserver_hostbanner_url`)</label>
          <input
            type="text"
            value={srvBannerUrl}
            onChange={(e) => setSrvBannerUrl(e.target.value)}
            placeholder="https://..."
          />
        </div>
      </div>

      <div className="form-row-2">
        <div className="form-field">
          <label>Hostbutton Tooltip (`virtualserver_hostbutton_tooltip`)</label>
          <input
            type="text"
            value={srvHostbuttonTooltip}
            onChange={(e) => setSrvHostbuttonTooltip(e.target.value)}
          />
        </div>

        <div className="form-field">
          <label>Hostbutton URL (`virtualserver_hostbutton_url`)</label>
          <input
            type="text"
            value={srvHostbuttonUrl}
            onChange={(e) => setSrvHostbuttonUrl(e.target.value)}
            placeholder="https://..."
          />
        </div>
      </div>

      <div className="form-row-2">
        <div className="form-field">
          <label>Sprachverschlüsselung (`virtualserver_codec_encryption_mode`)</label>
          <select
            value={srvCodecEncryptionMode}
            onChange={(e) => setSrvCodecEncryptionMode(Number(e.target.value))}
          >
            <option value={0}>0 — Pro Channel konfigurierbar</option>
            <option value={1}>1 — Global deaktiviert</option>
            <option value={2}>2 — Global erzwungen (AEAD / TLS 1.3)</option>
          </select>
        </div>

        <div className="form-row-2">
          <div className="form-field">
            <label>Standard-Servergruppe</label>
            <select
              value={srvDefaultSg}
              onChange={(e) => setSrvDefaultSg(Number(e.target.value))}
            >
              {state.serverGroups.map((sg) => (
                <option key={sg.sgid} value={sg.sgid}>
                  {sg.name} (#{sg.sgid})
                </option>
              ))}
            </select>
          </div>

          <div className="form-field">
            <label>Standard-Channelgruppe</label>
            <select
              value={srvDefaultCg}
              onChange={(e) => setSrvDefaultCg(Number(e.target.value))}
            >
              {state.channelGroups.map((cg) => (
                <option key={cg.cgid} value={cg.cgid}>
                  {cg.name} (#{cg.cgid})
                </option>
              ))}
            </select>
          </div>
        </div>
      </div>

      <div
        style={{
          display: 'grid',
          gridTemplateColumns: 'repeat(3, 1fr)',
          gap: '10px',
        }}
      >
        <div className="form-field">
          <label style={{ fontSize: '11px' }}>Anti-Flood Punkte-Abbau / Tick</label>
          <input
            type="number"
            value={antifloodReduce}
            onChange={(e) => setAntifloodReduce(Number(e.target.value))}
          />
        </div>
        <div className="form-field">
          <label style={{ fontSize: '11px' }}>Anti-Flood Befehls-Sperre ab</label>
          <input
            type="number"
            value={antifloodCmdBlock}
            onChange={(e) => setAntifloodCmdBlock(Number(e.target.value))}
          />
        </div>
        <div className="form-field">
          <label style={{ fontSize: '11px' }}>Anti-Flood IP-Sperre ab</label>
          <input
            type="number"
            value={antifloodIpBlock}
            onChange={(e) => setAntifloodIpBlock(Number(e.target.value))}
          />
        </div>
      </div>

      <button
        type="submit"
        className="btn-primary-sm"
        disabled={state.connectionState !== 'connected'}
      >
        <Save size={14} /> Alle Server-Einstellungen speichern
      </button>
    </form>
  );
};
