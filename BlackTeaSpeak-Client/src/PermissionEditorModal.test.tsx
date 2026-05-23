import { render, screen } from '@testing-library/react';
import { PermissionEditorModal } from './PermissionEditorModal';

describe('PermissionEditorModal', () => {
  it('renders without crashing', () => {
    render(<PermissionEditorModal targetType="servergroup" targetId="1" onClose={() => {}} />);
    expect(screen.getByText(/Permission Editor - servergroup \(1\)/i)).toBeInTheDocument();
  });
});
