import React, { useState } from 'react';
import { ChatMessage } from './types';

interface ChatPaneProps {
  messages: ChatMessage[];
  onSendMessage: (targetMode: number, target: string, message: string) => void;
  myClientId: string;
  currentChannelId: string;
}

export function ChatPane({ messages, onSendMessage, myClientId, currentChannelId }: ChatPaneProps) {
  const [activeTab, setActiveTab] = useState<number>(3); // 3 = Server, 2 = Channel
  const [inputText, setInputText] = useState('');

  const filteredMessages = messages.filter(m => m.targetMode === activeTab);

  const handleSend = (e: React.FormEvent) => {
    e.preventDefault();
    if (!inputText.trim()) return;

    let target = '0';
    if (activeTab === 2) target = currentChannelId;
    if (activeTab === 1) target = '0'; // Would be specific client id

    onSendMessage(activeTab, target, inputText);
    setInputText('');
  };

  return (
    <div className="chat-pane">
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
      </div>
      
      <div className="chat-messages">
        {filteredMessages.map(msg => (
          <div key={msg.id} className="chat-message">
            <span className="timestamp">
              {new Date(msg.timestamp).toLocaleTimeString([], {hour: '2-digit', minute:'2-digit'})}
            </span>
            <span className={`sender ${msg.senderId === myClientId ? 'me' : ''}`}>
              {msg.senderName}:
            </span>
            <span className="text">{msg.message}</span>
          </div>
        ))}
      </div>
      
      <form className="chat-input-area" onSubmit={handleSend}>
        <input 
          type="text" 
          value={inputText}
          onChange={e => setInputText(e.target.value)}
          placeholder={`Message ${activeTab === 3 ? 'Server' : 'Channel'}...`}
        />
        <button type="submit">Send</button>
      </form>
    </div>
  );
}
