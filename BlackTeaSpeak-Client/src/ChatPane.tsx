import React, { useState, useRef } from 'react';
import { ChatMessage, FileEntry } from './types';
import { Avatar } from './Avatar';
import { RichText } from './RichText';
import { EmojiPicker } from './EmojiPicker';

interface ChatPaneProps {
  messages: ChatMessage[];
  onSendMessage: (targetMode: number, target: string, message: string) => void;
  myClientId: string;
  currentChannelId: string;
  currentClientId?: string;
  onUploadFile?: (file: File, targetCid?: string) => void;
}

export function ChatPane({ messages, onSendMessage, myClientId, currentChannelId, currentClientId, onUploadFile }: ChatPaneProps) {
  const [activeTab, setActiveTab] = useState<number>(3); // 3 = Server, 2 = Channel
  const [inputText, setInputText] = useState('');
  const [showEmojiPicker, setShowEmojiPicker] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);

  const filteredMessages = messages.filter(m => m.targetMode === activeTab);

  const handleSend = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputText.trim()) return;

    let target = '0';
    if (activeTab === 2) target = currentChannelId;
    if (activeTab === 1) target = currentClientId || '0';

    onSendMessage(activeTab, target, inputText);
    setInputText('');
  };

  const insertEmoji = (emoji: string) => {
    if (inputRef.current) {
      const start = inputRef.current.selectionStart || 0;
      const end = inputRef.current.selectionEnd || 0;
      const newText = inputText.substring(0, start) + emoji + inputText.substring(end);
      setInputText(newText);
      
      // Restore cursor position
      setTimeout(() => {
        if (inputRef.current) {
          inputRef.current.selectionStart = inputRef.current.selectionEnd = start + emoji.length;
          inputRef.current.focus();
        }
      }, 0);
    } else {
      setInputText(inputText + emoji);
    }
    setShowEmojiPicker(false);
  };

  const handleDragOver = (e: React.DragEvent) => {
    e.preventDefault();
    if (activeTab === 2) setIsDragging(true); // Only allow in channel chat
  };

  const handleDragLeave = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
  };

  const handleDrop = (e: React.DragEvent) => {
    e.preventDefault();
    setIsDragging(false);
    
    if (activeTab !== 2 || !onUploadFile) {
      return; // Can only upload to channel
    }

    if (e.dataTransfer.files && e.dataTransfer.files.length > 0) {
      const file = e.dataTransfer.files[0];
      handleFileUpload(file);
    }
  };

  const handleFileUpload = (file: File) => {
    if (onUploadFile) {
      onUploadFile(file, currentChannelId);
      // Auto-insert link to the chat
      const fileUrl = `[url=ts3file://${currentChannelId}/${encodeURIComponent(file.name)}]${file.name}[/url]`;
      setInputText(prev => (prev ? prev + ' ' : '') + fileUrl);
    }
  };

  return (
    <div 
      className="chat-pane" 
      style={{ position: 'relative' }}
      onDragOver={handleDragOver}
      onDragEnter={handleDragOver}
      onDragLeave={handleDragLeave}
      onDrop={handleDrop}
    >
      {isDragging && (
        <div style={{
          position: 'absolute', top: 0, left: 0, right: 0, bottom: 0,
          backgroundColor: 'rgba(0,0,0,0.7)', zIndex: 10,
          display: 'flex', alignItems: 'center', justifyContent: 'center',
          color: '#fff', fontSize: '24px', fontWeight: 'bold',
          border: '3px dashed var(--accent-color)'
        }}>
          Drop file to upload to channel
        </div>
      )}

      <div className="chat-tabs">
        <button 
          className={`chat-tab ${activeTab === 3 ? 'active' : ''}`}
          onClick={() => setActiveTab(3)}
        >
          Server
        </button>
        <button 
          className={`chat-tab ${activeTab === 2 ? 'active' : ''}`}
          onClick={() => setActiveTab(2)}
        >
          Channel
        </button>
        <button 
          className={`chat-tab ${activeTab === 1 ? 'active' : ''}`}
          onClick={() => setActiveTab(1)}
          disabled={!currentClientId}
          title={!currentClientId ? "Select a client first" : ""}
        >
          Private
        </button>
      </div>
      
      <div className="chat-messages" style={{ display: 'flex', flexDirection: 'column', gap: '12px', padding: '12px' }}>
        {filteredMessages.map(msg => (
          <div key={msg.id} className="chat-message" style={{ display: 'flex', gap: '12px', alignItems: 'flex-start' }}>
            <Avatar name={msg.senderName} size={40} />
            <div className="message-content" style={{ display: 'flex', flexDirection: 'column', flex: 1 }}>
              <div className="message-header" style={{ display: 'flex', alignItems: 'baseline', gap: '8px', marginBottom: '2px' }}>
                <span className={`sender ${msg.senderId === myClientId ? 'me' : ''}`} style={{ fontWeight: 'bold', color: msg.senderId === myClientId ? 'var(--accent-color)' : '#eee' }}>
                  {msg.senderName}
                </span>
                <span className="timestamp" style={{ fontSize: '11px', color: '#888' }}>
                  {new Date(msg.timestamp).toLocaleTimeString([], {hour: '2-digit', minute:'2-digit'})}
                </span>
              </div>
              <div className="text" style={{ color: '#ccc', lineHeight: '1.4' }}>
                <RichText text={msg.message} />
              </div>
            </div>
          </div>
        ))}
      </div>
      
      <form className="chat-input-area" onSubmit={handleSend} style={{ position: 'relative', display: 'flex', gap: '8px', alignItems: 'center' }}>
        {activeTab === 2 && onUploadFile && (
          <button 
            type="button" 
            className="btn-icon" 
            title="Attach File"
            onClick={() => document.getElementById('chat-file-upload')?.click()}
          >
            📎
          </button>
        )}
        <input 
          type="file" 
          id="chat-file-upload" 
          style={{ display: 'none' }} 
          onChange={(e) => {
            if (e.target.files && e.target.files[0]) {
              handleFileUpload(e.target.files[0]);
              e.target.value = '';
            }
          }} 
        />
        
        <input 
          ref={inputRef}
          type="text" 
          value={inputText}
          onChange={e => setInputText(e.target.value)}
          placeholder={`Message ${activeTab === 3 ? 'Server' : activeTab === 2 ? 'Channel' : 'Client'}...`}
          style={{ flexGrow: 1 }}
        />
        
        <div style={{ position: 'relative' }}>
          <button 
            type="button" 
            className="btn-icon" 
            onClick={() => setShowEmojiPicker(!showEmojiPicker)}
          >
            😀
          </button>
          {showEmojiPicker && (
            <EmojiPicker onSelect={insertEmoji} onClose={() => setShowEmojiPicker(false)} />
          )}
        </div>
        
        <button type="submit">Send</button>
      </form>
    </div>
  );
}
