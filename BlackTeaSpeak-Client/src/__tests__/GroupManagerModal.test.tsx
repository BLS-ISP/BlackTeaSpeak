import { render, screen } from '@testing-library/react';
import { GroupsAndTokensModal } from '../components/modals/GroupsAndTokensModal';

describe('GroupsAndTokensModal', () => {
  it('renders without crashing', () => {
    render(<GroupsAndTokensModal onClose={() => {}} />);
    expect(
      screen.getAllByText(/Servergruppen, Channelgruppen & Privilege Keys/i).length
    ).toBeGreaterThan(0);
  });
});
