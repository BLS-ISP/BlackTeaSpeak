import { render, screen } from '@testing-library/react';
import App from '../App';
import { vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue({ identities: [], favorites: [] }),
}));

describe('App (BlackTeaSpeak Desktop v2.0)', () => {
  it('renders the top toolbar, connection banner, and desktop controls', async () => {
    render(<App />);
    expect(screen.getByText('BlackTeaSpeak')).toBeInTheDocument();
    expect(screen.getByText('Next-gen Voice & Chat')).toBeInTheDocument();
    expect(screen.getByText('v2.0 Desktop')).toBeInTheDocument();
    expect(screen.getByText(/Favoriten & IDs/i)).toBeInTheDocument();
  });
});
