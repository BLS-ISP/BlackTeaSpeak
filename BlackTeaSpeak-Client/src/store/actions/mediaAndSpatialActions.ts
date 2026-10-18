import { soundCues, SoundPackId } from '../../media/SoundCueEngine';
import { SpatialPosition } from '../../types/protocol';
import { StoreContext } from '../state/blackTeaStateTypes';

export function toggleInputMuteAction(ctx: StoreContext): void {
  const state = ctx.getState();
  const next = !state.inputMuted;
  ctx.voiceEngine.setInputMuted(next);
  ctx.patchState({ inputMuted: next });
  soundCues.play(next ? 'mic_muted' : 'mic_unmuted');
  if (state.connectionState === 'connected') {
    ctx.protocol
      .sendCommand('clientupdate', [{ client_input_muted: next ? '1' : '0' }])
      .catch(() => {});
  }
}

export function toggleOutputDeafenAction(ctx: StoreContext): void {
  const state = ctx.getState();
  const next = !state.outputDeafened;
  soundCues.play(next ? 'sound_muted' : 'sound_unmuted');
  ctx.voiceEngine.setOutputDeafened(next);
  ctx.patchState({ outputDeafened: next });
  if (state.connectionState === 'connected') {
    ctx.protocol
      .sendCommand('clientupdate', [{ client_output_muted: next ? '1' : '0' }])
      .catch(() => {});
  }
}

export function cycleSoundPackAction(ctx: StoreContext): void {
  const state = ctx.getState();
  const order: SoundPackId[] = ['default', 'default_male', 'none'];
  const idx = order.indexOf(state.soundPack);
  const next = order[(idx + 1) % order.length];
  soundCues.setSoundPack(next);
  ctx.patchState({ soundPack: next });
  if (next === 'default') {
    ctx.showToast('Voice Pack: TeamSpeak 3 Female');
    soundCues.play('sound_unmuted');
  } else if (next === 'default_male') {
    ctx.showToast('Voice Pack: TeamSpeak 3 Male');
    soundCues.play('sound_unmuted');
  } else {
    ctx.showToast('Voice Pack: Stummgeschaltet');
  }
}

export function setTransmissionModeAction(
  ctx: StoreContext,
  mode: 'vad' | 'ptt',
  thresholdDb?: number
): void {
  const db = thresholdDb ?? ctx.getState().vadThresholdDb;
  ctx.voiceEngine.setTransmissionMode(mode, db);
  ctx.patchState({ transmissionMode: mode, vadThresholdDb: db });
}

export function setPttPressedAction(ctx: StoreContext, pressed: boolean): void {
  const state = ctx.getState();
  if (state.pttPressed !== pressed && !state.inputMuted && !state.outputDeafened) {
    soundCues.play(pressed ? 'ptt_down' : 'ptt_up');
  }
  ctx.voiceEngine.setPttPressed(pressed);
  ctx.patchState({ pttPressed: pressed });
}

export function toggleWhisperModeAction(ctx: StoreContext): void {
  const next = !ctx.getState().whisperMode;
  ctx.voiceEngine.setWhisperMode(next);
  ctx.patchState({ whisperMode: next });
  soundCues.play(next ? 'whisper_on' : 'whisper_off');
  ctx.showToast(next ? 'Whisper-Modus aktiviert' : 'Normaler Channel-Voice-Modus');
}

import {
  syncSpatialConfigToTauri,
  syncSpatialPositionToTauri,
} from '../../protocols/tauriBridge';

export function toggleSpatial3DAction(ctx: StoreContext): void {
  const state = ctx.getState();
  const next = !state.telemetry.spatial3DEnabled;
  ctx.voiceEngine.setSpatial3DEnabled(next);
  syncSpatialConfigToTauri(next);
  ctx.patchState({
    telemetry: {
      ...state.telemetry,
      spatial3DEnabled: next,
    },
  });
  ctx.showToast(
    next ? '3D HRTF Spatial Audio aktiviert' : '3D HRTF Spatial Audio deaktiviert (Stereo)'
  );
}

export function updateClientSpatialPositionAction(
  ctx: StoreContext,
  clientId: number,
  pos: SpatialPosition
): void {
  const state = ctx.getState();
  if (clientId === state.selfClientId) {
    ctx.patchState({ selfSpatialPos: pos });
    ctx.voiceEngine.broadcastSelfSpatialPosition(pos);
    syncSpatialPositionToTauri(clientId, pos, true);
  } else {
    ctx.voiceEngine.updatePeerSpatialPosition(clientId, pos);
    syncSpatialPositionToTauri(clientId, pos, false);
    ctx.patchState({
      clients: state.clients.map((c) =>
        c.clid === clientId ? { ...c, spatialPos: pos } : c
      ),
    });
  }
}

export async function toggleCameraShareAction(ctx: StoreContext): Promise<void> {
  if (ctx.getState().cameraSharing) {
    ctx.videoEngine.stopCameraShare();
    ctx.patchState({ cameraSharing: false });
    ctx.protocol
      .sendCommand('broadcastvideo', [{ active: '0', kind: 'camera' }])
      .catch(() => {});
  } else {
    try {
      await ctx.videoEngine.startCameraShare(1280, 720, 30);
      ctx.patchState({ cameraSharing: true });
      ctx.protocol
        .sendCommand('broadcastvideo', [{ active: '1', kind: 'camera' }])
        .catch(() => {});
      ctx.showToast('WebCodecs Kamera-Stream gestartet (HTTP/3 QUIC)');
    } catch (err: any) {
      ctx.showToast(`Kamera konnte nicht gestartet werden: ${err.message}`);
    }
  }
}

export async function toggleScreenShareAction(ctx: StoreContext): Promise<void> {
  if (ctx.getState().screenSharing) {
    ctx.videoEngine.stopScreenShare();
    ctx.patchState({ screenSharing: false });
    ctx.protocol
      .sendCommand('broadcastvideo', [{ active: '0', kind: 'screen' }])
      .catch(() => {});
  } else {
    try {
      await ctx.videoEngine.startScreenShare(1920, 1080, 30, () => {
        ctx.patchState({ screenSharing: false });
        ctx.protocol
          .sendCommand('broadcastvideo', [{ active: '0', kind: 'screen' }])
          .catch(() => {});
      });
      ctx.patchState({ screenSharing: true });
      ctx.protocol
        .sendCommand('broadcastvideo', [{ active: '1', kind: 'screen' }])
        .catch(() => {});
      ctx.showToast('WebCodecs Screen-Sharing gestartet (HTTP/3 QUIC)');
    } catch (err: any) {
      ctx.showToast(`Screen-Share abgebrochen: ${err.message}`);
    }
  }
}

export function setPeerVolumeAction(
  ctx: StoreContext,
  clientId: number,
  gain: number
): void {
  const state = ctx.getState();
  ctx.voiceEngine.setPeerVolume(clientId, gain);
  const pct = Math.max(0, Math.min(200, Math.round(gain * 100)));
  ctx.patchState({
    clients: state.clients.map((c) =>
      c.clid === clientId ? { ...c, volumeGain: gain } : c
    ),
    musicBots: state.musicBots.map((b) =>
      b.client_id === clientId ? { ...b, volume: Math.min(100, pct) } : b
    ),
  });
}

export function togglePeerMuteAction(ctx: StoreContext, clientId: number): void {
  const state = ctx.getState();
  const target = state.clients.find((c) => c.clid === clientId);
  const nextMuted = !target?.localMuted;
  ctx.voiceEngine.setPeerMuted(clientId, nextMuted);
  ctx.patchState({
    clients: state.clients.map((c) =>
      c.clid === clientId ? { ...c, localMuted: nextMuted } : c
    ),
  });
}

export function setClientTalkingStateAction(
  ctx: StoreContext,
  clid: number,
  isTalking: boolean,
  isWhispering = false
): void {
  if (!clid) return;

  const existingTimer = ctx.peerTalkTimers.get(clid);
  if (existingTimer !== undefined) {
    window.clearTimeout(existingTimer);
    ctx.peerTalkTimers.delete(clid);
  }

  const state = ctx.getState();
  if (isTalking && clid !== state.selfClientId) {
    const timer = window.setTimeout(() => {
      ctx.peerTalkTimers.delete(clid);
      const curState = ctx.getState();
      const cur = curState.clients.find((c) => c.clid === clid);
      if (cur && (cur.isTalking || cur.isWhispering)) {
        ctx.patchState({
          clients: curState.clients.map((c) =>
            c.clid === clid ? { ...c, isTalking: false, isWhispering: false } : c
          ),
        });
      }
    }, 350);
    ctx.peerTalkTimers.set(clid, timer);
  }

  const target = state.clients.find((c) => c.clid === clid);
  if (
    target &&
    (target.isTalking !== isTalking || target.isWhispering !== (isTalking && isWhispering))
  ) {
    ctx.patchState({
      clients: state.clients.map((c) =>
        c.clid === clid
          ? { ...c, isTalking, isWhispering: isTalking && isWhispering }
          : c
      ),
    });
  }
}

export function handleIncomingMediaAction(
  ctx: StoreContext,
  packetType: number,
  senderClientId: number,
  payload: Uint8Array
): void {
  if (packetType === 1 || packetType === 3) {
    if (payload.byteLength > 0) {
      ctx.setClientTalkingState(senderClientId, true, packetType === 3);
      ctx.voiceEngine.handleIncomingOpusPacket(senderClientId, payload);
    } else {
      ctx.setClientTalkingState(senderClientId, false, false);
    }
  } else if (packetType === 2 || packetType === 4) {
    ctx.videoEngine.handleIncomingVideoPacket(packetType, senderClientId, payload);
  } else if (packetType === 5 && payload.byteLength >= 12) {
    const view = new DataView(payload.buffer, payload.byteOffset, 12);
    const pos: SpatialPosition = {
      x: view.getFloat32(0, true),
      y: view.getFloat32(4, true),
      z: view.getFloat32(8, true),
    };
    ctx.voiceEngine.updatePeerSpatialPosition(senderClientId, pos);
    const state = ctx.getState();
    ctx.patchState({
      clients: state.clients.map((c) =>
        c.clid === senderClientId ? { ...c, spatialPos: pos } : c
      ),
    });
  }
}
