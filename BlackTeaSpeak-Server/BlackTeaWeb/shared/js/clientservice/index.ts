import * as loader from "tc-loader";
import {Stage} from "tc-loader";
import type {ClientServiceInvite, ClientServices} from "tc-services";

type DisabledClientServiceResult<T = never> = {
    status: "error",
    result: { type: "ClientSessionUninitialized" },
    unwrap(): T,
};

function disabledResult<T = never>() : DisabledClientServiceResult<T> {
    return {
        status: "error",
        result: { type: "ClientSessionUninitialized" },
        unwrap() {
            throw new Error("Client services are disabled in this build");
        }
    };
}

function successResult<T>(value: T) {
    return {
        status: "success" as const,
        result: value,
        unwrap() {
            return value;
        }
    };
}

export const clientServices = {
    isSessionInitialized() {
        return false;
    },
    awaitSession() {
        return new Promise<void>(() => undefined);
    },
    start() {
        /* Client services are intentionally disabled in this build. */
    }
} as Pick<ClientServices, "isSessionInitialized" | "awaitSession" | "start">;

export const clientServiceInvite = {
    async logAction(..._args: unknown[]) {
        return successResult<void>(undefined);
    },
    async createInviteLink(..._args: unknown[]) {
        return disabledResult();
    },
    async queryInviteLink(..._args: unknown[]) {
        return disabledResult();
    }
} as Pick<ClientServiceInvite, "logAction" | "createInviteLink" | "queryInviteLink">;

loader.register_task(Stage.JAVASCRIPT_INITIALIZING, {
    priority: 30,
    function: async () => {
        (globalThis as any).clientServices = clientServices;
        (globalThis as any).clientServiceInvite = clientServiceInvite;
    },
    name: "client services disabled"
});