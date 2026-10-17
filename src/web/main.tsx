import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';

import { openLink } from './api.ts';
import { App } from './App.tsx';
import './styles.css';

// The app window can't open new browser tabs, so every link to an outside page opens in the default browser.
document.addEventListener('click', (event) => {
  const link = event.target instanceof Element ? event.target.closest('a[href^="https://"]') : null;
  if (!(link instanceof HTMLAnchorElement)) return;
  event.preventDefault();
  openLink(link.href).catch((err: unknown) => console.error(err));
});

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>
);
