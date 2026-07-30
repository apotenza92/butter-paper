export function assertIsolatedGuiTestEnvironment(
  suite,
  {
    platform = process.platform,
    githubActions = process.env.GITHUB_ACTIONS,
  } = {},
) {
  if (platform !== 'darwin' || githubActions === 'true') {
    return;
  }

  throw new Error(
    `${suite} is disabled on local macOS because it opens real application windows. `
    + 'Run it through the manual GitHub Actions workflow instead.',
  );
}
