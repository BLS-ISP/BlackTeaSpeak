import { createRoot } from 'react-dom/client';
import React from 'react';
import '../App.scss';

export const Dialogs = {
  confirm: (title: string, message: string): Promise<boolean> => {
    return new Promise((resolve) => {
      const container = document.createElement('div');
      document.body.appendChild(container);
      const root = createRoot(container);

      const close = (result: boolean) => {
        root.unmount();
        container.remove();
        resolve(result);
      };

      root.render(
        <div className="modal-overlay">
          <div className="modal-content prompt-modal slide-in">
            <h2>{title}</h2>
            <p>{message}</p>
            <div className="form-actions">
              <button className="btn-secondary" onClick={() => close(false)}>Cancel</button>
              <button className="btn-primary" onClick={() => close(true)}>Confirm</button>
            </div>
          </div>
        </div>
      );
    });
  },

  prompt: (title: string, message: string, defaultValue = ''): Promise<string | null> => {
    return new Promise((resolve) => {
      const container = document.createElement('div');
      document.body.appendChild(container);
      const root = createRoot(container);

      const close = (result: string | null) => {
        root.unmount();
        container.remove();
        resolve(result);
      };

      const PromptComponent = () => {
        const [val, setVal] = React.useState(defaultValue);
        return (
          <div className="modal-overlay">
            <div className="modal-content prompt-modal slide-in">
              <h2>{title}</h2>
              <p>{message}</p>
              <input 
                autoFocus
                className="modal-input"
                value={val} 
                onChange={e => setVal(e.target.value)} 
                onKeyDown={e => {
                  if (e.key === 'Enter') close(val);
                  if (e.key === 'Escape') close(null);
                }}
              />
              <div className="form-actions">
                <button className="btn-secondary" onClick={() => close(null)}>Cancel</button>
                <button className="btn-primary" onClick={() => close(val)}>OK</button>
              </div>
            </div>
          </div>
        );
      };

      root.render(<PromptComponent />);
    });
  }
};
