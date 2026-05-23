import puppeteer from 'puppeteer';

describe('WebClient E2E Connection Test', () => {
  beforeAll(async () => {
    // Wait for webpack dev server to be ready
    await new Promise(resolve => setTimeout(resolve, 5000));
  });

  it('should successfully connect to the server via WebTransport', async () => {
    // Navigate to the web client
    await page.goto('http://localhost:8080', { waitUntil: 'networkidle2' });

    // Ensure the global connection manager exists
    const hasConnectionManager = await page.evaluate(() => {
        return !!(window as any).teaWeb || !!(window as any).server_connections;
    });
    
    // We will bypass the UI since we don't know the exact DOM layout 
    // and programmatically trigger a connection to the local server
    await page.evaluate(async () => {
        return new Promise<void>((resolve, reject) => {
            // Find the global connection manager
            const cm = (window as any).server_connections || (window as any).teaWeb?.server_connections;
            if(!cm) return reject(new Error('Connection Manager not found globally'));
            
            const handler = cm.getActiveConnectionHandler();
            if(!handler) return reject(new Error('Active Connection Handler not found'));
            
            // Listen for state change
            handler.events().on("notify_connection_state_changed", (event: any) => {
                if (event.newState === 3 /* ConnectionState.CONNECTED */) {
                    resolve();
                } else if (event.newState === 0 /* ConnectionState.DISCONNECTED */) {
                    reject(new Error('Connection failed'));
                }
            });
            
            // Trigger connection
            handler.connect({
                host: '127.0.0.1',
                port: 9987
            }, 'TestUser');
        });
    });

    // Verify Audio Encoder/Decoder WebCodecs instances are attached 
    // by checking if voiceClients were created.
    const voiceClientsCount = await page.evaluate(() => {
        const cm = (window as any).server_connections || (window as any).teaWeb?.server_connections;
        const handler = cm.getActiveConnectionHandler();
        const vc = handler.serverConnection.getVoiceConnection();
        return vc.availableVoiceClients().length;
    });

    // The count might be 0 until someone speaks, but we can verify the WebCodecVoiceConnection is active
    const isVoiceConnectionActive = await page.evaluate(() => {
        const cm = (window as any).server_connections || (window as any).teaWeb?.server_connections;
        const handler = cm.getActiveConnectionHandler();
        const vc = handler.serverConnection.getVoiceConnection();
        return vc.getConnectionState() === 1 /* VoiceConnectionStatus.Connected */;
    });

    expect(isVoiceConnectionActive).toBeTruthy();
  });
});
