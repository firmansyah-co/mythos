import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './App';
import './styles/base.css';
import './styles/shell.css';
import './styles/portal.css';
import './styles/runtime.css';

const root = document.getElementById('root');
if (!root) throw new Error('The page has no #root element to mount the workbench into.');

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>
);
