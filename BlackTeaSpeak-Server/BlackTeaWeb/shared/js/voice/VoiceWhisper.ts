import {Registry} from "../events";
import {VoicePlayer} from "../voice/VoicePlayer";

export interface WhisperTargetChannelClients {
    target: "channel-clients",

    channels: number[],
    clients: number[]
}

export interface WhisperTargetGroups {
    target: "groups",
    groups: number[]
}

export interface WhisperTargetCustom {
    target: "custom",

    echo?: boolean,
    channels: number[],
    clients: number[],
    groups: number[]
}

export interface WhisperTargetEcho {
    target: "echo",
}

export type WhisperTarget = WhisperTargetGroups | WhisperTargetChannelClients | WhisperTargetEcho | WhisperTargetCustom;

export function cloneWhisperTarget(target: WhisperTarget | undefined) : WhisperTarget | undefined {
    if(!target) {
        return undefined;
    }

    switch (target.target) {
        case "echo":
            return { target: "echo" };

        case "channel-clients":
            return {
                target: "channel-clients",
                channels: target.channels.slice(0),
                clients: target.clients.slice(0)
            };

        case "groups":
            return {
                target: "groups",
                groups: target.groups.slice(0)
            };

        case "custom":
            return {
                target: "custom",
                echo: target.echo === true,
                channels: target.channels.slice(0),
                clients: target.clients.slice(0),
                groups: target.groups.slice(0)
            };
    }
}

export function normalizeWhisperTarget(target: WhisperTarget | undefined) : WhisperTargetCustom | undefined {
    if(!target) {
        return undefined;
    }

    switch (target.target) {
        case "echo":
            return {
                target: "custom",
                echo: true,
                channels: [],
                clients: [],
                groups: []
            };

        case "channel-clients":
            return {
                target: "custom",
                echo: false,
                channels: target.channels.slice(0),
                clients: target.clients.slice(0),
                groups: []
            };

        case "groups":
            return {
                target: "custom",
                echo: false,
                channels: [],
                clients: [],
                groups: target.groups.slice(0)
            };

        case "custom":
            return {
                target: "custom",
                echo: target.echo === true,
                channels: target.channels.slice(0),
                clients: target.clients.slice(0),
                groups: target.groups.slice(0)
            };
    }
}

export function hasWhisperTargetSelection(target: WhisperTarget | undefined) : boolean {
    const normalized = normalizeWhisperTarget(target);
    if(!normalized) {
        return false;
    }

    return normalized.echo === true
        || normalized.channels.length > 0
        || normalized.clients.length > 0
        || normalized.groups.length > 0;
}

export interface WhisperSessionEvents {
    notify_state_changed: { oldState: WhisperSessionState, newState: WhisperSessionState },
    notify_blocked_state_changed: { oldState: boolean, newState: boolean },
    notify_timed_out: {}
}

export enum WhisperSessionState {
    /* the session is getting initialized, not all variables may be set */
    INITIALIZING,

    /* there is currently no whispering */
    PAUSED,

    /* we're replaying some whisper */
    PLAYING,

    /* Something in the initialize process went wrong. */
    INITIALIZE_FAILED
}

export const kUnknownWhisperClientUniqueId = "unknown";

export interface WhisperSession {
    readonly events: Registry<WhisperSessionEvents>;

    /* get information about the whisperer */
    getClientId() : number;

    /* only ensured to be valid if session has been initialized */
    getClientName() : string | undefined;

    /* only ensured to be valid if session has been initialized */
    getClientUniqueId() : string | undefined;

    getSessionState() : WhisperSessionState;

    isBlocked() : boolean;
    setBlocked(blocked: boolean);

    getSessionTimeout() : number;
    setSessionTimeout(timeout: number);

    getLastWhisperTimestamp() : number;

    /**
     * This is only valid if the session has been initialized successfully,
     * and it hasn't been blocked
     *
     * @returns Returns the voice player
     */
    getVoicePlayer() : VoicePlayer | undefined;
}