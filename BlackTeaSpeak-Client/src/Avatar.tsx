import React from 'react';

interface AvatarProps {
  name: string;
  size?: number;
}

// Generates a consistent, vibrant color based on the username
const getAvatarColor = (name: string) => {
  let hash = 0;
  for (let i = 0; i < name.length; i++) {
    hash = name.charCodeAt(i) + ((hash << 5) - hash);
  }
  
  // Create vibrant HSL colors
  const h = Math.abs(hash % 360);
  const s = 70 + Math.abs((hash >> 3) % 20); // 70-90% saturation
  const l = 45 + Math.abs((hash >> 5) % 15); // 45-60% lightness
  
  return `hsl(${h}, ${s}%, ${l}%)`;
};

// Extracts 1 or 2 initials
const getInitials = (name: string) => {
  if (!name) return '?';
  const parts = name.split(/[-_ ]/);
  if (parts.length > 1 && parts[1].length > 0) {
    return (parts[0][0] + parts[1][0]).toUpperCase();
  }
  return name.substring(0, 2).toUpperCase();
};

export function Avatar({ name, size = 32 }: AvatarProps) {
  const color = getAvatarColor(name);
  const initials = getInitials(name);
  const fontSize = Math.floor(size * 0.45);

  return (
    <div 
      className="avatar" 
      title={name}
      style={{
        width: `${size}px`,
        height: `${size}px`,
        borderRadius: '50%',
        backgroundColor: color,
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'center',
        color: '#ffffff',
        fontWeight: 'bold',
        fontSize: `${fontSize}px`,
        flexShrink: 0,
        boxShadow: '0 2px 4px rgba(0,0,0,0.2)',
        userSelect: 'none'
      }}
    >
      {initials}
    </div>
  );
}
