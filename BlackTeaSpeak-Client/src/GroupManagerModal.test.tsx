import { render, screen } from '@testing-library/react';
import { GroupManagerModal } from './GroupManagerModal';

describe('GroupManagerModal', () => {
  it('renders without crashing', () => {
    render(<GroupManagerModal onClose={() => {}} />);
    expect(screen.getByText('Group Manager')).toBeInTheDocument();
    expect(screen.getByText('Server Groups')).toBeInTheDocument();
  });
});
