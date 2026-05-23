import {Registry} from "tc-shared/events";
import {spawnReactModal} from "tc-shared/ui/react-elements/modal";
import {ConnectionHandler} from "tc-shared/ConnectionHandler";
import {CommandResult} from "tc-shared/connection/ServerConnectionDeclaration";
import {VoiceConnectionStatus} from "tc-shared/connection/VoiceConnection";
import {GroupTarget, GroupType} from "tc-shared/permission/GroupManager";
import {LogCategory, logWarn} from "tc-shared/log";
import {tr} from "tc-shared/i18n/localize";
import {
    cloneWhisperTarget,
    WhisperTarget,
    WhisperTargetCustom
} from "tc-shared/voice/VoiceWhisper";
import {
    WhisperTargetDraft,
    WhisperTargetModalClient,
    WhisperTargetModalEvents,
    WhisperTargetModalGroup,
    WhisperTargetModalState,
    WhisperTargetModalVoiceState
} from "tc-shared/ui/modal/whisper-targets/Definitions";
import {ModalWhisperTargets} from "tc-shared/ui/modal/whisper-targets/Renderer";

function createEmptyDraft() : WhisperTargetDraft {
    return {
        echoSelf: false,
        currentChannel: false,
        clientIds: [],
        groupIds: []
    };
}

function cloneDraft(draft: WhisperTargetDraft) : WhisperTargetDraft {
    return {
        echoSelf: draft.echoSelf,
        currentChannel: draft.currentChannel,
        clientIds: draft.clientIds.slice(0),
        groupIds: draft.groupIds.slice(0)
    };
}

function draftFromTarget(target: WhisperTarget | undefined, currentChannelId?: number) : WhisperTargetDraft {
    const draft = createEmptyDraft();
    if(!target) {
        return draft;
    }

    switch (target.target) {
        case "echo":
            draft.echoSelf = true;
            break;

        case "channel-clients":
            draft.currentChannel = typeof currentChannelId === "number" && target.channels.indexOf(currentChannelId) !== -1;
            draft.clientIds = target.clients.slice(0);
            break;

        case "groups":
            draft.groupIds = target.groups.slice(0);
            break;

        case "custom":
            draft.echoSelf = target.echo === true;
            draft.currentChannel = typeof currentChannelId === "number" && target.channels.indexOf(currentChannelId) !== -1;
            draft.clientIds = target.clients.slice(0);
            draft.groupIds = target.groups.slice(0);
            break;
    }

    return draft;
}

function buildTarget(draft: WhisperTargetDraft, currentChannelId?: number) : WhisperTargetCustom | undefined {
    const result: WhisperTargetCustom = {
        target: "custom",
        echo: draft.echoSelf,
        channels: draft.currentChannel && typeof currentChannelId === "number" ? [ currentChannelId ] : [],
        clients: draft.clientIds.slice(0),
        groups: draft.groupIds.slice(0)
    };

    if(result.echo !== true && result.channels.length === 0 && result.clients.length === 0 && result.groups.length === 0) {
        return undefined;
    }

    return result;
}

function resolveVoiceState(connection: ConnectionHandler) : { state: WhisperTargetModalVoiceState, message?: string } {
    const voiceConnection = connection.getServerConnection().getVoiceConnection();
    switch (voiceConnection.getConnectionState()) {
        case VoiceConnectionStatus.Connected:
            return { state: "connected" };

        case VoiceConnectionStatus.Disconnected:
        case VoiceConnectionStatus.Disconnecting:
            return { state: "disconnected" };

        case VoiceConnectionStatus.Connecting:
            return { state: "connecting" };

        case VoiceConnectionStatus.ClientUnsupported:
            return { state: "unsupported-client" };

        case VoiceConnectionStatus.ServerUnsupported:
            return { state: "unsupported-server" };

        case VoiceConnectionStatus.Failed:
            return {
                state: "failed",
                message: voiceConnection.getFailedMessage()
            };
    }
}

class WhisperTargetsController {
    readonly events = new Registry<WhisperTargetModalEvents>();

    private readonly connection: ConnectionHandler;
    private readonly listeners: (() => void)[] = [];
    private readonly state: WhisperTargetModalState = {
        voiceState: "loading",
        currentChannel: undefined,
        clients: [],
        groups: [],
        selection: createEmptyDraft(),
        activeTarget: undefined,
        startPending: false,
        lastError: undefined
    };

    constructor(connection: ConnectionHandler) {
        this.connection = connection;
        this.initialize();
    }

    destroy() {
        this.listeners.forEach(listener => listener());
        this.listeners.length = 0;
        this.events.destroy();
    }

    private initialize() {
        const voiceConnection = this.connection.getServerConnection().getVoiceConnection();
        const refreshAvailability = () => {
            this.refreshAvailability();
            this.publishState();
        };

        this.refreshVoiceState();
        this.refreshAvailability();
        this.state.activeTarget = cloneWhisperTarget(voiceConnection.getWhisperTarget());
        this.state.selection = draftFromTarget(this.state.activeTarget, this.state.currentChannel?.channelId);
        this.publishState();

        this.listeners.push(this.events.on("query_state", () => this.publishState()));
        this.listeners.push(this.events.on("action_toggle_echo", event => {
            this.state.selection.echoSelf = event.enabled;
            this.state.lastError = undefined;
            this.publishState();
        }));
        this.listeners.push(this.events.on("action_toggle_current_channel", event => {
            this.state.selection.currentChannel = event.enabled;
            this.state.lastError = undefined;
            this.publishState();
        }));
        this.listeners.push(this.events.on("action_toggle_client", event => {
            this.toggleSelection(this.state.selection.clientIds, event.clientId, event.enabled);
            this.state.lastError = undefined;
            this.publishState();
        }));
        this.listeners.push(this.events.on("action_toggle_group", event => {
            this.toggleSelection(this.state.selection.groupIds, event.groupId, event.enabled);
            this.state.lastError = undefined;
            this.publishState();
        }));
        this.listeners.push(this.events.on("action_start_whisper", () => this.startWhisper()));
        this.listeners.push(this.events.on("action_stop_whisper", () => {
            this.state.lastError = undefined;
            voiceConnection.stopWhisper();
            this.publishState();
        }));

        this.listeners.push(voiceConnection.events.on("notify_connection_status_changed", () => {
            this.refreshVoiceState();
            this.publishState();
        }));
        this.listeners.push(voiceConnection.events.on("notify_whisper_target_changed", event => {
            this.state.activeTarget = cloneWhisperTarget(event.newTarget);
            this.publishState();
        }));

        this.listeners.push(this.connection.channelTree.events.on("notify_client_enter_view", refreshAvailability));
        this.listeners.push(this.connection.channelTree.events.on("notify_client_leave_view", refreshAvailability));
        this.listeners.push(this.connection.channelTree.events.on("notify_client_moved", refreshAvailability));
        this.listeners.push(this.connection.channelTree.events.on("notify_channel_client_order_changed", refreshAvailability));
        this.listeners.push(this.connection.channelTree.events.on("notify_channel_list_received", refreshAvailability));

        this.listeners.push(this.connection.groups.events.on("notify_groups_received", refreshAvailability));
        this.listeners.push(this.connection.groups.events.on("notify_groups_created", refreshAvailability));
        this.listeners.push(this.connection.groups.events.on("notify_groups_deleted", refreshAvailability));
        this.listeners.push(this.connection.groups.events.on("notify_groups_updated", refreshAvailability));
    }

    private toggleSelection(values: number[], value: number, enabled: boolean) {
        const index = values.indexOf(value);
        if(enabled) {
            if(index === -1) {
                values.push(value);
                values.sort((left, right) => left - right);
            }
        } else if(index !== -1) {
            values.splice(index, 1);
        }
    }

    private refreshVoiceState() {
        const voiceState = resolveVoiceState(this.connection);
        this.state.voiceState = voiceState.state;
        this.state.voiceMessage = voiceState.message;
    }

    private refreshAvailability() {
        const localClient = this.connection.getClient();
        const currentChannel = localClient?.currentChannel();

        this.state.currentChannel = currentChannel ? {
            channelId: currentChannel.getChannelId(),
            channelName: currentChannel.channelName()
        } : undefined;

        this.state.clients = currentChannel && localClient
            ? currentChannel.channelClientsOrdered()
                .filter(client => client.clientId() !== localClient.clientId())
                .map(client => ({
                    clientId: client.clientId(),
                    nickname: client.clientNickName()
                }))
            : [];

        this.state.groups = this.connection.groups.serverGroups
            .filter(group => group.target === GroupTarget.SERVER && group.type === GroupType.NORMAL)
            .map(group => ({
                groupId: group.id,
                name: group.name
            }));

        const availableClientIds = new Set(this.state.clients.map((client: WhisperTargetModalClient) => client.clientId));
        this.state.selection.clientIds = this.state.selection.clientIds.filter(clientId => availableClientIds.has(clientId));

        const availableGroupIds = new Set(this.state.groups.map((group: WhisperTargetModalGroup) => group.groupId));
        this.state.selection.groupIds = this.state.selection.groupIds.filter(groupId => availableGroupIds.has(groupId));

        if(!this.state.currentChannel) {
            this.state.selection.currentChannel = false;
        }
    }

    private publishState() {
        this.events.fire_react("notify_state", {
            state: {
                voiceState: this.state.voiceState,
                voiceMessage: this.state.voiceMessage,
                currentChannel: this.state.currentChannel ? { ...this.state.currentChannel } : undefined,
                clients: this.state.clients.slice(0),
                groups: this.state.groups.slice(0),
                selection: cloneDraft(this.state.selection),
                activeTarget: cloneWhisperTarget(this.state.activeTarget),
                startPending: this.state.startPending,
                lastError: this.state.lastError
            }
        });
    }

    private async startWhisper() {
        const target = buildTarget(this.state.selection, this.state.currentChannel?.channelId);
        if(!target) {
            this.state.lastError = tr("Select at least one whisper target.");
            this.publishState();
            return;
        }

        this.state.startPending = true;
        this.state.lastError = undefined;
        this.publishState();

        try {
            await this.connection.getServerConnection().getVoiceConnection().startWhisper(target);
        } catch (error) {
            if(error instanceof CommandResult) {
                this.state.lastError = error.formattedMessage();
            } else if(error instanceof Error) {
                this.state.lastError = error.message;
            } else if(typeof error === "string") {
                this.state.lastError = error;
            } else {
                logWarn(LogCategory.VOICE, tr("Failed to start whisper target selection: %o"), error);
                this.state.lastError = tr("Failed to start whisper");
            }
        } finally {
            this.state.startPending = false;
            this.publishState();
        }
    }
}

export function spawnWhisperTargetsModal(connection: ConnectionHandler) {
    const controller = new WhisperTargetsController(connection);
    const modal = spawnReactModal(ModalWhisperTargets, controller.events);

    controller.events.on("action_close", () => modal.destroy());

    modal.getEvents().on("close", () => controller.events.fire_react("notify_close"));
    modal.getEvents().on("destroy", () => {
        controller.events.fire("notify_destroy");
        controller.destroy();
    });

    modal.show();
}