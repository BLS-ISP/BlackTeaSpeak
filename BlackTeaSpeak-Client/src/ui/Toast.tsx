import { createRoot } from 'react-dom/client';
import { useState } from 'react';
import '../App.scss';

let toastContainer: HTMLDivElement | null = null;

interface ToastMsg {
  id: string;
  message: string;
  type: 'success' | 'error' | 'info';
}

let toasts: ToastMsg[] = [];
let notifyReact: (() => void) | null = null;

const ToastContainer = () => {
  const [, setTick] = useState(0);
  notifyReact = () => setTick(t => t + 1);

  return (
    <div className="toast-container">
      {toasts.map(t => (
        <div key={t.id} className={`toast toast-${t.type}`}>
          {t.message}
        </div>
      ))}
    </div>
  );
};

export const Toast = {
  show: (message: string, type: 'success' | 'error' | 'info' = 'info') => {
    if (!toastContainer) {
      toastContainer = document.createElement('div');
      document.body.appendChild(toastContainer);
      const root = createRoot(toastContainer);
      root.render(<ToastContainer />);
    }

    const id = Math.random().toString();
    toasts.push({ id, message, type });
    if (notifyReact) notifyReact();

    setTimeout(() => {
      toasts = toasts.filter(t => t.id !== id);
      if (notifyReact) notifyReact();
    }, 3000);
  },
  error: (msg: string) => Toast.show(msg, 'error'),
  success: (msg: string) => Toast.show(msg, 'success'),
  info: (msg: string) => Toast.show(msg, 'info'),
};
