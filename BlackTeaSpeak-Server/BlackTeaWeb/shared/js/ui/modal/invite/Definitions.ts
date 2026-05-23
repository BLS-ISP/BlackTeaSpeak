
export type InviteChannel = {
    channelId: number,
    channelName: string,
    depth: number
};

export interface InviteUiVariables {
    shortLink: boolean,
    advancedSettings: boolean,

    selectedChannel: number | 0,
    channelPassword: {
        raw: string | undefined,
        hashed: string | undefined
    },

    token: string | undefined,
    expiresAfter: number | 0,

    webClientUrlBase: { override: string | undefined, fallback: string },

    readonly availableChannels: InviteChannel[],

    readonly generatedLink: {
        status: "generating",
        nativeUrl?: string
    } | {
        status: "error", message: string,
        nativeUrl?: string
    } | {
        status: "success",
        longUrl: string,
        shortUrl: string,
        nativeUrl: string,
        expireDate: number | 0
    }
}

export interface InviteUiEvents {
    action_close: {}
}