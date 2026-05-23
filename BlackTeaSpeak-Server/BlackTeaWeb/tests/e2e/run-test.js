const puppeteer = require('puppeteer');
const { spawn } = require('child_process');
const crypto = require('crypto');
const fs = require('fs');

(async () => {
    console.log("Starting WebClient E2E Test...");
    
    console.log("Starting BlackTeaSpeak Server...");
    const serverProcess = spawn('D:\\projekt\\BlackTeaSpeak\\BlackTeaSpeak-Server\\target\\debug\\blackteaspeak_server.exe', ['serve-all'], {
        cwd: 'D:\\projekt\\BlackTeaSpeak\\BlackTeaSpeak-Server'
    });
    serverProcess.stdout.on('data', data => console.log('SERVER OUT:', data.toString()));
    serverProcess.stderr.on('data', data => console.error('SERVER ERR:', data.toString()));
    
    // Wait for the server to initialize
    await new Promise(r => setTimeout(r, 2000));

    
    const browser = await puppeteer.launch({
        headless: true,
        args: [
            '--no-sandbox',
            '--disable-setuid-sandbox',
            '--use-fake-ui-for-media-stream',
            '--use-fake-device-for-media-stream',
            '--allow-file-access-from-files',
            '--ignore-certificate-errors',
            '--ignore-certificate-errors-spki-list=PRm30mloEpUo/cHAli1/An2Z28PfehTd+VukhO6SrP4='
        ]
    });
    
    console.log("Browser launched. Navigating to WebClient...");
    const page = await browser.newPage();
    page.on('console', msg => console.log('PAGE LOG:', msg.text()));

    // Generate certificate hash for WebTransport
    const cert = fs.readFileSync('D:\\projekt\\BlackTeaSpeak\\BlackTeaSpeak-Server\\data\\tls\\blackteaweb-localhost-cert.pem');
    const x509 = new crypto.X509Certificate(cert);
    const hashBuffer = crypto.createHash('sha256').update(x509.raw).digest();

    await page.evaluateOnNewDocument((hashArray) => {
        const certHash = new Uint8Array(hashArray).buffer;
        const OriginalWebTransport = window.WebTransport;
        
        window.WebTransport = function(url, options = {}) {
            options.serverCertificateHashes = [
                { algorithm: "sha-256", value: certHash }
            ];
            return new OriginalWebTransport(url, options);
        };
    }, Array.from(hashBuffer));
    
    // Workaround: Cache certificate exception for the WebTransport port
    await page.goto('https://127.0.0.1:9988', { waitUntil: 'domcontentloaded' }).catch(() => {});
    
    // Webpack Dev Server should be running on localhost:8080
    await page.goto('https://127.0.0.1:8080/?connect_address=127.0.0.1:9988&nickname=TestUser', { waitUntil: 'networkidle2' });
    console.log("Page loaded. Auto-connect should trigger via URL parameters.");

    try {
        console.log("Waiting for backend connection...");
        const isVoiceConnectionActive = await page.evaluate(async () => {
            return new Promise((resolve) => {
                const checkInterval = setInterval(() => {
                    const cm = window.server_connections || window.teaWeb?.server_connections;
                    if (cm) {
                        const handler = cm.getActiveConnectionHandler();
                        if (handler && handler.serverConnection) {
                            const vc = handler.serverConnection.getVoiceConnection();
                            if (vc && vc.getConnectionState() === 3 /* VoiceConnectionStatus.Connected */) {
                                clearInterval(checkInterval);
                                resolve(true);
                            } else if (handler.connection_state === 0 /* DISCONNECTED */ && handler.connectAttemptId > 1) {
                                // If it disconnected and failed
                                clearInterval(checkInterval);
                                resolve(false);
                            }
                        }
                    }
                }, 500);

                // Timeout after 15 seconds
                setTimeout(() => {
                    clearInterval(checkInterval);
                    resolve(false);
                }, 15000);
            });
        });

        if (isVoiceConnectionActive) {
            console.log("✅ WebCodecVoiceConnection initialized and active!");
            process.exit(0);
        } else {
            console.error("❌ VoiceConnection is NOT active after timeout or failed.");
            process.exit(1);
        }

    } catch (e) {
        console.error("❌ Test failed:", e.message);
        process.exit(1);
    } finally {
        await browser.close();
        if (serverProcess) {
            console.log("Killing server...");
            serverProcess.kill();
        }
    }
})();
