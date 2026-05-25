import { useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { fetch as tauriFetch } from '@tauri-apps/plugin-http';
import { escapeTs3String } from '../ts3parser';
import { FileEntry } from '../types';
import { Toast } from '../ui/Toast';
import { Dialogs } from '../ui/Dialogs';

export function useFileTransfers(
  selectedChannelCid: string | undefined, 
  setChannelFiles: React.Dispatch<React.SetStateAction<FileEntry[]>>,
  setAvatarCache: React.Dispatch<React.SetStateAction<Record<string, string>>>
) {
  const pendingTransfers = useRef<Map<string, { type: 'upload' | 'download' | 'avatar_upload' | 'avatar_download', file?: File, fileEntry?: FileEntry, avatarHash?: string }>>(new Map());

  const refreshFiles = (cid?: string) => {
    const targetCid = cid || selectedChannelCid;
    if (targetCid) {
      setChannelFiles([]);
      invoke('send_command', { command: `ftgetfilelist cid=${targetCid} cpw= path=\\/` });
    }
  };

  const executeFileTransfer = async (transfer: any, ftkey: string, port: number) => {
    const baseUrl = `https://127.0.0.1:${port}`;
    try {
      if ((transfer.type === 'upload' || transfer.type === 'avatar_upload') && transfer.file) {
        await tauriFetch(`${baseUrl}/upload?transfer-key=${ftkey}`, {
          method: 'POST',
          body: transfer.file,
          headers: { 'Content-Type': 'application/octet-stream' },
          danger: { acceptInvalidCerts: true }
        });
        if (transfer.type === 'upload') {
          refreshFiles();
        } else if (transfer.type === 'avatar_upload') {
          invoke('send_command', { command: `clientupdate client_flag_avatar=${transfer.avatarHash}` });
        }
      } else if (transfer.type === 'download' && transfer.fileEntry) {
        const resp = await tauriFetch(`${baseUrl}/download?transfer-key=${ftkey}`, {
          danger: { acceptInvalidCerts: true }
        });
        const blob = await resp.blob();
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = transfer.fileEntry.name;
        a.click();
        URL.revokeObjectURL(url);
      } else if (transfer.type === 'avatar_download' && transfer.avatarHash) {
        const resp = await tauriFetch(`${baseUrl}/download?transfer-key=${ftkey}`, {
          danger: { acceptInvalidCerts: true }
        });
        const blob = await resp.blob();
        const url = URL.createObjectURL(blob);
        setAvatarCache(prev => ({ ...prev, [transfer.avatarHash]: url }));
      }
    } catch (e) {
      console.error("File transfer failed:", e);
      Toast.error("File transfer failed: " + e);
    }
  };

  const handleUploadFile = (file: File, targetCid?: string) => {
    const cid = targetCid || selectedChannelCid;
    if (!cid) return;
    const clientftfid = Math.floor(Math.random() * 10000).toString();
    pendingTransfers.current.set(clientftfid, { type: 'upload', file });
    invoke('send_command', { command: `ftinitupload clientftfid=${clientftfid} name=\\/${escapeTs3String(file.name)} cid=${cid} size=${file.size} overwrite=1 resume=0` });
  };

  const handleDownloadFile = (entry: FileEntry) => {
    if (!selectedChannelCid) return;
    const clientftfid = Math.floor(Math.random() * 10000).toString();
    pendingTransfers.current.set(clientftfid, { type: 'download', fileEntry: entry });
    invoke('send_command', { command: `ftinitdownload clientftfid=${clientftfid} name=\\/${escapeTs3String(entry.name)} cid=${selectedChannelCid} cpw= seekpos=0 proto=0` });
  };

  const handleDeleteFile = async (entry: FileEntry) => {
    if (!selectedChannelCid) return;
    if (await Dialogs.confirm('Delete File', `Are you sure you want to delete ${entry.name}?`)) {
      invoke('send_command', { command: `ftdeletefile cid=${selectedChannelCid} cpw= name=\\/${escapeTs3String(entry.name)}` });
      setTimeout(() => refreshFiles(), 500);
      Toast.success(`File ${entry.name} deleted.`);
    }
  };

  const handleUploadAvatar = (file: File) => {
    const avatarHash = crypto.randomUUID().replace(/-/g, '');
    const clientftfid = Math.floor(Math.random() * 10000).toString();
    pendingTransfers.current.set(clientftfid, { type: 'avatar_upload', file, avatarHash });
    invoke('send_command', { command: `ftinitupload clientftfid=${clientftfid} name=\\/avatar_${avatarHash} cid=0 cpw= size=${file.size} overwrite=1 resume=0` });
  };

  const fetchAvatar = (avatarHash: string, currentCache: Record<string, string>) => {
    if (currentCache[avatarHash]) return;
    const clientftfid = Math.floor(Math.random() * 10000).toString();
    pendingTransfers.current.set(clientftfid, { type: 'avatar_download', avatarHash });
    invoke('send_command', { command: `ftinitdownload clientftfid=${clientftfid} name=\\/avatar_${avatarHash} cid=0 cpw= seekpos=0 proto=0` });
  };

  return {
    pendingTransfers,
    executeFileTransfer,
    handleUploadFile,
    handleDownloadFile,
    handleDeleteFile,
    handleUploadAvatar,
    fetchAvatar,
    refreshFiles
  };
}
