import { test, expect } from '@playwright/test';

test.beforeEach(async ({ page }) => {
  // Mock Tauri APIs
  await page.addInitScript(() => {
    (window as any).__TAURI_INTERNALS__ = {
      invoke: async (cmd: string, args: any) => {
        if (cmd === 'load_config') {
          return {
            identities: [{
              id: '1', name: 'Test Identity', default_nickname: 'Tester',
              uid: 'test-uid-1234567890', public_key: 'pub', private_key: 'priv'
            }],
            favorites: [{
              id: 'f1', name: 'Test Server', address: '127.0.0.1:9987',
              nickname: 'Tester', identity_id: '1'
            }]
          };
        }
        if (cmd === 'connect_to_server') {
          return "Success";
        }
        if (cmd === 'generate_identity') {
          return {
            id: '2', name: args.name, default_nickname: 'NewUser',
            uid: 'new-uid', public_key: 'pub', private_key: 'priv'
          };
        }
        if (cmd === 'get_audio_devices') {
          return { inputs: ['Mocked Mic'], outputs: ['Mocked Speaker'] };
        }
        // Return dummy data for other commands
        return {};
      }
    };
    (window as any).__TAURI_IPC__ = () => {};
    (window as any).__TAURI_EVENT__ = {
      listen: async () => () => {},
      emit: async () => {}
    };
    // Mock global shortcut
    (window as any).__TAURI_PLUGIN_GLOBAL_SHORTCUT__ = {
      register: async () => {},
      unregister: async () => {},
      isRegistered: async () => false
    };
  });
});

test.describe('Visual Regression Tests', () => {

  test('App Home - Favorites Tab', async ({ page }) => {
    await page.goto('/');
    // Wait for favorites to load
    await expect(page.locator('text=Test Server')).toBeVisible();
    await expect(page).toHaveScreenshot('app-home-favorites.png', { fullPage: true });
  });

  test('App Home - Identities Tab', async ({ page }) => {
    await page.goto('/');
    await page.click('text=Identities');
    await expect(page.locator('text=Test Identity')).toBeVisible();
    await expect(page).toHaveScreenshot('app-home-identities.png', { fullPage: true });
  });

  test('Connected View - Main Layout', async ({ page }) => {
    await page.goto('/');
    // Click connect on the favorite
    await page.click('.card >> text=Connect');
    
    // ConnectedView should appear
    await expect(page.locator('text=Disconnect')).toBeVisible();
    
    // Give it a moment to render tree and panels
    await page.waitForTimeout(500); 
    
    await expect(page).toHaveScreenshot('connected-view-layout.png', { fullPage: true });
  });

  test('Connected View - Settings Modal', async ({ page }) => {
    await page.goto('/');
    await page.click('.card >> text=Connect');
    await expect(page.locator('text=Disconnect')).toBeVisible();
    
    // Open Settings
    await page.click('button:has-text("⚙️ Settings")');
    await expect(page.locator('.settings-modal')).toBeVisible();
    
    await expect(page).toHaveScreenshot('settings-modal.png');
  });

  test('Connected View - Group Manager Modal', async ({ page }) => {
    await page.goto('/');
    await page.click('.card >> text=Connect');
    await expect(page.locator('text=Disconnect')).toBeVisible();
    
    await page.click('button:has-text("👥 Groups")');
    await expect(page.locator('h2:has-text("Group Manager")')).toBeVisible();
    
    await expect(page).toHaveScreenshot('group-manager-modal.png');
  });

  test('Connected View - Ban Manager Modal', async ({ page }) => {
    await page.goto('/');
    await page.click('.card >> text=Connect');
    await expect(page.locator('text=Disconnect')).toBeVisible();
    
    await page.click('button:has-text("🔨 Bans")');
    await expect(page.locator('h2:has-text("Ban Manager")')).toBeVisible();
    
    await expect(page).toHaveScreenshot('ban-manager-modal.png');
  });

  test('Connected View - Token Manager Modal', async ({ page }) => {
    await page.goto('/');
    await page.click('.card >> text=Connect');
    await expect(page.locator('text=Disconnect')).toBeVisible();
    
    await page.click('button:has-text("🛡️ Manage Tokens")');
    await expect(page.locator('h2:has-text("Privilege Keys")')).toBeVisible();
    
    await expect(page).toHaveScreenshot('token-manager-modal.png');
  });
});
