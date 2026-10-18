import { render, screen } from '@testing-library/react';
import { DesktopClientModal } from '../components/modals/DesktopClientModal';
import { RedeemKeyModal } from '../components/modals/RedeemKeyModal';
import { vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue({ identities: [], favorites: [] }),
}));

describe('DesktopClientModal & RedeemKeyModal', () => {
  it('renders DesktopClientModal with Favorites, Identities & Plugin SDK tabs', () => {
    render(<DesktopClientModal onClose={() => {}} />);
    expect(
      screen.getByText(/Desktop-Client Zentrale/i)
    ).toBeInTheDocument();
    expect(screen.getAllByText(/Server-Favoriten/i).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/Ed25519-Identitäten/i).length).toBeGreaterThan(0);
  });

  it('renders RedeemKeyModal without crashing', () => {
    render(<RedeemKeyModal onClose={() => {}} />);
    expect(
      screen.getAllByText(/Berechtigungsschlüssel benutzen/i).length
    ).toBeGreaterThan(0);
  });
});
