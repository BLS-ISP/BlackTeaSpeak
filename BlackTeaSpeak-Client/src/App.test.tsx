import { render, screen } from '@testing-library/react';
import App from './App';
import { vi } from 'vitest';

// Mock tauri invoke
vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue({ identities: [], favorites: [] })
}));

describe('App', () => {
  it('renders the sidebar headers', async () => {
    render(<App />);
    expect(screen.getByText('BlackTeaSpeak')).toBeInTheDocument();
    expect(screen.getByText('Next-gen Voice & Chat')).toBeInTheDocument();
  });
});
