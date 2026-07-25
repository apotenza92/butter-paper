import '@fontsource/dm-sans/400.css';
import '@fontsource/dm-sans/500.css';
import './styles.css';

import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app';
import { TOOLTIP_SHOW_DELAY_MS } from './components/Tooltip';
import { TooltipProvider } from './components/ui/tooltip';
import { bootstrapThemeMode } from './theme';

async function bootstrap() {
  const initialThemeMode = await bootstrapThemeMode();
  const root = createRoot(document.getElementById('root') as HTMLElement);

  root.render(
    <StrictMode>
      <TooltipProvider delay={TOOLTIP_SHOW_DELAY_MS}>
        <App initialThemeMode={initialThemeMode} />
      </TooltipProvider>
    </StrictMode>,
  );
}

void bootstrap();
