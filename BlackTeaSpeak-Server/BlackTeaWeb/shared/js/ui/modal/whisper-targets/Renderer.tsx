import * as React from "react";
import {useContext, useState} from "react";
import {Registry} from "tc-shared/events";
import {
    hasWhisperTargetSelection,
    normalizeWhisperTarget,
    WhisperTarget
} from "tc-shared/voice/VoiceWhisper";
import {
    WhisperTargetDraft,
    WhisperTargetModalClient,
    WhisperTargetModalEvents,
    WhisperTargetModalGroup,
    WhisperTargetModalState
} from "tc-shared/ui/modal/whisper-targets/Definitions";
import {InternalModal} from "tc-shared/ui/react-elements/modal/Definitions";
import {Translatable} from "tc-shared/ui/react-elements/i18n";
import {Checkbox} from "tc-shared/ui/react-elements/Checkbox";
import {Button} from "tc-shared/ui/react-elements/Button";
import {LoadingDots} from "tc-shared/ui/react-elements/LoadingDots";
import {ClientIcon} from "svg-sprites/client-icons";
import {ClientIconRenderer} from "tc-shared/ui/react-elements/Icons";
import {tr} from "tc-shared/i18n/localize";

const cssStyle = require("./Renderer.scss");
const ModalEvents = React.createContext<Registry<WhisperTargetModalEvents>>(undefined);

function hasDraftSelection(selection: WhisperTargetDraft, hasCurrentChannel: boolean) : boolean {
    return selection.echoSelf
        || (selection.currentChannel && hasCurrentChannel)
        || selection.clientIds.length > 0
        || selection.groupIds.length > 0;
}

function resolveVoiceStateText(state: WhisperTargetModalState) : React.ReactNode {
    switch (state.voiceState) {
        case "loading":
            return <><Translatable>Loading voice state</Translatable> <LoadingDots /></>;

        case "connecting":
            return <><Translatable>Establishing voice connection</Translatable> <LoadingDots /></>;

        case "connected":
            return <Translatable>Voice connection is ready.</Translatable>;

        case "disconnected":
            return <Translatable>Voice connection is currently disconnected.</Translatable>;

        case "unsupported-client":
            return <Translatable>Your browser does not support the BlackTeaSpeak Web voice connection.</Translatable>;

        case "unsupported-server":
            return <Translatable>The connected server does not support BlackTeaSpeak Web voice negotiation.</Translatable>;

        case "failed":
            return state.voiceMessage || tr("Voice connection failed.");
    }
}

function renderTargetNames(target: WhisperTarget | undefined, state: WhisperTargetModalState) : string {
    const normalized = normalizeWhisperTarget(target);
    if(!hasWhisperTargetSelection(normalized)) {
        return tr("No active whisper target");
    }

    const clientNames = new Map<number, string>(state.clients.map((client: WhisperTargetModalClient) => [client.clientId, client.nickname]));
    const groupNames = new Map<number, string>(state.groups.map((group: WhisperTargetModalGroup) => [group.groupId, group.name]));
    const parts: string[] = [];

    if(normalized.echo) {
        parts.push(tr("Echo"));
    }

    normalized.channels.forEach(channelId => {
        if(state.currentChannel?.channelId === channelId) {
            parts.push(tr("Current channel") + ": " + state.currentChannel.channelName);
        } else {
            parts.push(tr("Channel") + " #" + channelId);
        }
    });

    normalized.clients.forEach(clientId => {
        parts.push(clientNames.get(clientId) || (tr("Client") + " #" + clientId));
    });

    normalized.groups.forEach(groupId => {
        parts.push(groupNames.get(groupId) || (tr("Group") + " #" + groupId));
    });

    return parts.join(", ");
}

const WhisperTargetSection = () => {
    const events = useContext(ModalEvents);
    const [state, setState] = useState<WhisperTargetModalState>(() => {
        events.fire("query_state");
        return {
            voiceState: "loading",
            currentChannel: undefined,
            clients: [],
            groups: [],
            selection: {
                echoSelf: false,
                currentChannel: false,
                clientIds: [],
                groupIds: []
            },
            activeTarget: undefined,
            startPending: false,
            lastError: undefined
        };
    });

    events.reactUse("notify_state", event => setState(event.state));

    const canStart = state.voiceState === "connected"
        && hasDraftSelection(state.selection, !!state.currentChannel)
        && !state.startPending;
    const hasActiveTarget = hasWhisperTargetSelection(state.activeTarget);

    return (
        <div className={cssStyle.container}>
            <div className={cssStyle.hero}>
                <div className={cssStyle.heroIcon}>
                    <ClientIconRenderer icon={ClientIcon.ToggleWhisper} className={cssStyle.icon} />
                </div>
                <div className={cssStyle.heroBody}>
                    <h1><Translatable>Whisper targets</Translatable></h1>
                    <p><Translatable>Select where your whisper audio should be routed.</Translatable></p>
                    <div className={cssStyle.voiceState + " " + (cssStyle[state.voiceState.replace(/-/g, "_")] || "") }>
                        {resolveVoiceStateText(state)}
                    </div>
                </div>
            </div>

            <div className={cssStyle.activeCard}>
                <div className={cssStyle.cardLabel}><Translatable>Active target</Translatable></div>
                <div className={cssStyle.cardValue}>{renderTargetNames(state.activeTarget, state)}</div>
            </div>

            <div className={cssStyle.section}>
                <h2><Translatable>Direct targets</Translatable></h2>
                <Checkbox
                    value={state.selection.echoSelf}
                    onChange={() => events.fire("action_toggle_echo", { enabled: !state.selection.echoSelf })}
                    label={<Translatable>Echo to yourself</Translatable>}
                />
                {state.currentChannel ? <Checkbox
                    value={state.selection.currentChannel}
                    onChange={() => events.fire("action_toggle_current_channel", { enabled: !state.selection.currentChannel })}
                    label={tr("Current channel") + ": " + state.currentChannel.channelName}
                /> : <div className={cssStyle.emptyState}><Translatable>You are currently not inside a channel.</Translatable></div>}
            </div>

            <div className={cssStyle.section}>
                <h2><Translatable>Clients in your current channel</Translatable></h2>
                {state.clients.length === 0 ? <div className={cssStyle.emptyState}><Translatable>No other clients are available in your current channel.</Translatable></div> : state.clients.map(client => (
                    <Checkbox
                        key={client.clientId}
                        value={state.selection.clientIds.indexOf(client.clientId) !== -1}
                        onChange={() => events.fire("action_toggle_client", {
                            clientId: client.clientId,
                            enabled: state.selection.clientIds.indexOf(client.clientId) === -1
                        })}
                        label={client.nickname}
                    />
                ))}
            </div>

            <div className={cssStyle.section}>
                <h2><Translatable>Server groups</Translatable></h2>
                {state.groups.length === 0 ? <div className={cssStyle.emptyState}><Translatable>No server groups are currently available.</Translatable></div> : state.groups.map(group => (
                    <Checkbox
                        key={group.groupId}
                        value={state.selection.groupIds.indexOf(group.groupId) !== -1}
                        onChange={() => events.fire("action_toggle_group", {
                            groupId: group.groupId,
                            enabled: state.selection.groupIds.indexOf(group.groupId) === -1
                        })}
                        label={group.name}
                    />
                ))}
            </div>

            {state.lastError ? <div className={cssStyle.errorBox}>{state.lastError}</div> : undefined}

            <div className={cssStyle.footer}>
                <Button type={"small"} color={"green"} disabled={!canStart} onClick={() => events.fire("action_start_whisper")}>
                    {hasActiveTarget ? <Translatable>Update whisper</Translatable> : <Translatable>Start whisper</Translatable>}
                </Button>
                <Button type={"small"} color={"blue"} disabled={!hasActiveTarget} onClick={() => events.fire("action_stop_whisper")}>
                    <Translatable>Stop whisper</Translatable>
                </Button>
                <Button type={"small"} color={"red"} onClick={() => events.fire("action_close")}>
                    <Translatable>Close</Translatable>
                </Button>
            </div>
        </div>
    );
};

export class ModalWhisperTargets extends InternalModal {
    private readonly events: Registry<WhisperTargetModalEvents>;

    constructor(events: Registry<WhisperTargetModalEvents>) {
        super();
        this.events = events;
    }

    renderBody(): React.ReactNode {
        return (
            <ModalEvents.Provider value={this.events}>
                <WhisperTargetSection />
            </ModalEvents.Provider>
        );
    }

    renderTitle(): React.ReactNode {
        return <Translatable>Whisper targets</Translatable>;
    }
}