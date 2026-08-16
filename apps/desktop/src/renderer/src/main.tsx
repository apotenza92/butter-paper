import './styles.css';

import { StrictMode, useEffect, useLayoutEffect } from 'react';
import { createRoot } from 'react-dom/client';
import { App } from './app';
import { TOOLTIP_SHOW_DELAY_MS } from './components/Tooltip';
import { TooltipProvider } from './components/ui/tooltip';
import { bootstrapThemeMode } from './theme';

markTestStartup('renderer-module-evaluated');

async function bootstrap() {
  markTestStartup('theme-requested');
  const initialThemeMode = await bootstrapThemeMode();
  markTestStartup('theme-ready');
  const root = createRoot(document.getElementById('root') as HTMLElement);

  markTestStartup('react-render-requested');
  root.render(
    <StrictMode>
      <TooltipProvider delay={TOOLTIP_SHOW_DELAY_MS}>
        {window.butterPaper.environment.testMode ? <StartupCommitMarker /> : null}
        <App initialThemeMode={initialThemeMode} />
      </TooltipProvider>
    </StrictMode>,
  );
}

function StartupCommitMarker() {
  useLayoutEffect(() => {
    markTestStartup('react-committed');
  }, []);

  useEffect(() => {
    const frame = requestAnimationFrame(() => {
      markTestStartup('first-animation-frame');
    });
    return () => cancelAnimationFrame(frame);
  }, []);

  return null;
}

function markTestStartup(name: string): void {
  if (!window.butterPaper?.environment.testMode
    || performance.getEntriesByName(`bp-startup:${name}`, 'mark').length > 0) {
    return;
  }
  performance.mark(`bp-startup:${name}`);
}

void bootstrap();
