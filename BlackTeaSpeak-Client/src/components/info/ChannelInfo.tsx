import React, { useState } from 'react';
import { Channel, FileEntry } from '../../types';

interface ChannelInfoProps {
  selectedChannel: Channel;
  channelFiles: FileEntry[];
  onUploadFile: (file: File) => void;
  onDownloadFile: (entry: FileEntry) => void;
  onDeleteFile: (entry: FileEntry) => void;
  onRefreshFiles: () => void;
}

export function ChannelInfo({ selectedChannel, channelFiles, onUploadFile, onDownloadFile, onDeleteFile, onRefreshFiles }: ChannelInfoProps) {
  const [activeTab, setActiveTab] = useState<'details'|'files'>('details');

  return (
    <div className="info-pane">
      <div className="info-header">
        <h2>{selectedChannel.channel_name}</h2>
        <p>Channel ID: {selectedChannel.cid}</p>
      </div>
      
      <div className="info-tabs">
        <button className={`tab-btn ${activeTab === 'details' ? 'active' : ''}`} onClick={() => setActiveTab('details')}>Details</button>
        <button className={`tab-btn ${activeTab === 'files' ? 'active' : ''}`} onClick={() => { setActiveTab('files'); onRefreshFiles(); }}>Files</button>
      </div>

      <div className="info-body">
        {activeTab === 'details' && (
          <div className="info-row">
            <span>Topic:</span>
            <span>{selectedChannel.channel_topic || 'No topic set'}</span>
          </div>
        )}

        {activeTab === 'files' && (
          <div className="file-browser">
            <div className="file-actions">
              <input type="file" id="upload-input" style={{display: 'none'}} onChange={(e) => {
                if (e.target.files && e.target.files[0]) {
                  onUploadFile(e.target.files[0]);
                  e.target.value = ''; // Reset
                }
              }} />
              <button className="btn-secondary" onClick={() => document.getElementById('upload-input')?.click()}>Upload File</button>
              <button className="btn-secondary" onClick={onRefreshFiles}>Refresh</button>
            </div>
            
            <ul className="file-list">
              {channelFiles.map(file => (
                <li key={file.name} className="file-item">
                  <span className="file-icon">{file.type === 0 ? '📁' : '📄'}</span>
                  <span className="file-name">{file.name}</span>
                  {file.type === 1 && <span className="file-size">{(file.size / 1024).toFixed(1)} KB</span>}
                  
                  {file.type === 1 && (
                    <div className="file-item-actions">
                      <button onClick={() => onDownloadFile(file)} title="Download">⬇️</button>
                      <button onClick={() => onDeleteFile(file)} title="Delete">🗑️</button>
                    </div>
                  )}
                </li>
              ))}
              {channelFiles.length === 0 && <p>No files found.</p>}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
}
