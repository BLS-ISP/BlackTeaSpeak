import { test, expect, chromium } from '@playwright/test';
import { spawn, ChildProcess } from 'child_process';
import net from 'net';

// Helper to check if the server is running on the query port (TCP 10011)
async function isServerRunning(port: number): Promise<boolean> {
  return new Promise((resolve) => {
    const s = new net.Socket();
    s.setTimeout(1000);
    s.once('error', () => { s.destroy(); resolve(false); });
    s.once('timeout', () => { s.destroy(); resolve(false); });
    s.once('connect', () => { s.destroy(); resolve(true); });
    s.connect(port, '127.0.0.1');
  });
}

test.describe('Functional Server Tests', () => {
  let tauriProcess: ChildProcess;
  let tauriPage: any;
  let browser: any;
  let serverIsUp = false;

  test.beforeAll(async () => {
    serverIsUp = await isServerRunning(10022);
    if (!serverIsUp) {
      console.log('Test server is not running on 127.0.0.1:10022. Skipping functional tests.');
      return;
    }

    // Launch Tauri in dev mode with remote debugging enabled
    tauriProcess = spawn('npm', ['run', 'tauri', 'dev'], {
      env: {
        ...process.env,
        WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9223'
      },
      shell: true
    });

    // Wait for the CDP port to open
    let cdpReady = false;
    for (let i = 0; i < 30; i++) { // Wait up to 30 seconds
      await new Promise(r => setTimeout(r, 1000));
      cdpReady = await isServerRunning(9223);
      if (cdpReady) break;
    }

    if (!cdpReady) {
      throw new Error('Tauri app failed to start or open CDP port 9223');
    }

    // Connect Playwright to the Tauri WebView
    browser = await chromium.connectOverCDP('http://127.0.0.1:9223');
    tauriPage = browser.contexts()[0].pages()[0];
  });

  test.afterAll(async () => {
    if (browser) await browser.close();
    if (tauriProcess) {
      tauriProcess.kill();
    }
  });

  test('Connect to Server and verify UI', async () => {
    test.setTimeout(120000);
    test.skip(!serverIsUp, 'Server is not running');
    
    // Create an identity if none exists, or just use the first one
    await tauriPage.click('text=Identities');
    const hasIdentity = await tauriPage.locator('.list-item').count() > 0;
    if (!hasIdentity) {
      await tauriPage.fill('input[placeholder="New Identity Name"]', 'E2E Tester');
      await tauriPage.click('button:has-text("Generate")');
    }

    // Go to Direct Connect
    await tauriPage.click('text=Direct Connect');
    
    // Fill in server info
    await tauriPage.fill('input[placeholder="e.g. 127.0.0.1:9987"]', '127.0.0.1:9987');
    await tauriPage.locator('.input-group:has-text("Nickname") input').fill('E2ETestBot');
    
    // Check for alerts
    tauriPage.on('dialog', async dialog => {
      console.log('ALERT DIALOG:', dialog.message());
      await dialog.accept();
    });

    // Select an identity by its index to avoid naming issues
    await tauriPage.locator('select').selectOption({ index: 1 });
    
    // Click Connect (retry if needed)
    await expect(async () => {
      await tauriPage.click('button[type="submit"]:has-text("Connect")');
      await expect(tauriPage.locator('text=Disconnect')).toBeVisible({ timeout: 15000 });
    }).toPass({ timeout: 30000 });
    
    // Check status
    const statusText = await tauriPage.locator('.status-message').textContent({ timeout: 10000 }).catch(() => "No status message found");
    console.log("Connection Status:", statusText);

    // Wait for Disconnect button to appear (meaning we connected)
    await expect(tauriPage.locator('text=Disconnect')).toBeVisible({ timeout: 5000 });

    // Verify the ConnectedView successfully mounted and displays the channel tree container
    await expect(tauriPage.locator('text=Server Channels')).toBeVisible({ timeout: 5000 });
    
    // Disconnect
    await tauriPage.click('text=Disconnect');
    await expect(tauriPage.locator('button:has-text("Direct Connect")')).toBeVisible();
  });
});
