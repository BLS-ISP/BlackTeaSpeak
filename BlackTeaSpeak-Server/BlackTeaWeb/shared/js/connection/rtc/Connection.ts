import {AbstractServerConnection, ServerCommand, ServerConnectionEvents} from "tc-shared/connection/ConnectionBase";
import {ConnectionState} from "tc-shared/ConnectionHandler";
import {LogCategory, logDebug, logError, logGroupNative, logInfo, logTrace, LogType, logWarn} from "tc-shared/log";
import {AbstractCommandHandler} from "tc-shared/connection/AbstractCommandHandler";
import {CommandResult} from "tc-shared/connection/ServerConnectionDeclaration";
import {tr, tra} from "tc-shared/i18n/localize";
import {Registry} from "tc-shared/events";
import {RemoteRTPAudioTrack, RemoteRTPTrackState, RemoteRTPVideoTrack, TrackClientInfo} from "./RemoteTrack";
import {SdpCompressor, SdpProcessor} from "./SdpUtils";
import {ErrorCode} from "tc-shared/connection/ErrorCode";
import {WhisperTarget} from "tc-shared/voice/VoiceWhisper";
import {VideoBroadcastConfig, VideoBroadcastType} from "tc-shared/connection/VideoConnection";
import {Settings, settings} from "tc-shared/settings";
import {getAudioBackend} from "tc-shared/audio/Player";

const kSdpCompressionMode = 1;

type RtcIceConfigurationMode = "configured-ice" | "host-only";
const kDefaultIceConfigurationMode: RtcIceConfigurationMode = "host-only";
type LocalIceCandidateCommandPayload = { media_line: number | null, candidate: string, media_id?: string };
type LocalIceMediaTarget = { media_line: number | null, media_id?: string };
type LocalIceCandidateStatsReport = {
    foundation?: string,
    protocol?: string,
    candidateType?: string,
    address?: string,
    ip?: string,
    port?: number,
    portNumber?: number,
    priority?: number,
    relatedAddress?: string,
    relatedPort?: number,
    tcpType?: string,
    usernameFragment?: string,
};

declare global {
    interface RTCRtpEncodingParameters {
        /* Browser supports it, but our bundled TS DOM types do not declare it yet */
        maxFramerate?: number;
    }

    interface HTMLCanvasElement {
        captureStream(framed: number) : MediaStream;
    }
}

export type RtcVideoBroadcastStatistics = {
    dimensions: { width: number, height: number },
    frameRate: number,

    codec?: { name: string, payloadType: number }

    bandwidth?: {
        /* bits per second */
        currentBps: number,
        /* bits per second */
        maxBps: number
    },

    qualityLimitation: "cpu" | "bandwidth" | "none",

    source: {
        frameRate: number,
        dimensions: { width: number, height: number },
    },
};

type RtcIceCandidateErrorSummary = {
    errorCode: number,
    errorText: string,
    candidateAddress: string,
    url: string,
    phase: "gathering" | "post-gathering",
};

type RtcDebugSnapshot = {
    connectionState: RTPConnectionState,
    failedReason: string | undefined,
    iceConfigurationMode: RtcIceConfigurationMode,
    configuredIceServerUrls: string[],
    localCandidateCount: number,
    isSecureContext: boolean,
    userAgent: string,
    peerConnectionState: RTCPeerConnectionState | undefined,
    peerIceConnectionState: RTCIceConnectionState | undefined,
    peerIceGatheringState: RTCIceGatheringState | undefined,
    peerSignalingState: RTCSignalingState | undefined,
    localDescriptionType: RTCSdpType | undefined,
    remoteDescriptionType: RTCSdpType | undefined,
    localDescriptionHasCandidates: boolean,
    remoteDescriptionHasCandidates: boolean,
    recentIceCandidateErrors: RtcIceCandidateErrorSummary[],
    statsSummary?: {
        totalReports: number,
        localCandidateCount: number,
        remoteCandidateCount: number,
        candidatePairCount: number,
        transportCount: number,
        selectedCandidatePairIds: string[],
    },
    statsError?: string,
};

class RetryTimeCalculator {
    private readonly minTime: number;
    private readonly maxTime: number;
    private readonly increment: number;

    private retryCount: number;
    private currentTime: number;

    constructor(minTime: number, maxTime: number, increment: number) {
        this.minTime = minTime;
        this.maxTime = maxTime;
        this.increment = increment;

        this.reset();
    }

    calculateRetryTime() {
        if(this.retryCount >= 5) {
            /* no more retries */
            return 0;
        }
        this.retryCount++;
        const time = this.currentTime;
        this.currentTime = Math.min(this.currentTime + this.increment, this.maxTime);
        return time;
    }

    reset() {
        this.currentTime = this.minTime;
        this.retryCount = 0;
    }
}

class RTCStatsWrapper {
    private readonly supplier: () => Promise<RTCStatsReport>;
    private readonly statistics;

    constructor(supplier: () => Promise<RTCStatsReport>) {
        this.supplier = supplier;
        this.statistics = {};
    }

    async initialize() {
        for(const [key, value] of await this.supplier()) {
            if(typeof this.statistics[key] !== "undefined") {
                logWarn(LogCategory.WEBRTC, tr("Duplicated statistics entry for key %s. Dropping duplicate."), key);
                continue;
            }
            this.statistics[key] = value;
        }
    }

    getValues() : (RTCStats & {[T: string]: string | number})[] {
        return Object.values(this.statistics);
    }

    getStatistic(key: string) : RTCStats & {[T: string]: string | number} {
        return this.statistics[key];
    }

    getStatisticsByType(type: string) : (RTCStats & {[T: string]: string | number})[] {
        return Object.values(this.statistics).filter((e: any) => e.type?.replace(/-/g, "") === type) as any;
    }

    getStatisticByType(type: string): RTCStats & {[T: string]: string | number} {
        const entries = this.getStatisticsByType(type);
        if(entries.length === 0) {
            throw tra("missing statistic entry {}", type);
        } else if(entries.length === 1) {
            return entries[0];
        } else {
            throw tra("duplicated statistics entry of type {}", type);
        }
    }
}

let dummyVideoTrack: MediaStreamTrack | undefined;
let dummyAudioTrack: MediaStreamTrack | undefined;

/*
 * For Firefox as soon we stop a sender we're never able to get the sender starting again...
 * (This only applies after the initial negotiation. Before values of null are allowed)
 * So we've to keep it alive with a dummy track.
 */
function getIdleTrack(kind: "video" | "audio") : MediaStreamTrack | null {
    if(kind === "video") {
        if(!dummyVideoTrack) {
            const canvas = document.createElement("canvas");
            canvas.getContext("2d");
            const stream = canvas.captureStream(1);
            dummyVideoTrack = stream.getVideoTracks()[0];
        }

        return dummyVideoTrack;
    } else if(kind === "audio") {
        if(!dummyAudioTrack) {
            const dest = getAudioBackend().getAudioContext().createMediaStreamDestination();
            dummyAudioTrack = dest.stream.getAudioTracks()[0];
        }

        return dummyAudioTrack;
    }

    return null;
}

class CommandHandler extends AbstractCommandHandler {
    private readonly handle: RTCConnection;
    private readonly sdpProcessor: SdpProcessor;

    constructor(connection: AbstractServerConnection, handle: RTCConnection, sdpProcessor: SdpProcessor) {
        super(connection);
        this.handle = handle;
        this.sdpProcessor = sdpProcessor;
        this.ignore_consumed = true;
    }

    handle_command(command: ServerCommand): boolean {
        if(command.command === "notifyrtcsessiondescription") {
            const data = command.arguments[0];
            if(!this.handle["peer"]) {
                if(this.handle["recentlyResetPeer"]()) {
                    logTrace(LogCategory.WEBRTC, tr("Received late remote %s after a local peer reset. Dropping stale signal."), data.mode);
                } else {
                    logWarn(LogCategory.WEBRTC, tr("Received remote %s without an active peer"), data.mode);
                }
                return;
            }

            /* webrtc-sdp somehow places some empty lines into the sdp */
            let sdp = data.sdp.replace(/\r?\n\r?\n/g, "\n");
            try {
                sdp = SdpCompressor.decompressSdp(sdp, 1);
            } catch (error) {
                logError(LogCategory.WEBRTC, tr("Failed to decompress remote SDP: %o"), error);
                this.handle["handleFatalError"](tr("Failed to decompress remote SDP"), true);
                return;
            }
            if(RTCConnection.kEnableSdpTrace) {
                const gr = logGroupNative(LogType.TRACE, LogCategory.WEBRTC, tra("Original remote SDP ({})", data.mode as string));
                gr.collapsed(true);
                gr.log("%s", data.sdp);
                gr.end();
            }
            try {
                sdp = this.sdpProcessor.processIncomingSdp(sdp, data.mode);
            } catch (error) {
                logError(LogCategory.WEBRTC, tr("Failed to reprocess SDP %s: %o"), data.mode, error);
                this.handle["handleFatalError"](tra("Failed to preprocess SDP {}", data.mode as string), true);
                return;
            }
            if(RTCConnection.kEnableSdpTrace) {
                const gr = logGroupNative(LogType.TRACE, LogCategory.WEBRTC, tra("Patched remote SDP ({})", data.mode as string));
                gr.collapsed(true);
                gr.log("%s", sdp);
                gr.end();
            }
            if(data.mode === "answer") {
                this.handle["peer"].setRemoteDescription({
                    sdp: sdp,
                    type: "answer"
                }).then(() => {
                    this.handle["cachedRemoteSessionDescription"] = sdp;
                    this.handle["peerRemoteDescriptionReceived"] = true;
                    setTimeout(() => this.handle.applyCachedRemoteIceCandidates(), 50);
                }).catch(error => {
                    logError(LogCategory.WEBRTC, tr("Failed to set the remote description: %o"), error);
                    this.handle["handleFatalError"](tr("Failed to set the remote description (answer)"), true);
                });
            } else if(data.mode === "offer") {
                this.handle["cachedRemoteSessionDescription"] = sdp;
                this.handle["peer"].setRemoteDescription({
                    sdp: sdp,
                    type: "offer"
                }).then(() => this.handle["peer"].createAnswer())
                .then(async answer => {
                    if(RTCConnection.kEnableSdpTrace) {
                        const gr = logGroupNative(LogType.TRACE, LogCategory.WEBRTC, tra("Original local SDP ({})", answer.type as string));
                        gr.collapsed(true);
                        gr.log("%s", answer.sdp);
                        gr.end();
                    }
                    answer.sdp = this.sdpProcessor.processOutgoingSdp(answer.sdp, "answer");

                    await this.handle["peer"].setLocalDescription(answer);
                    return this.handle["collectLocalDescriptionSdp"](this.handle["peer"], answer.sdp);
                })
                .then(answerSdp => {
                    const compressedAnswerSdp = SdpCompressor.compressSdp(answerSdp, kSdpCompressionMode);
                    if(RTCConnection.kEnableSdpTrace) {
                        const gr = logGroupNative(LogType.TRACE, LogCategory.WEBRTC, tra("Patched local SDP ({})", "answer"));
                        gr.collapsed(true);
                        gr.log("%s", compressedAnswerSdp);
                        gr.end();
                    }

                    return this.connection.send_command("rtcsessiondescribe", {
                        mode: "answer",
                        sdp: compressedAnswerSdp,
                        compression: kSdpCompressionMode
                    });
                }).catch(error => {
                    logError(LogCategory.WEBRTC, tr("Failed to set the remote description and execute the renegotiation: %o"), error);
                    this.handle["handleFatalError"](tr("Failed to set the remote description (offer/renegotiation)"), true);
                });
            } else {
                logWarn(LogCategory.NETWORKING, tr("Received invalid mode for rtc session description (%s)."), data.mode);
            }
            return true;
        } else if(command.command === "notifyrtcicecandidate") {
            const candidate = command.arguments[0]["candidate"];
            const mediaId = command.arguments[0]["mediaid"];
            const mediaLine = Number.parseInt(command.arguments[0]["medialine"]);

            if(Number.isNaN(mediaLine)) {
                logError(LogCategory.WEBRTC, tr("Failed to parse ICE media line: %o"), command.arguments[0]["medialine"]);
                return;
            }

            if(candidate) {
                const parsedCandidate = new RTCIceCandidate({
                    candidate: "candidate:" + candidate,
                    sdpMid: mediaId || undefined,
                    sdpMLineIndex: mediaLine
                });

                this.handle.handleRemoteIceCandidate(parsedCandidate, mediaLine);
            } else {
                this.handle.handleRemoteIceCandidate(undefined, mediaLine);
            }
        } else if(command.command === "notifyrtcstreamassignment") {
            const data = command.arguments[0];
            if(!data) {
                logWarn(LogCategory.WEBRTC, tr("Received rtc stream assignment without payload."));
                return true;
            }

            const ssrc = parseInt(data["streamid"]) >>> 0;

            if(parseInt(data["sclid"])) {
                const media = parseInt(data["media"]);
                this.handle["doMapStream"](ssrc, {
                    client_id: parseInt(data["sclid"]),
                    client_database_id: parseInt(data["scldbid"]),
                    client_name: data["sclname"],
                    client_unique_id: data["scluid"],
                    media: Number.isNaN(media) ? undefined : media
                });
            } else {
                this.handle["doMapStream"](ssrc, undefined);
            }
        } else if(command.command === "notifyrtcstreamstate") {
            const data = command.arguments[0];
            if(!data) {
                logWarn(LogCategory.WEBRTC, tr("Received rtc stream state without payload."));
                return true;
            }

            const state = parseInt(data["state"]);
            const ssrc = parseInt(data["streamid"]) >>> 0;

            if(state === 0) {
                /* stream stopped */
                this.handle["handleStreamState"](ssrc, 0, undefined);
            } else if(state === 1) {
                this.handle["handleStreamState"](ssrc, 1, {
                    client_id: parseInt(data["sclid"]),
                    client_database_id: parseInt(data["scldbid"]),
                    client_name: data["sclname"],
                    client_unique_id: data["scluid"],
                });
            } else {
                logWarn(LogCategory.WEBRTC, tr("Received unknown/invalid rtc track state: %d"), state);
            }
        }
        return false;
    }
}

export enum RTPConnectionState {
    DISCONNECTED,
    CONNECTING,
    CONNECTED,
    FAILED,
    NOT_SUPPORTED
}

class InternalRemoteRTPAudioTrack extends RemoteRTPAudioTrack {
    private muteTimeout;

    constructor(ssrc: number, transceiver: RTCRtpTransceiver) {
        super(ssrc, transceiver);
    }

    destroy() {
        this.handleTrackEnded();
        super.destroy();
    }

    handleAssignment(info: TrackClientInfo | undefined) {
        if(Object.isSimilar(this.currentAssignment, info)) {
            return;
        }

        this.currentAssignment = info;
        if(info) {
            logDebug(LogCategory.WEBRTC, tr("Remote RTP audio track %d mounted to client %o"), this.getSsrc(), info);
            this.setState(RemoteRTPTrackState.Bound);
        } else {
            logDebug(LogCategory.WEBRTC, tr("Remote RTP audio track %d has been unmounted."), this.getSsrc());
            this.setState(RemoteRTPTrackState.Unbound);
        }
    }

    handleStateNotify(state: number, info: TrackClientInfo | undefined) {
        if(!this.currentAssignment) {
            logWarn(LogCategory.WEBRTC, tr("Received stream state update for %d with miss info. Updating info."), this.getSsrc());
        }

        const validateInfo = () => {
            if(info.client_id !== this.currentAssignment.client_id) {
                logWarn(LogCategory.WEBRTC, tr("Received stream state update for %d with miss matching client info. Expected client %d but received %d. Updating stream assignment."),
                    this.getSsrc(), this.currentAssignment.client_id, info.client_id);
                this.currentAssignment = info;
                /* TODO: Update the assignment via doMapStream */
            } else if(info.client_unique_id !== this.currentAssignment.client_unique_id) {
                logWarn(LogCategory.WEBRTC, tr("Received stream state update for %d with miss matching client info. Expected client %s but received %s. Updating stream assignment."),
                    this.getSsrc(), this.currentAssignment.client_id, info.client_id);
                this.currentAssignment = info;
                /* TODO: Update the assignment via doMapStream */
            } else if(this.currentAssignment.client_name !== info.client_name) {
                this.currentAssignment.client_name = info.client_name;
                /* TODO: Notify name update */
            }
        };

        clearTimeout(this.muteTimeout);
        this.muteTimeout = undefined;
        if(state === 1) {
            validateInfo();
            this.shouldReplay = true;
            this.updateGainNode();
            this.setState(RemoteRTPTrackState.Started);
        } else {
            /* There wil be no info present */
            this.setState(RemoteRTPTrackState.Bound);

            /* since we're might still having some jitter stuff */
            this.muteTimeout = setTimeout(() => {
                this.shouldReplay = false;
                this.updateGainNode();
            }, 1000);
        }
    }
}

class InternalRemoteRTPVideoTrack extends RemoteRTPVideoTrack {
    constructor(ssrc: number, transceiver: RTCRtpTransceiver) {
        super(ssrc, transceiver);
    }

    destroy() {
        this.handleTrackEnded();
        super.destroy();
    }

    handleAssignment(info: TrackClientInfo | undefined) {
        if(Object.isSimilar(this.currentAssignment, info)) {
            return;
        }

        this.currentAssignment = info;
        if(info) {
            logDebug(LogCategory.WEBRTC, tr("Remote RTP video track %d mounted to client %o"), this.getSsrc(), info);
            this.setState(RemoteRTPTrackState.Bound);
        } else {
            logDebug(LogCategory.WEBRTC, tr("Remote RTP video track %d has been unmounted."), this.getSsrc());
            this.setState(RemoteRTPTrackState.Unbound);
        }
    }

    handleStateNotify(state: number, info: TrackClientInfo | undefined) {
        if(!this.currentAssignment) {
            logWarn(LogCategory.WEBRTC, tr("Received stream state update for %d with miss info. Updating info."), this.getSsrc());
        }

        const validateInfo = () => {
            if(info.client_id !== this.currentAssignment.client_id) {
                logWarn(LogCategory.WEBRTC, tr("Received stream state update for %d with miss matching client info. Expected client %d but received %d. Updating stream assignment."),
                    this.getSsrc(), this.currentAssignment.client_id, info.client_id);
                this.currentAssignment = info;
                /* TODO: Update the assignment via doMapStream */
            } else if(info.client_unique_id !== this.currentAssignment.client_unique_id) {
                logWarn(LogCategory.WEBRTC, tr("Received stream state update for %d with miss matching client info. Expected client %s but received %s. Updating stream assignment."),
                    this.getSsrc(), this.currentAssignment.client_id, info.client_id);
                this.currentAssignment = info;
                /* TODO: Update the assignment via doMapStream */
            } else if(this.currentAssignment.client_name !== info.client_name) {
                this.currentAssignment.client_name = info.client_name;
                /* TODO: Notify name update */
            }
        };

        if(state === 1) {
            validateInfo();
            this.setState(RemoteRTPTrackState.Started);
        } else {
            /* There wil be no info present */
            this.setState(RemoteRTPTrackState.Bound);
        }
    }
}

export type RTCSourceTrackType = "audio" | "audio-whisper" | "video" | "video-screen";
export type RTCBroadcastableTrackType = Exclude<RTCSourceTrackType, "audio-whisper">;
const kRtcSourceTrackTypes: RTCSourceTrackType[] = ["audio", "audio-whisper", "video", "video-screen"];

type TemporaryRtpStream = {
    createTimestamp: number,
    timeoutId: number,

    ssrc: number,
    status: number | undefined,
    info: TrackClientInfo | undefined
}

function rtcMediaLabel(media: number | undefined) : string {
    switch (media) {
        case 0:
            return "audio";
        case 1:
            return "audio-whisper";
        case 2:
            return "video";
        case 3:
            return "video-screen";
        default:
            return "unknown";
    }
}

export type RTCConnectionStatistics = {
    videoBytesReceived: number,
    videoBytesSent: number,

    voiceBytesReceived: number,
    voiceBytesSent
}

export interface RTCConnectionEvents {
    notify_state_changed: { oldState: RTPConnectionState, newState: RTPConnectionState },
    notify_audio_assignment_changed: { track: RemoteRTPAudioTrack, info: TrackClientInfo | undefined },
    notify_video_assignment_changed: { track: RemoteRTPVideoTrack, info: TrackClientInfo | undefined },
}

export class RTCConnection {
    public static readonly kEnableSdpTrace = true;

    private readonly audioSupport: boolean;
    private readonly events: Registry<RTCConnectionEvents>;
    private readonly connection: AbstractServerConnection;
    private readonly commandHandler: CommandHandler;
    private readonly sdpProcessor: SdpProcessor;

    private connectionState: RTPConnectionState;
    private connectTimeout: number;
    private failedReason: string;
    private retryCalculator: RetryTimeCalculator;
    private retryTimestamp: number;
    private retryTimeout: number;

    private peer: RTCPeerConnection;
    private currentIceConfigurationMode: RtcIceConfigurationMode = kDefaultIceConfigurationMode;
    private pendingIceConfigurationMode: RtcIceConfigurationMode = kDefaultIceConfigurationMode;
    private lastPeerResetTimestamp: number = 0;
    private localCandidateCount: number;
    private recentIceCandidateErrors: RtcIceCandidateErrorSummary[];

    private peerRemoteDescriptionReceived: boolean;
    private cachedRemoteIceCandidates: { candidate: RTCIceCandidate, mediaLine: number }[];

    private cachedRemoteSessionDescription: string;

    private currentTracks: {[T in RTCSourceTrackType]: MediaStreamTrack | undefined} = {
        "audio-whisper": undefined,
        "video-screen": undefined,
        audio: undefined,
        video: undefined
    };
    private currentTransceiver: {[T in RTCSourceTrackType]: RTCRtpTransceiver | undefined} = {
        "audio-whisper": undefined,
        "video-screen": undefined,
        audio: undefined,
        video: undefined
    };

    private remoteAudioTracks: {[key: number]: InternalRemoteRTPAudioTrack};
    private remoteVideoTracks: {[key: number]: InternalRemoteRTPVideoTrack};
    private temporaryStreams: {[key: number]: TemporaryRtpStream} = {};

    constructor(connection: AbstractServerConnection, audioSupport: boolean) {
        this.events = new Registry<RTCConnectionEvents>();
        this.connection = connection;
        this.sdpProcessor = new SdpProcessor();
        this.commandHandler = new CommandHandler(connection, this, this.sdpProcessor);
        this.retryCalculator = new RetryTimeCalculator(5000, 30000, 10000);
        this.audioSupport = audioSupport;

        this.connection.getCommandHandler().registerHandler(this.commandHandler);
        this.reset(true);

        this.connection.events.on("notify_connection_state_changed", event => this.handleConnectionStateChanged(event));
        (globalThis as any).rtp = this;
    }

    destroy() {
        this.connection.getCommandHandler().unregisterHandler(this.commandHandler);
    }

    isAudioEnabled() : boolean {
        return this.audioSupport;
    }

    getConnection() : AbstractServerConnection {
        return this.connection;
    }

    getEvents() {
        return this.events;
    }

    getConnectionState() : RTPConnectionState {
        return this.connectionState;
    }

    getFailReason() : string {
        return this.failedReason;
    }

    private recentlyResetPeer(timeoutMs: number = 5000) {
        return this.lastPeerResetTimestamp > 0 && Date.now() - this.lastPeerResetTimestamp <= timeoutMs;
    }

    private static getConfiguredDefaultIceServerUrls(): string[] {
        const configuredUrls = settings.getValue(Settings.KEY_RTC_ICE_SERVER_URLS);
        if(!Array.isArray(configuredUrls)) {
            return [];
        }

        return configuredUrls.filter((entry): entry is string => typeof entry === "string" && entry.length > 0);
    }

    private static getDefaultIceConfigurationMode(): RtcIceConfigurationMode {
        return RTCConnection.getConfiguredDefaultIceServerUrls().length > 0 ? "configured-ice" : "host-only";
    }

    private buildPeerConfiguration(): RTCConfiguration {
        let urls = this.getConfiguredIceServerUrls();
        if (urls.length === 0) {
            urls = ["stun:stun.l.google.com:19302", "stun:stun.services.mozilla.com"];
        }
        return {
            bundlePolicy: "max-bundle",
            rtcpMuxPolicy: "require",
            iceServers: [{ urls: urls }]
        };
    }

    private getConfiguredIceServerUrls(): string[] {
        return this.currentIceConfigurationMode === "host-only" ? [] : RTCConnection.getConfiguredDefaultIceServerUrls();
    }

    private extractLocalIceMediaTargets(peer: RTCPeerConnection): LocalIceMediaTarget[] {
        const sdp = peer.localDescription?.sdp || "";
        if(!sdp) {
            return [];
        }

        const targets: LocalIceMediaTarget[] = [];
        let mediaLine = -1;

        for(const line of sdp.split(/\r?\n/)) {
            if(line.startsWith("m=")) {
                mediaLine++;
                targets.push({ media_line: mediaLine });
                continue;
            }

            if(mediaLine < 0 || !line.startsWith("a=mid:")) {
                continue;
            }

            const mediaId = line.substring("a=mid:".length);
            if(mediaId.length > 0) {
                targets[targets.length - 1].media_id = mediaId;
            }
        }

        return targets;
    }

    private extractPrimaryLocalIceMediaTarget(peer: RTCPeerConnection): LocalIceMediaTarget {
        const sdp = peer.localDescription?.sdp || "";
        const targets = this.extractLocalIceMediaTargets(peer);

        const bundleLine = sdp.split(/\r?\n/).find(line => line.startsWith("a=group:BUNDLE "));
        if(bundleLine) {
            const bundleMid = bundleLine.substring("a=group:BUNDLE ".length).trim().split(/\s+/).find(Boolean);
            if(bundleMid) {
                const target = targets.find(candidateTarget => candidateTarget.media_id === bundleMid);
                if(target) {
                    return target;
                }
            }
        }

        const transceiverMid = peer.getTransceivers()
            .map(transceiver => transceiver.mid)
            .find(mediaId => typeof mediaId === "string" && mediaId.length > 0);
        if(transceiverMid) {
            const target = targets.find(candidateTarget => candidateTarget.media_id === transceiverMid);
            if(target) {
                return target;
            }

            return { media_line: 0, media_id: transceiverMid };
        }

        return targets[0] || { media_line: 0 };
    }

    private extractLocalDescriptionIceCandidatePayloads(peer: RTCPeerConnection): LocalIceCandidateCommandPayload[] {
        const sdp = peer.localDescription?.sdp || "";
        if(!sdp) {
            return [];
        }

        const payloads: LocalIceCandidateCommandPayload[] = [];
        let mediaLine = -1;
        let mediaId: string | undefined;

        for(const line of sdp.split(/\r?\n/)) {
            if(line.startsWith("m=")) {
                mediaLine++;
                mediaId = undefined;
                continue;
            }

            if(mediaLine < 0) {
                continue;
            }

            if(line.startsWith("a=mid:")) {
                mediaId = line.substring("a=mid:".length);
                continue;
            }

            if(!line.startsWith("a=candidate:")) {
                continue;
            }

            const payload: LocalIceCandidateCommandPayload = {
                media_line: mediaLine,
                candidate: line.substring(2)
            };
            if(mediaId && mediaId.length > 0) {
                payload.media_id = mediaId;
            }
            payloads.push(payload);
        }

        return payloads;
    }

    private async sendLocalIceCandidatePayloads(payloads: LocalIceCandidateCommandPayload[]) {
        for(const payload of payloads) {
            await this.connection.send_command("rtcicecandidate", payload);
        }
    }

    private buildLocalIceCandidateFromStats(report: RTCStats): string | undefined {
        const candidate = report as RTCStats & LocalIceCandidateStatsReport;
        const foundation = candidate.foundation;
        const protocol = candidate.protocol;
        const candidateType = candidate.candidateType;
        const address = candidate.address || candidate.ip;
        const port = Number(candidate.port ?? candidate.portNumber);
        const priority = Number(candidate.priority);

        if(typeof foundation !== "string" || foundation.length === 0 ||
            typeof protocol !== "string" || protocol.length === 0 ||
            typeof candidateType !== "string" || candidateType.length === 0 ||
            typeof address !== "string" || address.length === 0 ||
            !Number.isFinite(port) || !Number.isFinite(priority)) {
            return undefined;
        }

        if(address.endsWith(".local")) {
            logTrace(LogCategory.WEBRTC, tr("Allowing stats-derived local fqdn ICE candidate for %s"), address);
        }

        const candidateParts = [
            `candidate:${foundation}`,
            "1",
            protocol.toLowerCase(),
            String(priority),
            address,
            String(port),
            "typ",
            candidateType
        ];

        if(typeof candidate.relatedAddress === "string" && candidate.relatedAddress.length > 0 && Number.isFinite(candidate.relatedPort)) {
            candidateParts.push("raddr", candidate.relatedAddress, "rport", String(candidate.relatedPort));
        }

        if(typeof candidate.tcpType === "string" && candidate.tcpType.length > 0) {
            candidateParts.push("tcptype", candidate.tcpType);
        }

        if(typeof candidate.usernameFragment === "string" && candidate.usernameFragment.length > 0) {
            candidateParts.push("ufrag", candidate.usernameFragment);
        }

        return candidateParts.join(" ");
    }

    private async extractStatsLocalIceCandidatePayloads(peer: RTCPeerConnection): Promise<LocalIceCandidateCommandPayload[]> {
        const target = this.extractPrimaryLocalIceMediaTarget(peer);

        try {
            const stats = await peer.getStats();
            const payloads: LocalIceCandidateCommandPayload[] = [];
            const seenCandidates = new Set<string>();

            for(const report of stats.values()) {
                if(report.type !== "local-candidate") {
                    continue;
                }

                const candidate = this.buildLocalIceCandidateFromStats(report);
                if(!candidate || seenCandidates.has(candidate)) {
                    continue;
                }

                seenCandidates.add(candidate);
                const payload: LocalIceCandidateCommandPayload = {
                    media_line: target.media_line,
                    candidate
                };
                if(target.media_id) {
                    payload.media_id = target.media_id;
                }
                payloads.push(payload);
            }

            return payloads;
        } catch(error) {
            logTrace(LogCategory.WEBRTC, tr("Failed to reconstruct local ICE candidates from RTC stats: %o"), error);
            return [];
        }
    }

    private async statsContainLocalCandidates(peer: RTCPeerConnection): Promise<boolean> {
        try {
            const stats = await peer.getStats();
            for(const report of stats.values()) {
                if(report.type === "local-candidate") {
                    return true;
                }
            }
        } catch(error) {
            logTrace(LogCategory.WEBRTC, tr("Failed to inspect RTC stats for local ICE candidates: %o"), error);
        }

        return false;
    }

    private async tryForwardLocalDescriptionIceCandidates(peer: RTCPeerConnection): Promise<boolean> {
        const synthesizedLocalCandidates = this.extractLocalDescriptionIceCandidatePayloads(peer);
        if(synthesizedLocalCandidates.length === 0) {
            return false;
        }

        await this.sendLocalIceCandidatePayloads(synthesizedLocalCandidates);
        if(this.peer !== peer) {
            return true;
        }

        this.localCandidateCount = synthesizedLocalCandidates.length;
        logTrace(
            LogCategory.WEBRTC,
            tr("Browser finished ICE gathering without trickled candidate events. Forwarded %d local ICE candidates from the local SDP instead."),
            synthesizedLocalCandidates.length
        );
        return true;
    }

    private async prepareLocalIceCandidateFinish(peer: RTCPeerConnection): Promise<boolean> {
        if(this.localCandidateCount > 0) {
            return true;
        }

        if(await this.tryForwardLocalDescriptionIceCandidates(peer)) {
            return this.peer === peer;
        }

        if(this.peer !== peer) {
            return false;
        }

        const synthesizedStatsCandidates = await this.extractStatsLocalIceCandidatePayloads(peer);
        if(synthesizedStatsCandidates.length > 0) {
            if(this.peer !== peer) {
                return false;
            }

            await this.sendLocalIceCandidatePayloads(synthesizedStatsCandidates);
            if(this.peer !== peer) {
                return false;
            }

            this.localCandidateCount = synthesizedStatsCandidates.length;
            logTrace(
                LogCategory.WEBRTC,
                tr("Browser finished ICE gathering without exposed SDP candidates. Forwarded %d local ICE candidates reconstructed from RTC stats instead."),
                synthesizedStatsCandidates.length
            );
            return true;
        }

        if(await this.statsContainLocalCandidates(peer)) {
            if(this.peer !== peer) {
                return false;
            }

            logWarn(LogCategory.WEBRTC, tr("ICE stats expose local candidates, but the local SDP does not expose any candidates to forward to the server."));
            if(this.retryIceGatheringHostOnly(peer)) {
                return false;
            }

            this.handleFatalError(this.buildUnforwardableLocalIceCandidatesReason(), false);
            return false;
        }

        if(this.retryIceGatheringHostOnly(peer)) {
            return false;
        }

        logError(LogCategory.WEBRTC, tr("Received local ICE candidate finish, without having any candidates"));
        this.getDebugSnapshot().then(snapshot => {
            logError(LogCategory.WEBRTC, tr("RTC debug snapshot for missing local ICE candidates: %o"), snapshot);
        }).catch(error => {
            logWarn(LogCategory.WEBRTC, tr("Failed to gather RTC debug snapshot for missing local ICE candidates: %o"), error);
        });
        this.handleFatalError(this.buildNoLocalIceCandidatesReason(), false);
        return false;
    }

    private retryIceGatheringHostOnly(peer: RTCPeerConnection): boolean {
        if(this.peer !== peer || this.currentIceConfigurationMode === "host-only") {
            return false;
        }

        logWarn(
            LogCategory.WEBRTC,
            tr("Retrying ICE gathering in host-only mode because the configured ICE server list produced no usable local candidates")
        );
        this.pendingIceConfigurationMode = "host-only";
        this.reset(false);
        this.doInitialSetup();
        return true;
    }

    private async handleLocalIceCandidateFinish(peer: RTCPeerConnection | undefined) {
        if(!peer || this.peer !== peer) {
            return;
        }

        const hadLocalCandidatesBeforeFinish = this.localCandidateCount > 0;
        if(!(await this.prepareLocalIceCandidateFinish(peer))) {
            return;
        }

        if(hadLocalCandidatesBeforeFinish) {
            logTrace(LogCategory.WEBRTC, tr("Received local ICE candidate finish"));
        }

        this.connection.send_command("rtcicecandidate", { }).catch(error => {
            logWarn(LogCategory.WEBRTC, tr("Failed to transmit local ICE candidate finish to server: %o"), error);
        });
    }

    private localDescriptionContainsCandidates(peer: RTCPeerConnection | undefined = this.peer) {
        return /a=candidate:/m.test(peer?.localDescription?.sdp || "");
    }

    private async waitForLocalDescriptionIce(peer: RTCPeerConnection, timeoutMs: number = 1500): Promise<void> {
        if(this.localDescriptionContainsCandidates(peer) || peer.iceGatheringState === "complete") {
            return;
        }

        await new Promise<void>(resolve => {
            const cleanup = () => {
                clearTimeout(timeout);
                peer.removeEventListener("icecandidate", handleIceCandidate);
                peer.removeEventListener("icegatheringstatechange", handleGatheringStateChange);
                resolve();
            };

            const handleIceCandidate = () => {
                if(this.localDescriptionContainsCandidates(peer)) {
                    cleanup();
                }
            };

            const handleGatheringStateChange = () => {
                if(peer.iceGatheringState === "complete" || this.localDescriptionContainsCandidates(peer)) {
                    cleanup();
                }
            };

            const timeout = setTimeout(cleanup, timeoutMs);
            peer.addEventListener("icecandidate", handleIceCandidate);
            peer.addEventListener("icegatheringstatechange", handleGatheringStateChange);
        });
    }

    private async collectLocalDescriptionSdp(peer: RTCPeerConnection, fallbackSdp: string): Promise<string> {
        await this.waitForLocalDescriptionIce(peer);
        return peer.localDescription?.sdp || fallbackSdp;
    }

    async getDebugSnapshot(): Promise<RtcDebugSnapshot> {
        const localDescription = this.peer?.localDescription?.sdp || "";
        const remoteDescription = this.peer?.remoteDescription?.sdp || "";

        const snapshot: RtcDebugSnapshot = {
            connectionState: this.connectionState,
            failedReason: this.failedReason,
            iceConfigurationMode: this.currentIceConfigurationMode,
            configuredIceServerUrls: this.getConfiguredIceServerUrls(),
            localCandidateCount: this.localCandidateCount,
            isSecureContext: globalThis.isSecureContext,
            userAgent: navigator.userAgent,
            peerConnectionState: this.peer?.connectionState,
            peerIceConnectionState: this.peer?.iceConnectionState,
            peerIceGatheringState: this.peer?.iceGatheringState,
            peerSignalingState: this.peer?.signalingState,
            localDescriptionType: this.peer?.localDescription?.type,
            remoteDescriptionType: this.peer?.remoteDescription?.type,
            localDescriptionHasCandidates: /a=candidate:/m.test(localDescription),
            remoteDescriptionHasCandidates: /a=candidate:/m.test(remoteDescription),
            recentIceCandidateErrors: this.recentIceCandidateErrors.slice(),
        };

        if(!this.peer) {
            return snapshot;
        }

        try {
            const statsSummary = {
                totalReports: 0,
                localCandidateCount: 0,
                remoteCandidateCount: 0,
                candidatePairCount: 0,
                transportCount: 0,
                selectedCandidatePairIds: [] as string[],
            };

            const stats = await this.peer.getStats();
            stats.forEach(report => {
                statsSummary.totalReports++;

                switch(report.type) {
                    case "local-candidate":
                        statsSummary.localCandidateCount++;
                        break;

                    case "remote-candidate":
                        statsSummary.remoteCandidateCount++;
                        break;

                    case "candidate-pair":
                        statsSummary.candidatePairCount++;
                        break;

                    case "transport": {
                        statsSummary.transportCount++;
                        const selectedCandidatePairId = (report as RTCTransportStats).selectedCandidatePairId;
                        if(selectedCandidatePairId) {
                            statsSummary.selectedCandidatePairIds.push(selectedCandidatePairId);
                        }
                        break;
                    }
                }
            });

            snapshot.statsSummary = statsSummary;
        } catch(error) {
            snapshot.statsError = `${error}`;
        }

        return snapshot;
    }

    getRetryTimestamp() : number | 0 {
        return this.retryTimestamp;
    }

    restartConnection() {
        if(this.connectionState === RTPConnectionState.DISCONNECTED) {
            /* We've been disconnected on purpose */
            return;
        }

        this.reset(true);
        this.doInitialSetup();
    }

    reset(updateConnectionState: boolean) {
        logTrace(LogCategory.WEBRTC, tr("Resetting the RTC connection (Updating connection state: %o)"), updateConnectionState);
        const hadPeer = !!this.peer;
        if(this.peer) {
            if(this.getConnection().connected()) {
                this.getConnection().send_command("rtcsessionreset").catch(error => {
                    logWarn(LogCategory.WEBRTC, tr("Failed to signal RTC session reset to server: %o"), error);
                });
            }

            this.peer.onconnectionstatechange = undefined;
            this.peer.ondatachannel = undefined;
            this.peer.onicecandidate = undefined;
            this.peer.onicecandidateerror = undefined;
            this.peer.oniceconnectionstatechange = undefined;
            this.peer.onicegatheringstatechange = undefined;
            this.peer.onnegotiationneeded = undefined;
            this.peer.onsignalingstatechange = undefined;
            this.peer.ontrack = undefined;

            this.peer.close();
            this.peer = undefined;
        }
        if(hadPeer) {
            this.lastPeerResetTimestamp = Date.now();
        }
        this.peerRemoteDescriptionReceived = false;
        this.cachedRemoteIceCandidates = [];
        this.cachedRemoteSessionDescription = undefined;

        clearTimeout(this.connectTimeout);
        Object.keys(this.currentTransceiver).forEach(key => this.currentTransceiver[key] = undefined);

        this.sdpProcessor.reset();

        if(this.remoteAudioTracks) {
            Object.values(this.remoteAudioTracks).forEach(track => track.destroy());
        }
        this.remoteAudioTracks = {};

        if(this.remoteVideoTracks) {
            Object.values(this.remoteVideoTracks).forEach(track => track.destroy());
        }
        this.remoteVideoTracks = {};

        this.temporaryStreams = {};
        this.localCandidateCount = 0;
    this.recentIceCandidateErrors = [];

        clearTimeout(this.retryTimeout);
        this.retryTimeout = 0;
        this.retryTimestamp = 0;
        /*
         * We do not reset the retry timer here since we might get called when a fatal error occurs.
         * Instead we're resetting it every time we've changed the server connection state.
         */
        /* this.retryCalculator.reset(); */

        if(updateConnectionState) {
            this.updateConnectionState(RTPConnectionState.DISCONNECTED);
        }
    }

    async setTrackSource(type: RTCSourceTrackType, source: MediaStreamTrack | null) : Promise<MediaStreamTrack> {
        switch (type) {
            case "audio":
            case "audio-whisper":
                if(!this.audioSupport) { throw tr("audio support isn't enabled"); }
                if(source && source.kind !== "audio") { throw tr("invalid track type"); }
                break;
            case "video":
            case "video-screen":
                if(source && source.kind !== "video") { throw tr("invalid track type"); }
                break;
        }

        if(this.currentTracks[type] === source) {
            return;
        }

        const oldTrack = this.currentTracks[type] = source;
        await this.updateTracks();
        return oldTrack;
    }

    async clearTrackSources(types: RTCSourceTrackType[]) : Promise<MediaStreamTrack[]> {
        const result = [];

        for(const type of types) {
            if(this.currentTracks[type]) {
                result.push(this.currentTracks[type]);
                this.currentTracks[type] = null;
            } else {
                result.push(undefined);
            }
        }

        if(result.find(entry => typeof entry !== "undefined") !== -1) {
            await this.updateTracks();
        }

        return result;
    }

    getTrackTypeFromSsrc(ssrc: number) : RTCSourceTrackType | undefined {
        const mediaId = this.sdpProcessor.getLocalMediaIdFromSsrc(ssrc);
        if(!mediaId) {
            return undefined;
        }

        for(const type of kRtcSourceTrackTypes) {
            if(this.currentTransceiver[type]?.mid === mediaId) {
                return type;
            }
        }

        return undefined;
    }

    public async startVideoBroadcast(type: VideoBroadcastType, config: VideoBroadcastConfig) {
        let track: RTCBroadcastableTrackType;
        let broadcastType: number;
        switch (type) {
            case "camera":
                broadcastType = 0;
                track = "video";
                break;

            case "screen":
                broadcastType = 1;
                track = "video-screen";
                break;

            default:
                throw tr("invalid video broadcast type");
        }

        let payload = {};
        payload["broadcast_keyframe_interval"] = config.keyframeInterval;
        payload["broadcast_bitrate_max"] = config.maxBandwidth;
        payload["ssrc"] =  this.sdpProcessor.getLocalSsrcFromFromMediaId(this.currentTransceiver[track].mid);
        payload["type"] = broadcastType;

        try {
            await this.connection.send_command("broadcastvideo", payload);
        } catch (error) {
            if(error instanceof CommandResult) {
                if(error.id === ErrorCode.SERVER_INSUFFICIENT_PERMISSIONS) {
                    throw tr("failed on permission") + " " + this.connection.client.permissions.getFailedPermission(error);
                }

                error = error.formattedMessage();
            }
            logError(LogCategory.WEBRTC, tr("failed to start %s broadcast: %o"), type, error);
            throw tr("failed to signal broadcast start");
        }
    }

    private calculateScaleResolutionDownBy(source: MediaStreamTrack | null, config: VideoBroadcastConfig) : number | undefined {
        const trackSettings = source?.getSettings?.();
        if(typeof trackSettings?.width !== "number" || typeof trackSettings?.height !== "number") {
            return undefined;
        }

        const widthScale = config.width > 0 ? trackSettings.width / config.width : 1;
        const heightScale = config.height > 0 ? trackSettings.height / config.height : 1;
        const scale = Math.max(widthScale, heightScale, 1);
        if(scale <= 1.05) {
            return undefined;
        }

        return Math.round(scale * 100) / 100;
    }

    public async configureVideoSender(type: VideoBroadcastType, source: MediaStreamTrack | null, config: VideoBroadcastConfig) {
        const track: RTCBroadcastableTrackType = type === "camera" ? "video" : "video-screen";
        const sender = this.currentTransceiver[track]?.sender;
        if(!sender) {
            return;
        }

        const parameters = sender.getParameters();
        if(!parameters.encodings || parameters.encodings.length === 0) {
            parameters.encodings = [{}];
        }

        const encoding = parameters.encodings[0];
        if(config.maxBandwidth > 0) {
            encoding.maxBitrate = config.maxBandwidth;
        } else {
            delete encoding.maxBitrate;
        }

        if(config.maxFrameRate > 0) {
            encoding.maxFramerate = config.maxFrameRate;
        } else {
            delete encoding.maxFramerate;
        }

        const scaleResolutionDownBy = this.calculateScaleResolutionDownBy(source, config);
        if(scaleResolutionDownBy) {
            encoding.scaleResolutionDownBy = scaleResolutionDownBy;
        } else {
            delete encoding.scaleResolutionDownBy;
        }

        parameters.degradationPreference = type === "screen" ? "maintain-framerate" : "balanced";

        try {
            await sender.setParameters(parameters);
        } catch (error) {
            logWarn(LogCategory.WEBRTC, tr("Failed to configure %s video sender parameters: %o"), type, error);
        }
    }

    public async changeVideoBroadcastConfig(type: VideoBroadcastType, config: VideoBroadcastConfig) {
        let track: RTCBroadcastableTrackType;
        let broadcastType: number;
        switch (type) {
            case "camera":
                broadcastType = 0;
                track = "video";
                break;

            case "screen":
                broadcastType = 1;
                track = "video-screen";
                break;

            default:
                throw tr("invalid video broadcast type");
        }

        let payload = {};
        payload["broadcast_keyframe_interval"] = config.keyframeInterval;
        payload["broadcast_bitrate_max"] = config.maxBandwidth;
        payload["bt"] = broadcastType;

        try {
            await this.connection.send_command("broadcastvideoconfigure", payload);
        } catch (error) {
            if(error instanceof CommandResult) {
                if(error.id === ErrorCode.SERVER_INSUFFICIENT_PERMISSIONS) {
                    throw tr("failed on permission") + " " + this.connection.client.permissions.getFailedPermission(error);
                }

                error = error.formattedMessage();
            }
            logError(LogCategory.WEBRTC, tr("failed to update %s broadcast: %o"), type, error);
            throw tr("failed to update broadcast config");
        }
    }

    public async startAudioBroadcast() {
        try {
            await this.connection.send_command("broadcastaudio", {
                ssrc: this.sdpProcessor.getLocalSsrcFromFromMediaId(this.currentTransceiver["audio"].mid)
            });
        } catch (error) {
            logError(LogCategory.WEBRTC, tr("failed to start %s broadcast: %o"), "audio", error);
            throw tr("failed to signal broadcast start");
        }
    }

    public async startWhisper(target: WhisperTarget) : Promise<void> {
        if(!this.audioSupport) {
            throw tr("audio support isn't enabled");
        }

        const transceiver = this.currentTransceiver["audio-whisper"];
        if(typeof transceiver === "undefined") {
            throw tr("missing transceiver");
        }

        const ssrc = this.sdpProcessor.getLocalSsrcFromFromMediaId(transceiver.mid);
        const payload: any[] = [];

        const addTargets = (type: number, targets: number[]) => {
            targets.forEach(targetId => payload.push({
                ssrc: ssrc,
                type: type,
                target: targetId,
                id: 0
            }));
        };

        if(target.target === "echo") {
            payload.push({
                ssrc: ssrc,
                type: 0x10, /* self */
                target: 0,
                id: 0
            });
        } else if(target.target === "channel-clients") {
            addTargets(0x01, target.channels);
            addTargets(0x02, target.clients);
        } else if(target.target === "groups") {
            addTargets(0x04, target.groups);
        } else if(target.target === "custom") {
            if(target.echo) {
                payload.push({
                    ssrc: ssrc,
                    type: 0x10,
                    target: 0,
                    id: 0
                });
            }

            addTargets(0x01, target.channels);
            addTargets(0x02, target.clients);
            addTargets(0x04, target.groups);
        } else {
            throw new Error("target not yet supported");
        }

        if(payload.length === 0) {
            throw new Error("target not yet supported");
        }

        await this.connection.send_command("whispersessioninitialize", payload, { flagset: ["new"] });
    }

    public stopTrackBroadcast(type: RTCBroadcastableTrackType) {
        let promise: Promise<any>;
        switch (type) {
            case "audio":
                promise = this.connection.send_command("broadcastaudio", {
                    ssrc: 0
                });
                break;

            case "video-screen":
                promise = this.connection.send_command("broadcastvideo", {
                    type: 1,
                    ssrc: 0
                });
                break;

            case "video":
                promise = this.connection.send_command("broadcastvideo", {
                    type: 0,
                    ssrc: 0
                });
                break;
        }

        promise.catch(error => {
            logWarn(LogCategory.WEBRTC, tr("Failed to signal track broadcast stop: %o"), error);
        });
    }

    public setNotSupported() {
        this.reset(false);
        this.updateConnectionState(RTPConnectionState.NOT_SUPPORTED);
    }

    private updateConnectionState(newState: RTPConnectionState) {
        if(this.connectionState === newState) { return; }

        const oldState = this.connectionState;
        if(newState !== RTPConnectionState.FAILED) {
            this.failedReason = undefined;
        }
        this.connectionState = newState;
        this.events.fire("notify_state_changed", { oldState: oldState, newState: newState });
    }

    private handleFatalError(error: string, allowRetry: boolean) {
        this.reset(false);
        this.failedReason = error;
        this.updateConnectionState(RTPConnectionState.FAILED);

        const log = this.connection.client.log;
        if(allowRetry) {
            const time = this.retryCalculator.calculateRetryTime();
            if(time > 0) {
                this.retryTimestamp = Date.now() + time;
                this.retryTimeout = setTimeout(() => {
                    this.doInitialSetup();
                }, time);

                log.log("webrtc.fatal.error", {
                    message: error,
                    retryTimeout: time
                });
            } else {
                allowRetry = false;
            }
        }

        if(!allowRetry) {
            log.log("webrtc.fatal.error", {
                message: error,
                retryTimeout: 0
            });
        }
    }

    private formatRecentIceCandidateError() {
        if(this.recentIceCandidateErrors.length === 0) {
            return undefined;
        }

        const lastError = this.recentIceCandidateErrors.slice(-1)[0];
        return `${lastError.errorCode}/${lastError.errorText} (${lastError.candidateAddress || "unknown-host"} via ${lastError.url || "unknown-url"})`;
    }

    private buildNoLocalIceCandidatesReason() {
        const details: string[] = [
            tr("Failed to gather any local ICE candidates."),
            tr("ICE gathering finished without host or srflx candidates."),
        ];

        details.push(tr("ICE configuration mode: %s."), this.currentIceConfigurationMode);
        if(this.getConfiguredIceServerUrls().length === 0) {
            details.push(tr("External STUN servers were disabled for the current retry."));
        }

        if(this.peer?.iceGatheringState) {
            details.push(tr("Gathering state: %s."), this.peer.iceGatheringState);
        }

        const lastError = this.formatRecentIceCandidateError();
        if(lastError) {
            details.push(tr("Last ICE error: %s."), lastError);
        } else {
            details.push(tr("This usually points to Chrome policy, extension, proxy, VPN, or similar network filtering."));
        }

        if(!globalThis.isSecureContext) {
            details.push(tr("The page is not running in a secure context."));
        }

        details.push(tr("For a full snapshot run `await window.rtp.getDebugSnapshot()` in the browser console."));
        return details.join(" ");
    }

    private buildUnforwardableLocalIceCandidatesReason() {
        const details: string[] = [
            tr("Local ICE candidates were found in RTC stats, but the browser did not expose any forwardable candidates through SDP or trickled ICE events."),
            tr("The remaining RTC stats data could not be reconstructed into a server-usable ICE candidate payload."),
        ];

        details.push(tr("ICE configuration mode: %s."), this.currentIceConfigurationMode);
        if(this.getConfiguredIceServerUrls().length === 0) {
            details.push(tr("External STUN servers were disabled for the current retry."));
        }

        if(this.peer?.iceGatheringState) {
            details.push(tr("Gathering state: %s."), this.peer.iceGatheringState);
        }

        const lastError = this.formatRecentIceCandidateError();
        if(lastError) {
            details.push(tr("Last ICE error: %s."), lastError);
        }

        details.push(tr("For a full snapshot run `await window.rtp.getDebugSnapshot()` in the browser console."));
        return details.join(" ");
    }

    private static checkBrowserSupport() {
        if(!window.RTCRtpSender || !RTCRtpSender.prototype) {
            throw tr("Missing RTCRtpSender");
        }

        if(!RTCRtpSender.prototype.getParameters) {
            throw tr("RTCRtpSender.getParameters");
        }

        if(!RTCRtpSender.prototype.replaceTrack) {
            throw tr("RTCRtpSender.getParameters");
        }
    }

    public doInitialSetup() {
        if(!("RTCPeerConnection" in window)) {
            this.handleFatalError(tr("WebRTC has been disabled (RTCPeerConnection is not defined)"), false);
            return;
        }

        if(!("addTransceiver" in RTCPeerConnection.prototype)) {
            this.handleFatalError(tr("WebRTC api incompatible (RTCPeerConnection.addTransceiver missing)"), false);
            return;
        }

        this.currentIceConfigurationMode = this.pendingIceConfigurationMode;
        this.pendingIceConfigurationMode = RTCConnection.getDefaultIceConfigurationMode();
        this.peer = new RTCPeerConnection(this.buildPeerConfiguration());

        if(this.audioSupport) {
            this.currentTransceiver["audio"] = this.peer.addTransceiver("audio");
            this.currentTransceiver["audio-whisper"] = this.peer.addTransceiver("audio");

            if(window.detectedBrowser.name === "firefox") {
                /*
                 * For some reason FF (<= 85.0) does not replay any audio from extra added transceivers.
                 * On the other hand, if the server is creating that track or we're using it for sending audio as well
                 * it works. So we just wait for the server to come up with new streams (even though we need to renegotiate...).
                 * For Chrome we only need to negotiate once in most cases.
                 * Side note: This does not apply to video channels!
                 */
            } else {
                /* add some other transceivers for later use */
                for(let i = 0; i < settings.getValue(Settings.KEY_RTC_EXTRA_AUDIO_CHANNELS); i++) {
                    this.peer.addTransceiver("audio", { direction: "recvonly" });
                }
            }
        }

        this.currentTransceiver["video"] = this.peer.addTransceiver("video");
        this.currentTransceiver["video-screen"] = this.peer.addTransceiver("video");

        /* add some other transceivers for later use */
        for(let i = 0; i < settings.getValue(Settings.KEY_RTC_EXTRA_VIDEO_CHANNELS); i++) {
            this.peer.addTransceiver("video", { direction: "recvonly" });
        }

        this.peer.onicecandidate = event => this.handleLocalIceCandidate(event.candidate);
        this.peer.onicecandidateerror = event => this.handleIceCandidateError(event as any);
        this.peer.oniceconnectionstatechange = () => this.handleIceConnectionStateChanged();
        this.peer.onicegatheringstatechange = () => this.handleIceGatheringStateChanged();

        this.peer.onsignalingstatechange = () => this.handleSignallingStateChanged();
        this.peer.onconnectionstatechange = () => this.handlePeerConnectionStateChanged();

        this.peer.ondatachannel = event => this.handleDataChannel(event.channel);
        this.peer.ontrack = event => this.handleTrack(event);

        this.updateConnectionState(RTPConnectionState.CONNECTING);
        this.doInitialSetup0().catch(error => {
            this.handleFatalError(tr("initial setup failed"), true);
            logError(LogCategory.WEBRTC, tr("Connection setup failed: %o"), error);
        });
    }

    private async updateTracks() {
        for(const type of kRtcSourceTrackTypes) {
            if(!this.currentTransceiver[type]?.sender) {
                continue;
            }

            let fallback;
            switch (type) {
                case "audio":
                case "audio-whisper":
                    fallback = getIdleTrack("audio");
                    break;

                case "video":
                case "video-screen":
                    fallback = getIdleTrack("video");
                    break;
            }

            let target = this.currentTracks[type] || fallback;
            if(this.currentTransceiver[type].sender.track === target) {
                continue;
            }

            await this.currentTransceiver[type].sender.replaceTrack(target);

            /* Firefox has some crazy issues */
            if(window.detectedBrowser.name !== "firefox") {
                if(target) {
                    logTrace(LogCategory.NETWORKING, "Setting sendrecv from %o", this.currentTransceiver[type].direction, this.currentTransceiver[type].currentDirection);
                    this.currentTransceiver[type].direction = "sendrecv";
                } else if(type === "video" || type === "video-screen") {
                    /*
                     * We don't need to stop & start the audio transceivers every time we're toggling the stream state.
                     * This would be a much overall cost than just keeping it going.
                     *
                     * The video streams instead are not toggling that much and since they split up the bandwidth between them,
                     * we've to shut them down if they're no needed. This not only allows the one stream to take full advantage
                     * of the bandwidth it also reduces resource usage.
                     */
                    //this.currentTransceiver[type].direction = "recvonly";
                }
            }
            logTrace(LogCategory.WEBRTC, "Replaced track for %o (Fallback: %o)", type, target === fallback);
        }
    }

    private async doInitialSetup0() {
        RTCConnection.checkBrowserSupport();

        const peer = this.peer;
        await this.updateTracks();

        const offer = await peer.createOffer({ iceRestart: false, offerToReceiveAudio: this.audioSupport, offerToReceiveVideo: true });
        if(offer.type !== "offer") { throw tr("created ofer isn't of type offer"); }
        if(this.peer !== peer) { return; }

        if(RTCConnection.kEnableSdpTrace) {
            const gr = logGroupNative(LogType.TRACE, LogCategory.WEBRTC, tra("Original initial local SDP (offer)"));
            gr.collapsed(true);
            gr.log("%s", offer.sdp);
            gr.end();
        }
        try {
            offer.sdp = this.sdpProcessor.processOutgoingSdp(offer.sdp, "offer");
            const gr = logGroupNative(LogType.TRACE, LogCategory.WEBRTC, tra("Patched initial local SDP (offer)"));
            gr.collapsed(true);
            gr.log("%s", offer.sdp);
            gr.end();
        } catch (error) {
            logError(LogCategory.WEBRTC, tr("Failed to preprocess outgoing initial offer: %o"), error);
            this.handleFatalError(tr("Failed to preprocess outgoing initial offer"), true);
            return;
        }

        await peer.setLocalDescription(offer);
        if(this.peer !== peer) { return; }

        const localOfferSdp = await this.collectLocalDescriptionSdp(peer, offer.sdp);
        if(this.peer !== peer) { return; }

        try {
            await this.connection.send_command("rtcsessiondescribe", {
                mode: "offer",
                sdp: localOfferSdp
            });
        } catch (error) {
            if(this.peer !== peer) { return; }
            if(error instanceof CommandResult) {
                if(error.id === ErrorCode.COMMAND_NOT_FOUND) {
                    this.setNotSupported();
                    return;
                }
                error = error.formattedMessage();
            }
            logWarn(LogCategory.VOICE, tr("Failed to initialize RTP connection: %o"), error);
            throw tr("server failed to accept our offer");
        }
        if(this.peer !== peer) { return; }

        this.peer.onnegotiationneeded = () => this.handleNegotiationNeeded();
        this.connectTimeout = setTimeout(() => {
            this.handleFatalError("Connection initialize timeout", true);
        }, 30_000);

        /* Nothing left to do. Server should send a notifyrtcsessiondescription with mode answer */
    }

    private handleConnectionStateChanged(event: ServerConnectionEvents["notify_connection_state_changed"]) {
        if(event.newState === ConnectionState.CONNECTED) {
            /* will be called by the server connection handler */
        } else {
            this.reset(true);
            this.retryCalculator.reset();
        }
    }

    private handleLocalIceCandidate(candidate: RTCIceCandidate | undefined) {
        if(candidate) {
            /*
             * Even if we're only offering local candidates we still should count them else we might
             * get an candidate finish without any candidates (which should never happen).
             * An example for this would be safari.
             */
            this.localCandidateCount++;

            if(candidate.address?.endsWith(".local")) {
                logTrace(LogCategory.WEBRTC, tr("Allowing local fqdn ICE candidate %s"), candidate.toJSON().candidate);
            }

            const json = candidate.toJSON();
            logInfo(LogCategory.WEBRTC, tr("Received local ICE candidate %s"), json.candidate);
            const payload: LocalIceCandidateCommandPayload = {
                media_line: json.sdpMLineIndex,
                candidate: json.candidate
            };
            if(typeof json.sdpMid === "string" && json.sdpMid.length > 0) {
                payload.media_id = json.sdpMid;
            }

            this.connection.send_command("rtcicecandidate", payload).catch(error => {
                logWarn(LogCategory.WEBRTC, tr("Failed to transmit local ICE candidate to server: %o"), error);
            });
        } else {
            this.handleLocalIceCandidateFinish(this.peer).catch(error => {
                logWarn(LogCategory.WEBRTC, tr("Failed to validate local ICE candidate completion: %o"), error);
                this.handleFatalError(this.buildNoLocalIceCandidatesReason(), false);
            });
        }
    }

    public handleRemoteIceCandidate(candidate: RTCIceCandidate | undefined, mediaLine: number) {
        if(!this.peer) {
            if(this.recentlyResetPeer()) {
                logInfo(LogCategory.WEBRTC, tr("Received late remote ICE candidate after a local peer reset. Dropping stale candidate."));
            } else {
                logWarn(LogCategory.WEBRTC, tr("Received remote ICE candidate without an active peer. Dropping candidate."));
            }
            return;
        }

        if(!this.peerRemoteDescriptionReceived) {
            logInfo(LogCategory.WEBRTC, tr("Received remote ICE candidate but haven't yet received a remote description. Caching the candidate."));
            this.cachedRemoteIceCandidates.push({ mediaLine: mediaLine, candidate: candidate });
            return;
        }

        if(!candidate) {
            /* candidates finished */
        } else {
            this.peer.addIceCandidate(candidate).then(() => {
                logInfo(LogCategory.WEBRTC, tr("Successfully added a remote ice candidate for media line %d: %s"), mediaLine, candidate.candidate);
            }).catch(error => {
                logWarn(LogCategory.WEBRTC, tr("Failed to add a remote ice candidate for media line %d: %o (Candidate: %s)"), mediaLine, error, candidate.candidate);
            });
        }
    }

    public applyCachedRemoteIceCandidates() {
        for(const { candidate, mediaLine } of this.cachedRemoteIceCandidates) {
            this.handleRemoteIceCandidate(candidate, mediaLine);
        }

        this.handleRemoteIceCandidate(undefined, 0);
        this.cachedRemoteIceCandidates = [];
    }

    private handleIceCandidateError(event: RTCPeerConnectionIceErrorEvent) {
        this.recentIceCandidateErrors.push({
            errorCode: event.errorCode,
            errorText: event.errorText,
            candidateAddress: event.address || "",
            url: event.url,
            phase: this.peer.iceGatheringState === "gathering" ? "gathering" : "post-gathering",
        });
        if(this.recentIceCandidateErrors.length > 5) {
            this.recentIceCandidateErrors.shift();
        }

        if(this.peer.iceGatheringState === "gathering") {
            logWarn(LogCategory.WEBRTC, tr("Received error while gathering the ice candidates: %d/%s for %s (url: %s)"),
                event.errorCode, event.errorText, event.address, event.url);
        } else {
            logTrace(LogCategory.WEBRTC, tr("Ice candidate %s (%s) errored: %d/%s"),
                event.url, event.address, event.errorCode, event.errorText);
        }
    }
    private handleIceConnectionStateChanged() {
        logInfo(LogCategory.WEBRTC, tr("ICE connection state changed to %s"), this.peer.iceConnectionState);
    }
    private handleIceGatheringStateChanged() {
        logInfo(LogCategory.WEBRTC, tr("ICE gathering state changed to %s"), this.peer.iceGatheringState);
    }

    private handleSignallingStateChanged() {
        logInfo(LogCategory.WEBRTC, tr("Peer signalling state changed to %s"), this.peer.signalingState);
    }
    private handleNegotiationNeeded() {
        logWarn(LogCategory.WEBRTC, tr("Local peer needs negotiation, but that's not supported that."));
    }
    private handlePeerConnectionStateChanged() {
        logInfo(LogCategory.WEBRTC, tr("Peer connection state changed to %s"), this.peer.connectionState);
        switch (this.peer.connectionState) {
            case "connecting":
                this.updateConnectionState(RTPConnectionState.CONNECTING);
                break;

            case "connected":
                clearTimeout(this.connectTimeout);
                this.retryCalculator.reset();
                this.updateConnectionState(RTPConnectionState.CONNECTED);
                break;

            case "failed":
                if(this.connectionState !== RTPConnectionState.FAILED) {
                    this.handleFatalError(tr("peer connection failed"), true);
                }
                break;

            case "closed":
            case "disconnected":
            case "new":
                if(this.connectionState !== RTPConnectionState.FAILED) {
                    this.updateConnectionState(RTPConnectionState.DISCONNECTED);
                }
                break;
        }
    }

    private handleDataChannel(_channel: RTCDataChannel) {
        /* We're not doing anything with data channels */
    }

    private releaseTemporaryStream(ssrc: number) : TemporaryRtpStream | undefined {
        if(this.temporaryStreams[ssrc]) {
            const stream = this.temporaryStreams[ssrc];
            clearTimeout(stream.timeoutId);
            stream.timeoutId = 0;
            delete this.temporaryStreams[ssrc];
            return stream;
        }

        return undefined;
    }

    private handleTrack(event: RTCTrackEvent) {
        const ssrc = this.sdpProcessor.getRemoteSsrcFromFromMediaId(event.transceiver.mid);
        if(typeof ssrc !== "number") {
            logError(LogCategory.WEBRTC, tr("Received track without knowing its ssrc. Ignoring track..."));
            return;
        }

        const tempInfo = this.releaseTemporaryStream(ssrc);
        if(event.track.kind === "audio") {
            if(!this.audioSupport) {
                logWarn(LogCategory.WEBRTC, tr("Received remote audio track %d but audio has been disabled. Dropping track."), ssrc);
                return;
            }

            const track = new InternalRemoteRTPAudioTrack(ssrc, event.transceiver);
            logDebug(LogCategory.WEBRTC, tr("Received remote audio track on ssrc %o"), ssrc);
            if(tempInfo?.info !== undefined) {
                track.handleAssignment(tempInfo.info);
                this.events.fire("notify_audio_assignment_changed", {
                    info: tempInfo.info,
                    track: track
                });
            }
            if(tempInfo?.status !== undefined) {
                track.handleStateNotify(tempInfo.status, tempInfo.info);
            }
            this.remoteAudioTracks[ssrc] = track;
        } else if(event.track.kind === "video") {
            const track = new InternalRemoteRTPVideoTrack(ssrc, event.transceiver);
            logDebug(LogCategory.WEBRTC, tr("Received remote video track on ssrc %o"), ssrc);
            if(tempInfo?.info !== undefined) {
                track.handleAssignment(tempInfo.info);
                this.events.fire("notify_video_assignment_changed", {
                    info: tempInfo.info,
                    track: track
                });
            }
            if(tempInfo?.status !== undefined) {
                track.handleStateNotify(tempInfo.status, tempInfo.info);
            }
            this.remoteVideoTracks[ssrc] = track;
        } else {
            logWarn(LogCategory.WEBRTC, tr("Received track with unknown kind '%s'."), event.track.kind);
        }
    }

    private getOrCreateTempStream(ssrc: number) : TemporaryRtpStream {
        if(this.temporaryStreams[ssrc]) {
            return this.temporaryStreams[ssrc];
        }

        const tempStream = this.temporaryStreams[ssrc] = {
            ssrc: ssrc,
            timeoutId: 0,
            createTimestamp: Date.now(),

            info: undefined,
            status: undefined
        };
        tempStream.timeoutId = setTimeout(() => {
            const mediaLabel = rtcMediaLabel(tempStream.info?.media);
            const clientId = tempStream.info?.client_id;
            const clientName = tempStream.info?.client_name;

            if(tempStream.info?.media === 0 || tempStream.info?.media === 1) {
                logTrace(
                    LogCategory.WEBRTC,
                    tr("Received delayed %s stream mapping for client %s (%s); no matching track surfaced within 30 seconds (ssrc: %o)."),
                    mediaLabel,
                    clientId ?? "unknown",
                    clientName ?? "unknown",
                    ssrc
                );
            } else {
                logWarn(
                    LogCategory.WEBRTC,
                    tr("Received delayed %s stream mapping for client %s (%s); no matching track surfaced within 30 seconds (ssrc: %o)."),
                    mediaLabel,
                    clientId ?? "unknown",
                    clientName ?? "unknown",
                    ssrc
                );
            }
            delete this.temporaryStreams[ssrc];
        }, 30_000);
        return tempStream;
    }

    private doMapStream(ssrc: number, target: TrackClientInfo | undefined) {
        if(this.remoteAudioTracks[ssrc]) {
            const track = this.remoteAudioTracks[ssrc];
            track.handleAssignment(target);
            this.events.fire("notify_audio_assignment_changed", {
                info: target,
                track: track
            });
        } else if(this.remoteVideoTracks[ssrc]) {
            const track = this.remoteVideoTracks[ssrc];
            track.handleAssignment(target);
            this.events.fire("notify_video_assignment_changed", {
                info: target,
                track: track
            });
        } else {
            let tempStream = this.getOrCreateTempStream(ssrc);
            tempStream.info = target;
        }
    }

    private handleStreamState(ssrc: number, state: number, info: TrackClientInfo | undefined) {
        if(this.remoteAudioTracks[ssrc]) {
            const track = this.remoteAudioTracks[ssrc];
            track.handleStateNotify(state, info);
        } else if(this.remoteVideoTracks[ssrc]) {
            const track = this.remoteVideoTracks[ssrc];
            track.handleStateNotify(state, info);
        } else {
            let tempStream = this.getOrCreateTempStream(ssrc);
            if(info && typeof info.media === "undefined") {
                /* the media will only be send on stream assignments, not on stream state changes */
                info.media = tempStream.info?.media;
            }
            tempStream.info = info ?? tempStream.info;
            tempStream.status = state;
        }
    }

    async getConnectionStatistics() : Promise<RTCConnectionStatistics> {
        try {
            if(!this.peer) {
                throw "missing peer";
            }

            const statisticsInfo = await this.peer.getStats();
            const statistics = [...statisticsInfo.entries()].map(e => e[1]) as RTCStats[];
            const inboundStreams = statistics.filter(e => e.type.replace(/-/, "") === "inboundrtp" && 'bytesReceived' in e) as any[];
            const outboundStreams = statistics.filter(e => e.type.replace(/-/, "") === "outboundrtp" && 'bytesSent' in e) as any[];

            if(inboundStreams.length > 0) {
                console.log("Inbound stats object properties:", JSON.parse(JSON.stringify(inboundStreams[0])));
            }

            return {
                voiceBytesSent: outboundStreams.filter(e => e.kind === "audio" || e.mediaType === "audio" || (!e.kind && !e.mediaType)).reduce((a, b) => a + b.bytesSent, 0),
                voiceBytesReceived: inboundStreams.filter(e => e.kind === "audio" || e.mediaType === "audio" || (!e.kind && !e.mediaType)).reduce((a, b) => a + b.bytesReceived, 0),

                videoBytesSent: outboundStreams.filter(e => e.kind === "video" || e.mediaType === "video").reduce((a, b) => a + b.bytesSent, 0),
                videoBytesReceived: inboundStreams.filter(e => e.kind === "video" || e.mediaType === "video").reduce((a, b) => a + b.bytesReceived, 0)
            }
        } catch (error) {
            logWarn(LogCategory.WEBRTC, tr("Failed to calculate connection statistics: %o"), error);
            return {
                videoBytesReceived: 0,
                videoBytesSent: 0,

                voiceBytesReceived: 0,
                voiceBytesSent: 0
            };
        }
    }

    async getVideoBroadcastStatistics(type: RTCBroadcastableTrackType) : Promise<RtcVideoBroadcastStatistics | undefined> {
        if(!this.currentTransceiver[type]?.sender) { return undefined; }

        const senderStatistics = new RTCStatsWrapper(() => this.currentTransceiver[type].sender.getStats());
        await senderStatistics.initialize();
        if(senderStatistics.getValues().length === 0) { return undefined; }

        const trackSettings = this.currentTransceiver[type].sender.track?.getSettings() || {};

        const result = {} as RtcVideoBroadcastStatistics;

        const outboundStream = senderStatistics.getStatisticByType("outboundrtp");
        /* only available in chrome */
        if("codecId" in outboundStream) {
            if(typeof outboundStream.codecId !== "string") { throw tr("invalid codec id type"); }
            const codecInfo = senderStatistics.getStatistic(outboundStream.codecId);
            if(codecInfo?.type !== "codec") { throw tra("invalid/missing codec statistic for codec {}", outboundStream.codecId); }

            if(typeof codecInfo.mimeType !== "string") { throw tr("codec statistic missing mine type"); }
            if(typeof codecInfo.payloadType !== "number") { throw tr("codec statistic has invalid payloadType type"); }

            result.codec = {
                name: codecInfo.mimeType.startsWith("video/") ? codecInfo.mimeType.substr(6) : codecInfo.mimeType || tr("unknown"),
                payloadType: codecInfo.payloadType
            };
        } else {
            /* TODO: Get the only one video type from the sdp */
        }

        if("frameWidth" in outboundStream && "frameHeight" in outboundStream) {
            if(typeof outboundStream.frameWidth !== "number") { throw tr("invalid frameWidth attribute of outboundrtp statistic"); }
            if(typeof outboundStream.frameHeight !== "number") { throw tr("invalid frameHeight attribute of outboundrtp statistic"); }

            result.dimensions = {
                width: outboundStream.frameWidth,
                height: outboundStream.frameHeight
            };
        } else if("height" in trackSettings && "width" in trackSettings) {
            result.dimensions = {
                height: trackSettings.height,
                width: trackSettings.width
            };
        } else {
            result.dimensions = {
                width: 0,
                height: 0
            };
        }

        if("framesPerSecond" in outboundStream) {
            if(typeof outboundStream.framesPerSecond !== "number") { throw tr("invalid framesPerSecond attribute of outboundrtp statistic"); }
            result.frameRate = outboundStream.framesPerSecond;
        } else if("frameRate" in trackSettings) {
            result.frameRate = trackSettings.frameRate;
        } else {
            result.frameRate = 0;
        }

        if("qualityLimitationReason" in outboundStream) {
            /* TODO: verify the value? */
            if(typeof outboundStream.qualityLimitationReason !== "string") { throw tr("invalid qualityLimitationReason attribute of outboundrtp statistic"); }
            result.qualityLimitation = outboundStream.qualityLimitationReason as any;
        } else {
            result.qualityLimitation = "none";
        }

        if("mediaSourceId" in outboundStream) {
            if(typeof outboundStream.mediaSourceId !== "string") { throw tr("invalid media source type"); }
            const source = senderStatistics.getStatistic(outboundStream.mediaSourceId);
            if(source?.type !== "media-source") { throw tra("invalid/missing media source statistic for source {}", outboundStream.mediaSourceId); }

            if(typeof source.width !== "number") { throw tr("invalid width attribute of media-source statistic"); }
            if(typeof source.height !== "number") { throw tr("invalid height attribute of media-source statistic"); }
            if(typeof source.framesPerSecond !== "number") { throw tr("invalid framesPerSecond attribute of media-source statistic"); }

            result.source = {
                dimensions: { height: source.height, width: source.width },
                frameRate: source.framesPerSecond
            };
        } else {
            result.source = {
                dimensions: { width: 0, height: 0 },
                frameRate: 0
            };

            if("height" in trackSettings && "width" in trackSettings) {
                result.source.dimensions = {
                    height: trackSettings.height,
                    width: trackSettings.width
                };
            }

            if("frameRate" in trackSettings) {
                result.source.frameRate = trackSettings.frameRate;
            }
        }

        return result;
    }
}