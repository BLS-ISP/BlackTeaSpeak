import { render, screen } from '@testing-library/react';
import { PermissionsModal } from '../components/modals/PermissionsModal';

describe('PermissionsModal', () => {
  it('renders without crashing', () => {
    render(<PermissionsModal onClose={() => {}} />);
    expect(
      screen.getByText(/5-Tier Permission-Editor/i)
    ).toBeInTheDocument();
  });
});
