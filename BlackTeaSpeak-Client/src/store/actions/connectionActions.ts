import { soundCues } from '../../media/SoundCueEngine';
import { StoreContext } from '../state/blackTeaStateTypes';

export async function connectToServer(ctx: StoreContext): Promise<void> {
  const state = ctx.getState();
  if (state.connectionState === 'connecting') return;
  localStorage.setItem('blackteaweb_nick', state.nickname);

  ctx.patchState({
    connectionState: 'connecting',
    connectionError: null,
    channels: [],
    clients: [],
  });

  try {
    await ctx.protocol.connectAndHandshake({
      host: state.host,
      port: state.port,
      certHash: state.certHash,
      nickname: state.nickname,
      serverPassword: state.serverPassword,
      identityPublicKey: state.identityPublicKey,
    });

    ctx.patchState({ connectionState: 'connected' });
    soundCues.play('connected');
    ctx.appendSystemChat(
      `Verbunden mit ${ctx.getState().serverInfo.virtualserver_name} über HTTP/3 WebTransport (QUIC).`
    );

    const activeIdentity = ctx
      .getState()
      .desktopIdentities.find((i) => i.id === ctx.getState().activeIdentityId);
    if (activeIdentity) {
      await ctx.voiceEngine.setAudioProcessingConfig({
        echoCancellation: activeIdentity.echo_cancellation,
        noiseSuppression: activeIdentity.noise_suppression,
        autoGainControl: activeIdentity.auto_gain_control,
        inputDevice: activeIdentity.audio_input_device,
        outputDevice: activeIdentity.audio_output_device,
      });
    }

    ctx.voiceEngine.startMicrophone().catch((err) => {
      ctx.appendSystemChat(
        `Hinweis: Mikrofon konnte nicht gestartet werden (${err?.message || 'Kein Gerät'}). Empfang aktiv.`
      );
    });

    ctx.protocol.sendCommand('servergrouplist').catch(() => {});
    ctx.protocol.sendCommand('channelgrouplist').catch(() => {});
    ctx.protocol.sendCommand('permissionlist').catch(() => {});
    ctx.protocol.sendCommand('mbotlist').catch(() => {});
  } catch (err: any) {
    const msg = err?.message || String(err);
    soundCues.play('error');
    ctx.patchState({
      connectionState: 'error',
      connectionError: msg,
    });
    ctx.appendSystemChat(`Verbindungsfehler: ${msg}`);
  }
}

export function disconnectFromServer(ctx: StoreContext): void {
  ctx.voiceEngine.dispose();
  ctx.videoEngine.dispose();
  ctx.protocol.disconnect();
  soundCues.play('disconnected');
  ctx.patchState({
    connectionState: 'disconnected',
    cameraSharing: false,
    screenSharing: false,
    activeVideoStreams: {},
  });
  ctx.appendSystemChat('Verbindung getrennt.');
}

export async function moveSelfToChannel(
  ctx: StoreContext,
  cid: number,
  password = ''
): Promise<void> {
  const state = ctx.getState();
  if (state.connectionState !== 'connected') return;
  const prevCid = state.currentChannelId;
  try {
    await ctx.protocol.sendCommand('clientmove', [
      {
        clid: String(state.selfClientId),
        cid: String(cid),
        cpw: password,
      },
    ]);
    ctx.patchState({ currentChannelId: cid, selectedChannelId: cid });
    if (prevCid !== cid && !ctx.getState().outputDeafened) {
      soundCues.play('channel_switched');
    }
  } catch (err: any) {
    soundCues.play('error');
    ctx.showToast(`Channel-Wechsel fehlgeschlagen: ${err.message}`);
  }
}

export async function sendChatMessageAction(
  ctx: StoreContext,
  text: string
): Promise<void> {
  const state = ctx.getState();
  const trimmed = text.trim();
  if (!trimmed || state.connectionState !== 'connected') return;

  if (trimmed.toLowerCase().startsWith('/raw ')) {
    await executeRawCommandLineAction(ctx, trimmed.slice(5));
    return;
  }

  const tab = state.activeChatTab;
  const targetmode: 1 | 2 | 3 = tab === 'private' ? 1 : tab === 'channel' ? 2 : 3;
  const target =
    targetmode === 1
      ? String(state.privateChatTargetClientId ?? state.selectedClientId ?? 0)
      : targetmode === 2
      ? String(state.currentChannelId)
      : '1';

  try {
    await ctx.protocol.sendCommand('sendtextmessage', [
      {
        targetmode: String(targetmode),
        target,
        msg: trimmed,
      },
    ]);
    if (!ctx.getState().outputDeafened) {
      soundCues.play('chat_message_outbound');
    }
  } catch (err: any) {
    soundCues.play('error');
    ctx.showToast(`Nachricht fehlgeschlagen: ${err.message}`);
  }
}

export async function executeRawCommandLineAction(
  ctx: StoreContext,
  rawLine: string
): Promise<void> {
  const parts = rawLine.trim().split(/\s+/);
  const cmd = parts[0];
  if (!cmd) return;
  const row: Record<string, string> = {};
  const flags: string[] = [];
  for (let i = 1; i < parts.length; i++) {
    const token = parts[i];
    if (token.startsWith('-')) {
      flags.push(token);
    } else {
      const eq = token.indexOf('=');
      if (eq !== -1) {
        row[token.slice(0, eq)] = token.slice(eq + 1).replace(/\\s/g, ' ');
      }
    }
  }
  try {
    const frames = await ctx.protocol.sendCommand(cmd, [row], flags);
    ctx.appendSystemChat(
      `Befehl /${cmd} ausgeführt (${frames.length} Antwort-Frames empfangen).`
    );
  } catch (err: any) {
    ctx.appendSystemChat(`Befehl /${cmd} Fehler: ${err.message}`);
  }
}
