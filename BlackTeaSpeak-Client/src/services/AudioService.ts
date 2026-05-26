export class AudioService {
    private static enabled = true;
    private static lastPlayed: Record<string, number> = {};
    private static cooldownMs = 1000;

    private static play(path: string) {
        if (!this.enabled) return;

        const now = Date.now();
        if (this.lastPlayed[path] && now - this.lastPlayed[path] < this.cooldownMs) {
            return; // prevent spam
        }

        const audio = new Audio(`/audio/${path}`);
        audio.play().catch(e => console.warn('Audio play failed', e));
        this.lastPlayed[path] = now;
    }

    public static playConnected() { this.play('speech/connection.connected.wav'); }
    public static playDisconnected() { this.play('speech/connection.disconnected.wav'); }
    public static playUserJoined() { this.play('speech/user.joined.wav'); }
    public static playUserLeft() { this.play('speech/user.left.wav'); }
    public static playUserMoved() { this.play('speech/user.moved.wav'); }
    public static playUserMovedSelf() { this.play('speech/user.moved.self.wav'); }
    public static playChannelJoined() { this.play('speech/channel.joined.wav'); }
    public static playMessageReceived() { this.play('effects/message_received.wav'); }
    public static playMicMuted() { this.play('speech/microphone.muted.wav'); }
    public static playMicActivated() { this.play('speech/microphone.activated.wav'); }
    public static playSpeakerMuted() { this.play('speech/sound.muted.wav'); }
    public static playSpeakerActivated() { this.play('speech/sound.activated.wav'); }
}
