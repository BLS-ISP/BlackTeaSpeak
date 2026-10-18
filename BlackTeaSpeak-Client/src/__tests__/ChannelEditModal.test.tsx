import { render, screen } from '@testing-library/react';
import { ChannelDialogModal } from '../components/modals/ChannelDialogModal';

describe('ChannelDialogModal', () => {
  it('renders without crashing for creation', () => {
    render(<ChannelDialogModal mode="create" onClose={() => {}} />);
    expect(screen.getByText('Neuen Channel erstellen')).toBeInTheDocument();
  });
});
