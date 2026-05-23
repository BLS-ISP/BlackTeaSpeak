import { render, screen } from '@testing-library/react';
import { BanManagerModal } from './BanManagerModal';

describe('BanManagerModal', () => {
  it('renders without crashing', () => {
    render(<BanManagerModal onClose={() => {}} />);
    expect(screen.getByText('Ban Manager')).toBeInTheDocument();
  });
});
