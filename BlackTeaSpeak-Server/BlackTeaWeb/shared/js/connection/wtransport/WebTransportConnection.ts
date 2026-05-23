import {LogCategory, logWarn, logInfo, logError} from "tc-shared/log";
import {ConnectionStatistics} from "tc-shared/connection/ConnectionBase";
import {tr} from "tc-shared/i18n/localize";

export type WebTransportUrl = {
    host: string;
    port: number;
    path?: string;
};

export class WrappedWebTransport {
    public readonly address: WebTransportUrl;
    public state: "unconnected" | "connecting" | "connected" | "errored";

    private transport?: any;
    private bidiStream?: any;
    private reader?: any;
    private writer?: any;
    
    private datagramReader?: any;
    private datagramWriter?: any;

    public callbackMessage?: (message: string) => void;
    public callbackDatagram?: (message: Uint8Array) => void;
    public callbackDisconnect?: (code: number, reason?: string) => void;
    public callbackErrored?: () => void;

    private errorQueue: Error[] = [];
    private connectResultListener: (() => void)[] = [];

    private bytesReceived = 0;
    private bytesSend = 0;

    constructor(addr: WebTransportUrl) {
        this.address = addr;
        this.state = "unconnected";
    }

    getControlStatistics(): ConnectionStatistics {
        return {
            bytesReceived: this.bytesReceived,
            bytesSend: this.bytesSend
        };
    }

    socketUrl(): string {
        let result = `https://${this.address.host}:${this.address.port}`;
        if(this.address.path) {
            result += (this.address.path.startsWith("/") ? "" : "/") + this.address.path;
        }
        return result;
    }

    doConnect() {
        this.closeConnection();
        this.state = "connecting";

        try {
            // @ts-ignore
            this.transport = new WebTransport(this.socketUrl());
            
            this.transport.ready.then(async () => {
                this.state = "connected";
                
                this.bidiStream = await this.transport.createBidirectionalStream();
                this.reader = this.bidiStream.readable.getReader();
                this.writer = this.bidiStream.writable.getWriter();
                
                this.datagramReader = this.transport.datagrams.readable.getReader();
                this.datagramWriter = this.transport.datagrams.writable.getWriter();

                this.fireConnectResult();

                this.readCommandLoop();
                this.readDatagramLoop();
            }).catch(error => {
                this.state = "errored";
                this.errorQueue.push(error);
                this.fireConnectResult();
            });

            this.transport.closed.then((info) => {
                if (this.state === "connecting") {
                    this.state = "errored";
                    this.errorQueue.push(new Error(tr("WebTransport closed early ") + JSON.stringify(info)));
                    this.fireConnectResult();
                } else if (this.state === "connected") {
                    if (this.callbackDisconnect) {
                        this.callbackDisconnect(0, "Connection closed");
                    }
                    this.closeConnection();
                }
            }).catch(err => {
                this.state = "errored";
                if(this.callbackErrored) this.callbackErrored();
                this.fireConnectResult();
            });

        } catch (error) {
            this.state = "errored";
            this.errorQueue.push(error as Error);
            this.fireConnectResult();
        }
    }

    private async readCommandLoop() {
        let buffer = "";
        const decoder = new TextDecoder();
        
        try {
            while (this.reader) {
                const { value, done } = await this.reader.read();
                if (done) break;
                
                this.bytesReceived += value.byteLength;
                buffer += decoder.decode(value, { stream: true });
                
                let newlineIndex;
                while ((newlineIndex = buffer.indexOf('\n')) !== -1) {
                    const line = buffer.substring(0, newlineIndex);
                    buffer = buffer.substring(newlineIndex + 1);
                    if(line.trim().length > 0 && this.callbackMessage) {
                        this.callbackMessage(line);
                    }
                }
            }
        } catch (e) {
            logError(LogCategory.NETWORKING, "Error reading commands: %o", e);
        }
    }

    private async readDatagramLoop() {
        try {
            while (this.datagramReader) {
                const { value, done } = await this.datagramReader.read();
                if (done) break;
                
                this.bytesReceived += value.byteLength;
                if(this.callbackDatagram) {
                    this.callbackDatagram(value);
                }
            }
        } catch(e) {
            logError(LogCategory.NETWORKING, "Error reading datagrams: %o", e);
        }
    }

    async awaitConnectResult() {
        while (this.state === "connecting") {
            await new Promise<void>(resolve => this.connectResultListener.push(resolve));
        }
    }

    closeConnection() {
        this.state = "unconnected";

        if(this.reader) {
            this.reader.cancel().catch(() => {});
            this.reader = undefined;
        }
        if(this.writer) {
            this.writer.close().catch(() => {});
            this.writer = undefined;
        }
        if(this.datagramReader) {
            this.datagramReader.cancel().catch(() => {});
            this.datagramReader = undefined;
        }
        if(this.datagramWriter) {
            this.datagramWriter.close().catch(() => {});
            this.datagramWriter = undefined;
        }
        if(this.transport) {
            this.transport.close();
            this.transport = undefined;
        }
        this.bidiStream = undefined;

        this.bytesReceived = 0;
        this.bytesSend = 0;
        this.errorQueue = [];
        this.fireConnectResult();
    }

    private fireConnectResult() {
        while(this.connectResultListener.length > 0)
            this.connectResultListener.pop()!();
    }

    hasError() {
        return this.errorQueue.length !== 0;
    }

    popError() {
        return this.errorQueue.shift();
    }

    sendMessage(message: string | Uint8Array) {
        if(!this.writer) return;

        if(typeof message === "string") {
            const encoded = new TextEncoder().encode(message + "\n");
            this.bytesSend += encoded.byteLength;
            this.writer.write(encoded).catch(e => {
                logError(LogCategory.NETWORKING, "Failed to write text message: %o", e);
            });
        } else {
            this.bytesSend += message.byteLength;
            this.writer.write(message).catch(e => {
                logError(LogCategory.NETWORKING, "Failed to write binary message: %o", e);
            });
        }
    }

    sendDatagram(message: Uint8Array) {
        if(!this.datagramWriter) return;
        this.bytesSend += message.byteLength;
        this.datagramWriter.write(message).catch(e => {
             logError(LogCategory.NETWORKING, "Failed to write datagram: %o", e);
        });
    }
}
