import { useState, useEffect, useRef } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { FileEntry } from './types';
import { Dialogs } from './ui/Dialogs';
import { Toast } from './ui/Toast';
import { eventBus } from './EventBus';

interface FileBrowserModalProps {
  channelId: string;
  onClose: () => void;
}

export function FileBrowserModal({ channelId, onClose }: FileBrowserModalProps) {
  const [files, setFiles] = useState<FileEntry[]>([]);
  const [currentPath, setCurrentPath] = useState("/");
  const [isLoading, setIsLoading] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    const unsubscribe = eventBus.subscribe((rows) => {
      for (const row of rows) {
        if (row.command === 'unknown' && row.args.name !== undefined && row.args.size !== undefined) {
          // This is a file list row!
          setFiles(prev => {
            const entry: FileEntry = {
              name: row.args.name,
              size: parseInt(row.args.size || "0"),
              datetime: parseInt(row.args.datetime || "0"),
              type: parseInt(row.args.type || "1"),
              empty: row.args.empty === "1"
            };
            
            if (prev.some(p => p.name === entry.name)) return prev;
            return [...prev, entry];
          });
        }
        if (row.command === 'error') {
          setIsLoading(false);
        }
        
        // Listen for ftinitupload and ftinitdownload responses
        if (row.command === 'unknown' && row.args.ftkey !== undefined) {
          const { ftkey, port, serverftfid, clientftfid, size } = row.args;
          const isUpload = !row.args.size; // Server doesn't echo size on upload response immediately? Actually we can check clientftfid
          
          handleFileTransferReady(
            clientftfid, 
            ftkey, 
            parseInt(port), 
            parseInt(size || "0"), 
            row.args.ip || "127.0.0.1"
          );
        }
      }
    });

    refreshFiles();

    return () => {
      unsubscribe();
    };
  }, [channelId, currentPath]);

  const refreshFiles = () => {
    setIsLoading(true);
    setFiles([]);
    invoke('send_command', { command: `ftlist cid=${channelId} path=${currentPath}` });
  };

  // We need a map of pending transfers because ftinitupload is asynchronous
  const pendingTransfers = useRef<Record<string, { type: 'upload'|'download', name: string, data?: Uint8Array }>>({});
  const nextFtId = useRef(1);

  const handleUploadClick = () => {
    fileInputRef.current?.click();
  };

  const handleFileChange = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) return;

    const arrayBuffer = await file.arrayBuffer();
    const bytes = new Uint8Array(arrayBuffer);
    
    const ftId = nextFtId.current++;
    pendingTransfers.current[ftId.toString()] = {
      type: 'upload',
      name: file.name,
      data: bytes
    };

    Toast.success(`Starting upload: ${file.name}`);
    
    // Convert path spaces etc. TS3 escaping:
    const escapedName = file.name.replace(/ /g, '\\s');
    invoke('send_command', { 
      command: `ftinitupload clientftfid=${ftId} name=${currentPath}${escapedName} cid=${channelId} size=${file.size} overwrite=1` 
    });
  };

  const handleDownload = (filename: string) => {
    const ftId = nextFtId.current++;
    pendingTransfers.current[ftId.toString()] = {
      type: 'download',
      name: filename
    };

    Toast.success(`Starting download: ${filename}`);
    const escapedName = filename.replace(/ /g, '\\s');
    invoke('send_command', { 
      command: `ftinitdownload clientftfid=${ftId} name=${currentPath}${escapedName} cid=${channelId}` 
    });
  };

  const handleDelete = async (filename: string) => {
    if (await Dialogs.confirm("Delete File", `Are you sure you want to delete ${filename}?`)) {
      const escapedName = filename.replace(/ /g, '\\s');
      invoke('send_command', { command: `ftdelete cid=${channelId} cpw= name=${currentPath}${escapedName}` });
      setTimeout(refreshFiles, 500);
    }
  };

  const handleFileTransferReady = async (clientftfid: string, ftkey: string, port: number, size: number, ip: string) => {
    const transfer = pendingTransfers.current[clientftfid];
    if (!transfer) return;

    try {
      if (transfer.type === 'upload' && transfer.data) {
        // We pass the raw bytes directly to Rust as a number array
        await invoke('upload_file', { 
          ip, port, ftkey, size: transfer.data.length, fileData: Array.from(transfer.data), clientFtId: parseInt(clientftfid) 
        });
        Toast.success(`Upload complete: ${transfer.name}`);
        setTimeout(refreshFiles, 500);
      } else if (transfer.type === 'download') {
        const bytes: number[] = await invoke('download_file', { 
          ip, port, ftkey, size, clientFtId: parseInt(clientftfid) 
        });
        
        // Trigger browser download!
        const blob = new Blob([new Uint8Array(bytes)]);
        const url = URL.createObjectURL(blob);
        const a = document.createElement('a');
        a.href = url;
        a.download = transfer.name;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
        
        Toast.success(`Download complete: ${transfer.name}`);
      }
    } catch (e: any) {
      Toast.error(`Transfer failed: ${e.toString()}`);
    }
    
    delete pendingTransfers.current[clientftfid];
  };

  const formatSize = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i];
  };

  const formatDate = (timestamp: number) => {
    return new Date(timestamp * 1000).toLocaleString();
  };

  return (
    <div className="modal-overlay" style={{ zIndex: 1000, display: 'flex', alignItems: 'center', justifyContent: 'center' }}>
      <div className="modal-content" style={{ width: '800px', height: '600px', display: 'flex', flexDirection: 'column' }}>
        
        <div className="info-header" style={{ marginBottom: '12px', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <h3 style={{ margin: 0 }}>Files - Channel {channelId}</h3>
          <div>
            <input type="file" ref={fileInputRef} style={{ display: 'none' }} onChange={handleFileChange} />
            <button className="btn-secondary" onClick={handleUploadClick} style={{ marginRight: '8px' }}>⬆ Upload File</button>
            <button className="btn-primary" onClick={refreshFiles}>⟳ Refresh</button>
          </div>
        </div>

        <div style={{ padding: '8px', background: '#111', border: '1px solid #333', marginBottom: '8px', borderRadius: '4px' }}>
          Path: <strong>{currentPath}</strong>
        </div>

        <div className="tree-table-header" style={{ display: 'flex', borderBottom: '2px solid #444', padding: '8px', fontWeight: 'bold', fontSize: '12px', color: '#aaa' }}>
          <div style={{ flex: 1 }}>Name</div>
          <div style={{ width: '100px', textAlign: 'right' }}>Size</div>
          <div style={{ width: '150px', textAlign: 'right' }}>Date</div>
          <div style={{ width: '80px', textAlign: 'center' }}>Actions</div>
        </div>

        <div className="list-view" style={{ flexGrow: 1, overflowY: 'auto', background: '#1a1a1a' }}>
          {files.map((f, idx) => (
            <div key={idx} style={{ display: 'flex', borderBottom: '1px solid #333', padding: '8px', alignItems: 'center' }}>
              <div style={{ flex: 1, display: 'flex', alignItems: 'center' }}>
                <span style={{ marginRight: '8px', fontSize: '16px' }}>{f.type === 0 ? '📁' : '📄'}</span>
                {f.name}
              </div>
              <div style={{ width: '100px', textAlign: 'right', color: '#888', fontSize: '12px' }}>
                {f.type === 1 ? formatSize(f.size) : '--'}
              </div>
              <div style={{ width: '150px', textAlign: 'right', color: '#888', fontSize: '12px' }}>
                {formatDate(f.datetime)}
              </div>
              <div style={{ width: '80px', display: 'flex', justifyContent: 'center', gap: '8px' }}>
                {f.type === 1 && (
                  <button className="btn-icon muted" onClick={() => handleDownload(f.name)}>⬇</button>
                )}
                <button className="btn-icon muted" onClick={() => handleDelete(f.name)}>🗑</button>
              </div>
            </div>
          ))}
          {files.length === 0 && !isLoading && (
            <div style={{ padding: '20px', textAlign: 'center', color: '#888' }}>This directory is empty.</div>
          )}
          {isLoading && files.length === 0 && (
            <div style={{ padding: '20px', textAlign: 'center', color: '#888' }}>Loading files...</div>
          )}
        </div>

        <div className="form-actions" style={{ marginTop: '16px' }}>
          <button className="btn-primary" onClick={onClose}>Close</button>
        </div>

      </div>
    </div>
  );
}
