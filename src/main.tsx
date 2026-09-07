import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app/App';
import './ui/styles.css';
import './ui/design-system/tokens.css';
import './ui/design-system/components.css';

const root = document.getElementById('root')!;
root.classList.add('colui-design');
createRoot(root).render(<StrictMode><App /></StrictMode>);
