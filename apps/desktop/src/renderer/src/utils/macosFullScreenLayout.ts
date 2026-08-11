export interface MacosFullScreenLayoutState {
  platform: string;
  fullScreen: boolean;
  menuBarVisible: boolean;
}

export function resolveMacosFullScreenLayout(state: MacosFullScreenLayoutState) {
  const macosFullScreen = state.platform.toLowerCase().includes('mac') && state.fullScreen;
  return {
    showWindowTitleBar: !macosFullScreen,
    showAppMenuBar: state.menuBarVisible,
  };
}
