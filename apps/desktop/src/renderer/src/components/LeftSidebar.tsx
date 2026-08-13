import type { PageModel, PageRotationDirection } from '@butter-paper/core';
import type { LocalPdfSession } from '../services/documentSession';
import type { LeftSidebarPanel } from '../state/viewerStore';
import { PageThumbnailList } from './PageThumbnailList';
import {
  PRIMARY_BAND_HEIGHT,
  SHELL_BAND_BORDER_BOTTOM,
  SHELL_HEADER_INSET_X,
  SHELL_PANEL_BORDER_RIGHT,
  SHELL_SURFACE_PANEL,
  SHELL_TEXT_MUTED,
  SHELL_TEXT_PRIMARY,
} from './shellSpacing';

interface LeftSidebarProps {
  session: LocalPdfSession | null;
  pages: readonly PageModel[];
  panel: LeftSidebarPanel;
  mutationDisabled?: boolean;
  onSelectPage: (pageIndex: number, source?: 'thumbnail', previewUrl?: string | null) => void;
  onSetPageScale: (pageIndex: number) => void;
  onRotatePage: (pageIndex: number, direction: PageRotationDirection) => void;
}

export function LeftSidebar({ session, pages, mutationDisabled = false, onSelectPage, onSetPageScale, onRotatePage }: LeftSidebarProps) {
  const title = 'Page Thumbnails';
  return (
    <aside
      className={['relative flex h-full flex-none flex-col', SHELL_SURFACE_PANEL, SHELL_PANEL_BORDER_RIGHT].join(' ')}
      data-testid="left-sidebar"
      id="left-sidebar-panel"
      aria-label={title}
      style={{ width: '300px' }}
    >
      <div
        className={[
          'flex items-center justify-center text-center text-[12px] font-semibold',
          PRIMARY_BAND_HEIGHT,
          SHELL_HEADER_INSET_X,
          SHELL_BAND_BORDER_BOTTOM,
          SHELL_TEXT_PRIMARY,
        ].join(' ')}
        data-testid="left-sidebar-header"
      >
        {title}
      </div>
      {session && pages.length > 0 ? (
        <PageThumbnailList
          session={session}
          pages={pages}
          mutationDisabled={mutationDisabled}
          onSelectPage={onSelectPage}
          onSetPageScale={onSetPageScale}
          onRotatePage={onRotatePage}
        />
      ) : (
        <div className={['flex flex-1 items-center justify-center px-4 text-center text-[12px]', SHELL_TEXT_MUTED].join(' ')}>
          Open a PDF to browse page thumbnails.
        </div>
      )}
    </aside>
  );
}
