export function assertIsolatedGuiTestEnvironment(
  suite,
  {
    platform = process.platform,
    githubActions = process.env.GITHUB_ACTIONS,
    allowLocalMacOS = false,
  } = {},
) {
  if (platform !== 'darwin' || githubActions === 'true' || allowLocalMacOS) {
    return;
  }

  throw new Error(
    `${suite} is disabled on local macOS because it opens real application windows. `
    + 'Run it through the manual GitHub Actions workflow instead.',
  );
}
