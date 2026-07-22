import '@fontsource/dm-sans/400.css';
import '@fontsource/dm-sans/500.css';
import './styles.css';

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app';
import { bootstrapThemeMode } from './theme';

async function bootstrap() {
  const initialThemeMode = await bootstrapThemeMode();
  const root = createRoot(document.getElementById('root') as HTMLElement);

  root.render(
    <StrictMode>
      <App initialThemeMode={initialThemeMode} />
    </StrictMode>,
  );
}

void bootstrap();
