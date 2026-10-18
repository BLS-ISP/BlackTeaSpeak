import { render, screen } from '@testing-library/react';
import { ModerationModal } from '../components/modals/ModerationModal';

describe('ModerationModal (Ban & Complaint Manager)', () => {
  it('renders without crashing', () => {
    render(<ModerationModal onClose={() => {}} />);
    expect(
      screen.getByText(/Moderation: Bannliste, Beschwerden & Offline-Nachrichten/i)
    ).toBeInTheDocument();
    expect(screen.getAllByText(/Bannliste/i).length).toBeGreaterThan(0);
  });
});
