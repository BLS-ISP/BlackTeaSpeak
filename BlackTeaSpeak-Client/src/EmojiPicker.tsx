import React, { useState } from 'react';

const EMOJI_LIST = [
  "😀", "😃", "😄", "😁", "😆", "😅", "😂", "🤣", "😊", "😇",
  "🙂", "🙃", "😉", "😌", "😍", "🥰", "😘", "😗", "😙", "😚",
  "😋", "😛", "😝", "😜", "🤪", "🤨", "🧐", "🤓", "😎", "🤩",
  "🥳", "😏", "😒", "😞", "😔", "😟", "😕", "🙁", "☹️", "😣",
  "😖", "😫", "😩", "🥺", "😢", "😭", "😤", "😠", "😡", "🤬",
  "🤯", "😳", "🥵", "🥶", "😱", "😨", "😰", "😥", "😓", "🤗",
  "🤔", "🤭", "🤫", "🤥", "😶", "😐", "😑", "😬", "🙄", "😯",
  "😦", "😧", "😮", "😲", "🥱", "😴", "🤤", "😪", "😵", "🤐",
  "🥴", "🤢", "🤮", "🤧", "😷", "🤒", "🤕", "🤑", "🤠", "😈",
  "👿", "👹", "👺", "🤡", "💩", "👻", "💀", "☠️", "👽", "👾",
  "🤖", "🎃", "😺", "😸", "😹", "😻", "😼", "😽", "🙀", "😿",
  "😾", "👍", "👎", "👏", "🙌", "👐", "🤲", "🤝", "🙏", "✌️",
  "🤞", "🤟", "🤘", "🤙", "👈", "👉", "👆", "🖕", "👇", "☝️",
  "👋", "🤚", "🖐", "🖖", "💪", "🦵", "🦶", "👂", "👃", "🧠",
  "🦷", "🦴", "👀", "👁", "👅", "👄", "💋", "🩸", "❤️", "🧡"
];

interface EmojiPickerProps {
  onSelect: (emoji: string) => void;
  onClose: () => void;
}

export function EmojiPicker({ onSelect, onClose }: EmojiPickerProps) {
  return (
    <div className="emoji-picker" style={{
      position: 'absolute',
      bottom: '100%',
      right: 0,
      marginBottom: '8px',
      backgroundColor: '#1e1e1e',
      border: '1px solid #333',
      borderRadius: '8px',
      padding: '8px',
      width: '280px',
      height: '220px',
      overflowY: 'auto',
      display: 'grid',
      gridTemplateColumns: 'repeat(8, 1fr)',
      gap: '4px',
      boxShadow: '0 4px 12px rgba(0,0,0,0.5)',
      zIndex: 1000,
    }}>
      <div style={{
        position: 'fixed',
        top: 0, left: 0, right: 0, bottom: 0,
        zIndex: -1
      }} onClick={onClose} />
      
      {EMOJI_LIST.map((emoji, idx) => (
        <button
          key={idx}
          className="emoji-btn"
          onClick={() => onSelect(emoji)}
          style={{
            background: 'none',
            border: 'none',
            fontSize: '20px',
            cursor: 'pointer',
            padding: '4px',
            borderRadius: '4px',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            transition: 'background 0.2s',
          }}
          onMouseOver={(e) => e.currentTarget.style.backgroundColor = '#333'}
          onMouseOut={(e) => e.currentTarget.style.backgroundColor = 'transparent'}
        >
          {emoji}
        </button>
      ))}
    </div>
  );
}
