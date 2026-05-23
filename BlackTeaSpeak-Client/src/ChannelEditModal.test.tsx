import { render, screen } from '@testing-library/react';
import { ChannelEditModal } from './ChannelEditModal';

describe('ChannelEditModal', () => {
  it('renders without crashing for creation', () => {
    render(<ChannelEditModal cpid="0" onClose={() => {}} />);
    expect(screen.getByText('Create Channel')).toBeInTheDocument();
  });
});
