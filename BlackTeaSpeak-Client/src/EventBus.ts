import { listen } from '@tauri-apps/api/event';
import { parseTs3Response, Ts3ResponseRow } from './ts3parser';

export type ServerEventCallback = (rows: Ts3ResponseRow[], rawPayload: string) => void;

class ServerEventBus {
  private listeners: Set<ServerEventCallback> = new Set();
  private readyPromise: Promise<void> | null = null;

  public subscribe(callback: ServerEventCallback): () => void {
    this.listeners.add(callback);
    this.ensureListening();
    
    return () => {
      this.listeners.delete(callback);
    };
  }

  public async waitForReady(): Promise<void> {
    await this.ensureListening();
  }

  private ensureListening(): Promise<void> {
    if (this.readyPromise) return this.readyPromise;

    this.readyPromise = new Promise((resolve) => {
      listen<string>('server_event', (event) => {
        if (this.listeners.size === 0) return;
      
      const payload = event.payload;
      try {
        const parsed = parseTs3Response(payload);
        this.listeners.forEach(cb => {
          try {
            cb(parsed, payload);
          } catch (e) {
            console.error("Error in server event listener:", e);
          }
        });
      } catch (err) {
        console.error("Failed to parse server_event payload:", err);
      }
    }).then(() => {
      resolve();
    }).catch(err => {
      console.error("Failed to setup Tauri listen:", err);
      resolve(); // resolve anyway so we don't block forever
    });
    });

    return this.readyPromise;
  }
}

export const eventBus = new ServerEventBus();
