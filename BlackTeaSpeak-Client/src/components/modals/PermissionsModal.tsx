import React, { useEffect, useState } from 'react';
import { Shield, Search, Plus, Trash2, X } from 'lucide-react';
import { blackTeaStore, useBlackTeaStore } from '../../store/useBlackTeaStore';

type PermTier = 'servergroup' | 'channelgroup' | 'channel' | 'client' | 'channelclient';

export const PermissionsModal: React.FC<{ onClose: () => void }> = ({ onClose }) => {
  const state = useBlackTeaStore();
  const [tier, setTier] = useState<PermTier>('servergroup');
  const [targetId1, setTargetId1] = useState<number>(state.serverGroups[0]?.sgid ?? 6);
  const [targetId2, setTargetId2] = useState<number>(
    state.selfDatabaseId || state.clients[0]?.client_database_id || 1
  );
  const [search, setSearch] = useState('');
  const [onlyAssigned, setOnlyAssigned] = useState(false);
  const [newPermSid, setNewPermSid] = useState('i_channel_join_power');
  const [newPermVal, setNewPermVal] = useState(75);
  const [newPermSkip, setNewPermSkip] = useState(false);
  const [newPermNegated, setNewPermNegated] = useState(false);

  useEffect(() => {
    blackTeaStore.loadPermissionsData(tier, targetId1, targetId2).catch(() => {});
  }, [tier, targetId1, targetId2]);

  const handleAssign = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!newPermSid.trim()) return;
    await blackTeaStore.setPermissionValue(
      tier,
      targetId1,
      targetId2,
      newPermSid.trim(),
      newPermVal,
      newPermSkip,
      newPermNegated
    );
  };

  const assignedMap = new Map(state.inspectedPermissions.map((p) => [p.permsid, p]));
  const baseCatalog =
    state.permissionCatalog.length > 0
      ? state.permissionCatalog
      : state.inspectedPermissions;
  const combinedList = baseCatalog.filter((c) => {
    const sid = (c.permsid || '').toLowerCase();
    const desc = (c.permdesc || '').toLowerCase();
    const matchesSearch =
      !search.trim() ||
      sid.includes(search.toLowerCase()) ||
      desc.includes(search.toLowerCase());
    if (!matchesSearch) return false;
    if (onlyAssigned && !assignedMap.has(c.permsid)) return false;
    return true;
  });

  const grantPermNameFor = (permsid: string): string | null => {
    const idx = permsid.indexOf('_');
    if (idx <= 0) return null;
    if (permsid.startsWith('i_needed_modify_power_')) return null;
    return `i_needed_modify_power_${permsid.slice(idx + 1)}`;
  };

  const onlineClientsWithDbid = state.clients.filter((c) => c.client_database_id > 0);

  return (
    <div className="modal-card wide">
      <div className="modal-header">
        <div className="modal-title">
          <Shield size={18} />
          <span>5-Tier Permission-Editor (TS3 &amp; TeaSpeak kompatibel)</span>
        </div>
        <button className="btn-icon-sm" onClick={onClose}>
          <X size={16} />
        </button>
      </div>

      <div className="modal-tabs">
        {(
          [
            ['servergroup', '1. Servergruppen'],
            ['channelgroup', '2. Channelgruppen'],
            ['channel', '3. Channel-Rechte'],
            ['client', '4. Client-Rechte'],
            ['channelclient', '5. Channel-Client'],
          ] as [PermTier, string][]
        ).map(([key, label]) => (
          <button
            key={key}
            className={`modal-tab ${tier === key ? 'active' : ''}`}
            onClick={() => {
              setTier(key);
              if (key === 'servergroup') setTargetId1(state.serverGroups[0]?.sgid ?? 6);
              if (key === 'channelgroup') setTargetId1(state.channelGroups[0]?.cgid ?? 5);
              if (key === 'channel') setTargetId1(state.currentChannelId || 1);
              if (key === 'client') {
                setTargetId1(
                  state.selfDatabaseId || state.clients[0]?.client_database_id || 1
                );
              }
              if (key === 'channelclient') {
                setTargetId1(state.currentChannelId || 1);
                setTargetId2(
                  state.selfDatabaseId || state.clients[0]?.client_database_id || 1
                );
              }
            }}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="modal-body">
        <div className="perm-toolbar">
          <div className="perm-target-selectors">
            {tier === 'servergroup' && (
              <select
                value={targetId1}
                onChange={(e) => setTargetId1(Number(e.target.value))}
              >
                {state.serverGroups.map((g) => (
                  <option key={g.sgid} value={g.sgid}>
                    Servergruppe: {g.name} (#{g.sgid})
                  </option>
                ))}
              </select>
            )}

            {tier === 'channelgroup' && (
              <select
                value={targetId1}
                onChange={(e) => setTargetId1(Number(e.target.value))}
              >
                {state.channelGroups.map((g) => (
                  <option key={g.cgid} value={g.cgid}>
                    Channelgruppe: {g.name} (#{g.cgid})
                  </option>
                ))}
              </select>
            )}

            {(tier === 'channel' || tier === 'channelclient') && (
              <select
                value={targetId1}
                onChange={(e) => setTargetId1(Number(e.target.value))}
              >
                {state.channels.map((ch) => (
                  <option key={ch.cid} value={ch.cid}>
                    Channel: {ch.channel_name} (#{ch.cid})
                  </option>
                ))}
              </select>
            )}

            {(tier === 'client' || tier === 'channelclient') && (
              <>
                {onlineClientsWithDbid.length > 0 && (
                  <select
                    value={tier === 'client' ? targetId1 : targetId2}
                    onChange={(e) =>
                      tier === 'client'
                        ? setTargetId1(Number(e.target.value))
                        : setTargetId2(Number(e.target.value))
                    }
                  >
                    {onlineClientsWithDbid.map((c) => (
                      <option key={c.clid} value={c.client_database_id}>
                        Client: {c.client_nickname} (DBID #{c.client_database_id})
                      </option>
                    ))}
                  </select>
                )}
                <input
                  type="number"
                  value={tier === 'client' ? targetId1 : targetId2}
                  onChange={(e) =>
                    tier === 'client'
                      ? setTargetId1(Number(e.target.value))
                      : setTargetId2(Number(e.target.value))
                  }
                  placeholder="Client DBID"
                  title="Client Database ID (cldbid)"
                  className="small-num"
                />
              </>
            )}

            <label className="chk-label" style={{ marginLeft: 6 }}>
              <input
                type="checkbox"
                checked={onlyAssigned}
                onChange={(e) => setOnlyAssigned(e.target.checked)}
              />
              Nur aktive ({state.inspectedPermissions.length})
            </label>
          </div>

          <div className="perm-search-box">
            <Search size={14} />
            <input
              type="text"
              placeholder="Permission filtern (z.B. b_virtualserver, i_channel)..."
              value={search}
              onChange={(e) => setSearch(e.target.value)}
            />
          </div>
        </div>

        <form className="perm-assign-form" onSubmit={handleAssign}>
          <input
            type="text"
            value={newPermSid}
            onChange={(e) => setNewPermSid(e.target.value)}
            placeholder="Permission Name (z.B. i_client_kick_from_server_power)"
          />
          <input
            type="number"
            value={newPermVal}
            onChange={(e) => setNewPermVal(Number(e.target.value))}
            placeholder="Wert"
            className="small-num"
          />
          <label className="chk-label">
            <input
              type="checkbox"
              checked={newPermSkip}
              onChange={(e) => setNewPermSkip(e.target.checked)}
            />
            Skip
          </label>
          <label className="chk-label">
            <input
              type="checkbox"
              checked={newPermNegated}
              onChange={(e) => setNewPermNegated(e.target.checked)}
            />
            Negated
          </label>
          <button type="submit" className="btn-primary-sm">
            <Plus size={14} /> Setzen
          </button>
        </form>

        <div className="perm-table-wrap">
          <table className="data-table">
            <thead>
              <tr>
                <th>Permission (permsid)</th>
                <th>Wert</th>
                <th>Skip</th>
                <th>Negated</th>
                <th>Grant (Vergaberecht)</th>
                <th>Aktion</th>
              </tr>
            </thead>
            <tbody>
              {combinedList.slice(0, 200).map((item) => {
                const assigned = assignedMap.get(item.permsid);
                const grantName = grantPermNameFor(item.permsid);
                const grantAssigned = grantName ? assignedMap.get(grantName) : undefined;
                return (
                  <tr
                    key={item.permsid}
                    className={assigned ? 'row-assigned' : ''}
                    onClick={() => {
                      setNewPermSid(item.permsid);
                      if (assigned) {
                        setNewPermVal(assigned.permvalue);
                        setNewPermSkip(assigned.permskip);
                        setNewPermNegated(assigned.permnegated);
                      } else if (item.permsid.startsWith('b_')) {
                        setNewPermVal(1);
                      }
                    }}
                  >
                    <td>
                      <code>{item.permsid}</code>
                      {item.permdesc && (
                        <div style={{ fontSize: 11, color: '#64748b', marginTop: 2 }}>
                          {item.permdesc}
                        </div>
                      )}
                    </td>
                    <td>
                      {assigned ? (
                        <strong className="val-active">{assigned.permvalue}</strong>
                      ) : (
                        <span className="muted-note">—</span>
                      )}
                    </td>
                    <td>{assigned?.permskip ? 'Ja' : '—'}</td>
                    <td>{assigned?.permnegated ? 'Ja' : '—'}</td>
                    <td>
                      {grantAssigned ? (
                        <strong className="val-active">{grantAssigned.permvalue}</strong>
                      ) : grantName ? (
                        <button
                          type="button"
                          className="btn-xs"
                          onClick={(e) => {
                            e.stopPropagation();
                            setNewPermSid(grantName);
                            setNewPermVal(75);
                          }}
                          title={`Grant-Recht (${grantName}) auswählen`}
                        >
                          + Grant
                        </button>
                      ) : (
                        <span className="muted-note">—</span>
                      )}
                    </td>
                    <td>
                      {assigned && (
                        <button
                          className="btn-icon-xs danger"
                          onClick={(e) => {
                            e.stopPropagation();
                            blackTeaStore.removePermissionValue(
                              tier,
                              targetId1,
                              targetId2,
                              item.permsid
                            );
                          }}
                          title="Permission entfernen"
                        >
                          <Trash2 size={13} />
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
