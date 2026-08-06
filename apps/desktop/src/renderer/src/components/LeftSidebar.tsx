import type { PageModel, PageRotationDirection } from '@butter-paper/core';
import type { LocalPdfSession } from '../services/documentSession';
import type { LeftSidebarPanel } from '../state/viewerStore';
import {
  DEFAULT_LEFT_SIDEBAR_WIDTH,
  MAX_LEFT_SIDEBAR_WIDTH,
  MIN_LEFT_SIDEBAR_WIDTH,
} from '../state/viewerStore';
import { PageThumbnailList } from './PageThumbnailList';
import { SidebarResizeHandle } from './SidebarResizeHandle';
import {
  PRIMARY_BAND_HEIGHT,
  SHELL_BORDER_SUBTLE,
  SHELL_HEADER_INSET_X,
  SHELL_SURFACE_PANEL,
  SHELL_TEXT_MUTED,
  SHELL_TEXT_PRIMARY,
} from './shellSpacing';

interface LeftSidebarProps {
  session: LocalPdfSession | null;
  pages: readonly PageModel[];
  panel: LeftSidebarPanel;
  width: number;
  mutationDisabled?: boolean;
  onSelectPage: (pageIndex: number, source?: 'thumbnail', previewUrl?: string | null) => void;
  onSetPageScale: (pageIndex: number) => void;
  onRotatePage: (pageIndex: number, direction: PageRotationDirection) => void;
  onWidthChange: (width: number) => void;
}

export function LeftSidebar({ session, pages, width, mutationDisabled = false, onSelectPage, onSetPageScale, onRotatePage, onWidthChange }: LeftSidebarProps) {
  const title = 'Page Thumbnails';
  return (
    <aside
      className={['relative flex h-full flex-none flex-col border-r', SHELL_SURFACE_PANEL, SHELL_BORDER_SUBTLE].join(' ')}
      data-testid="left-sidebar"
      id="left-sidebar-panel"
      aria-label={title}
      style={{ width: `${width}px` }}
    >
      <div
        className={[
          'flex items-center justify-center border-b text-center text-[12px] font-semibold',
          PRIMARY_BAND_HEIGHT,
          SHELL_HEADER_INSET_X,
          SHELL_BORDER_SUBTLE,
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
      <SidebarResizeHandle
        side="left"
        width={width}
        minWidth={MIN_LEFT_SIDEBAR_WIDTH}
        maxWidth={MAX_LEFT_SIDEBAR_WIDTH}
        defaultWidth={DEFAULT_LEFT_SIDEBAR_WIDTH}
        label={`${title} sidebar`}
        testId="left-sidebar-resize-handle"
        onWidthChange={onWidthChange}
      />
    </aside>
  );
}
