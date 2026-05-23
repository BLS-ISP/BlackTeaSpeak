import { render, screen } from '@testing-library/react';
import { TokenManagerModal } from './TokenManagerModal';

describe('TokenManagerModal', () => {
  it('renders without crashing', () => {
    render(<TokenManagerModal onClose={() => {}} />);
    expect(screen.getByText('Privilege Keys')).toBeInTheDocument();
  });
});
