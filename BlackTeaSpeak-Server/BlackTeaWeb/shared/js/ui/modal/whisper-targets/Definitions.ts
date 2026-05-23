import {WhisperTarget} from "tc-shared/voice/VoiceWhisper";

export type WhisperTargetModalVoiceState =
    "loading"
    | "connecting"
    | "connected"
    | "disconnected"
    | "unsupported-client"
    | "unsupported-server"
    | "failed";

export interface WhisperTargetDraft {
    echoSelf: boolean,
    currentChannel: boolean,
    clientIds: number[],
    groupIds: number[]
}

export interface WhisperTargetModalChannel {
    channelId: number,
    channelName: string
}

export interface WhisperTargetModalClient {
    clientId: number,
    nickname: string
}

export interface WhisperTargetModalGroup {
    groupId: number,
    name: string
}

export interface WhisperTargetModalState {
    voiceState: WhisperTargetModalVoiceState,
    voiceMessage?: string,
    currentChannel?: WhisperTargetModalChannel,
    clients: WhisperTargetModalClient[],
    groups: WhisperTargetModalGroup[],
    selection: WhisperTargetDraft,
    activeTarget?: WhisperTarget,
    startPending: boolean,
    lastError?: string
}

export interface WhisperTargetModalEvents {
    query_state: {},
    action_close: {},
    action_start_whisper: {},
    action_stop_whisper: {},
    action_toggle_echo: { enabled: boolean },
    action_toggle_current_channel: { enabled: boolean },
    action_toggle_client: { clientId: number, enabled: boolean },
    action_toggle_group: { groupId: number, enabled: boolean },

    notify_state: { state: WhisperTargetModalState },
    notify_destroy: {},
    notify_close: {}
}