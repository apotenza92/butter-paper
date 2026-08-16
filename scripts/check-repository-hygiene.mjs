import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync } from 'node:fs';
import { basename, extname } from 'node:path';

const repositoryFiles = execFileSync(
  'git',
  ['ls-files', '--cached', '--others', '--exclude-standard', '-z'],
  { encoding: 'utf8' },
)
  .split('\0')
  .filter((filePath) => filePath && existsSync(filePath));

const forbiddenStatePath = /(^|\/)(plans?|subagents?)(\/|$)/i;
const forbiddenStateName = /^(memory|now|worklog|backlog|handoff|roadmap)([-_.].*)?\.md$/i;
const forbiddenPlanName = /(^|[-_.])(plan|roadmap|handoff|worklog)([-_.]|$).*\.md$/i;
const forbiddenGeneratedPath = /(^|\/)(artifacts|test-results|playwright-report|coverage|dist|\.vite|release|target|icon\.iconset)(\/|$)/;
const trackedBuildMetadata = /\.tsbuildinfo$/;
const textExtensions = new Set(['.cjs', '.css', '.html', '.js', '.json', '.md', '.mjs', '.toml', '.ts', '.tsx', '.yaml', '.yml']);
const obsoleteWorkflowPaths = ['.github/workflows/packages.yml'];
const retiredReleaseSecretName = /\b(?:MACOS_CSC_LINK|CSC_LINK|APPLE_ID|APPLE_APP_SPECIFIC_PASSWORD|APPLE_ID_PASSWORD)\b/;

const violations = [];

function collectFiles(directory) {
  if (!existsSync(directory)) {
    return [];
  }

  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const filePath = `${directory}/${entry.name}`;
    return entry.isDirectory() ? collectFiles(filePath) : [filePath];
  });
}

for (const filePath of repositoryFiles) {
  const fileName = basename(filePath);

  if (forbiddenStatePath.test(filePath) || forbiddenStateName.test(fileName) || forbiddenPlanName.test(fileName)) {
    violations.push(`${filePath}: changing work state belongs in GitHub issues or pull requests`);
  }

  if (forbiddenGeneratedPath.test(filePath) || trackedBuildMetadata.test(filePath)) {
    violations.push(`${filePath}: generated output must remain untracked`);
  }

  if (!textExtensions.has(extname(filePath).toLowerCase())) {
    continue;
  }

  const contents = readFileSync(filePath, 'utf8');
  if (/\/Users\/[^/]+\//.test(contents) || /\/home\/[^/]+\//.test(contents)) {
    violations.push(`${filePath}: contains a machine-specific home-directory path`);
  }
}

for (const filePath of obsoleteWorkflowPaths) {
  if (existsSync(filePath)) {
    violations.push(`${filePath}: obsolete unsigned packaging workflow must not return`);
  }
}

const releaseConfigurationFiles = repositoryFiles.filter((filePath) => (
  filePath.startsWith('.github/workflows/')
  || filePath === 'apps/desktop/electron-builder.config.cjs'
  || filePath === 'apps/desktop/package.json'
  || filePath === 'package.json'
));
for (const filePath of releaseConfigurationFiles) {
  const contents = readFileSync(filePath, 'utf8');
  if (retiredReleaseSecretName.test(contents)) {
    violations.push(`${filePath}: references a retired Apple signing or notarization secret name`);
  }
  if (filePath.startsWith('.github/workflows/')) {
    for (const match of contents.matchAll(/^\s*uses:\s*([^\s#]+).*$/gm)) {
      const action = match[1];
      if (!action.startsWith('./') && !/@[a-f0-9]{40}$/.test(action)) {
        violations.push(`${filePath}: third-party action is not pinned to a full commit SHA: ${action}`);
      }
    }
  }
}

const maintainedCommandFiles = ['package.json', 'apps/desktop/package.json', ...releaseConfigurationFiles];
for (const filePath of new Set(maintainedCommandFiles)) {
  const contents = readFileSync(filePath, 'utf8');
  const referencedPaths = [
    ...contents.matchAll(/(?:^|[\s'"`])(scripts\/[A-Za-z0-9_.\/-]+\.(?:mjs|cjs|js|sh|ps1))/gm),
    ...contents.matchAll(/uses:\s+(\.\/[A-Za-z0-9_.\/-]+\.ya?ml)\s*$/gm),
  ].map((match) => match[1]);
  for (const referencedPath of referencedPaths) {
    if (!existsSync(referencedPath)) {
      violations.push(`${filePath}: references missing maintained path ${referencedPath}`);
    }
  }
}

const desktopComponentsPath = 'apps/desktop/components.json';
if (!existsSync(desktopComponentsPath)) {
  violations.push(`${desktopComponentsPath}: maintained desktop frontend must keep its shadcn configuration`);
} else {
  const desktopComponents = JSON.parse(readFileSync(desktopComponentsPath, 'utf8'));
  if (desktopComponents.style !== 'base-nova') {
    violations.push(`${desktopComponentsPath}: maintained desktop frontend must use the official Base UI Nova style`);
  }
  if (desktopComponents.iconLibrary !== 'lucide') {
    violations.push(`${desktopComponentsPath}: maintained desktop frontend must preserve Lucide icons`);
  }
  if (desktopComponents.rsc !== false || desktopComponents.tsx !== true) {
    violations.push(`${desktopComponentsPath}: maintained desktop frontend must remain non-RSC TypeScript`);
  }
  if (desktopComponents.tailwind?.css !== 'src/renderer/src/styles.css'
    || desktopComponents.tailwind?.baseColor !== 'neutral'
    || desktopComponents.tailwind?.cssVariables !== true) {
    violations.push(`${desktopComponentsPath}: maintained desktop frontend must preserve the Nova Tailwind and neutral-token configuration`);
  }
  if (desktopComponents.aliases?.ui !== '@/components/ui' || desktopComponents.aliases?.utils !== '@/lib/utils') {
    violations.push(`${desktopComponentsPath}: generated components must retain the configured UI and utility import boundaries`);
  }
}

const desktopPackagePath = 'apps/desktop/package.json';
const desktopPackage = JSON.parse(readFileSync(desktopPackagePath, 'utf8'));
if (desktopPackage.dependencies?.['@base-ui/react'] !== '1.6.0') {
  violations.push(`${desktopPackagePath}: maintained shadcn components require exact @base-ui/react 1.6.0`);
}
if (desktopPackage.devDependencies?.shadcn !== '4.16.1') {
  violations.push(`${desktopPackagePath}: maintained shadcn components require exact shadcn CLI 4.16.1`);
}
if (desktopPackage.dependencies?.['@fontsource-variable/geist'] !== '5.3.0') {
  violations.push(`${desktopPackagePath}: the stock Nova preset requires exact @fontsource-variable/geist 5.3.0`);
}
if (desktopPackage.dependencies?.['@fontsource/dm-sans'] || desktopPackage.devDependencies?.['@fontsource/dm-sans']) {
  violations.push(`${desktopPackagePath}: the retired Rhea DM Sans dependency must not remain active`);
}

const rendererStylesPath = 'apps/desktop/src/renderer/src/styles.css';
const rendererStyles = readFileSync(rendererStylesPath, 'utf8');
if (!rendererStyles.includes('@import "@fontsource-variable/geist";')
  || !rendererStyles.includes("--font-sans: 'Geist Variable', sans-serif;")) {
  violations.push(`${rendererStylesPath}: the stock Nova Geist font import and semantic font token must remain configured`);
}
if (/DM Sans|--bp-(?:surface|border-subtle|selected-neutral|text-(?:primary|secondary|muted))|\.bp-(?:surface|border-(?:subtle|left-inset|right-inset|bottom-inset)|text-(?:primary|secondary|muted))\b/.test(rendererStyles)) {
  violations.push(`${rendererStylesPath}: retired Rhea typography and bespoke shell surface, border, selection, or text styling must not return`);
}

const shadcnSkillPath = '.agents/skills/shadcn/SKILL.md';
const skillsLockPath = 'skills-lock.json';
if (!existsSync(shadcnSkillPath) || !existsSync(skillsLockPath)) {
  violations.push('the reviewed official shadcn project skill and skills-lock.json must remain installed');
} else {
  const skillsLock = JSON.parse(readFileSync(skillsLockPath, 'utf8'));
  const shadcnSkill = skillsLock.skills?.shadcn;
  if (shadcnSkill?.source !== 'shadcn/ui' || shadcnSkill?.skillPath !== 'skills/shadcn/SKILL.md') {
    violations.push(`${skillsLockPath}: shadcn skill must remain locked to the official shadcn/ui source`);
  }
}

const activeUiFiles = [
  desktopPackagePath,
  ...collectFiles('apps/desktop/src/renderer/src').filter((filePath) => textExtensions.has(extname(filePath).toLowerCase())),
];
const forbiddenUiCompatibility = [
  { pattern: /@radix-ui\//, reason: 'Radix packages are not part of the maintained Base UI frontend' },
  { pattern: /from\s+['"]radix-ui['"]/, reason: 'the unified Radix package is not part of the maintained Base UI frontend' },
  { pattern: /\basChild\b/, reason: 'use Base UI render composition instead of the Radix asChild API' },
  { pattern: /--radix-/, reason: 'Radix CSS variables are not valid Base UI contracts' },
  { pattern: /\bbp-(?:control|menu|tooltip)(?:-|\b)/, reason: 'obsolete bespoke standard-control styling must not return' },
];

for (const filePath of activeUiFiles) {
  const contents = readFileSync(filePath, 'utf8');
  for (const { pattern, reason } of forbiddenUiCompatibility) {
    if (pattern.test(contents)) {
      violations.push(`${filePath}: ${reason}`);
    }
  }
}

const rendererSourceFiles = collectFiles('apps/desktop/src/renderer/src')
  .filter((filePath) => /\.tsx?$/.test(filePath));
const generatedUiPrefix = 'apps/desktop/src/renderer/src/components/ui/';
const compositionFiles = rendererSourceFiles.filter((filePath) => !filePath.startsWith(generatedUiPrefix));

const domainUiDirectory = 'apps/desktop/src/renderer/src/components/domain-ui';
const approvedDomainUiExceptions = new Map([
  [
    `${domainUiDirectory}/ClosableDocumentTab.tsx`,
    {
      reason: 'shadcn Tabs has no closable document-tab pattern, so an unstyled official TabsTrigger is paired with only a compact close affordance; dirty-state and sortable behaviour remain non-visual document behaviour',
      allowVisualStyling: true,
      allowDynamicDragStyle: true,
      rawElements: new Set(['button']),
    },
  ],
  [
    `${domainUiDirectory}/SplitButtonSegment.tsx`,
    {
      reason: 'shadcn ButtonGroup does not synchronize',
      allowVisualStyling: true,
      allowDynamicDragStyle: false,
      rawElements: new Set(),
    },
  ],
  [
    `${domainUiDirectory}/LastTemplatePreviewTooltip.tsx`,
    {
      reason: 'the rich last-template preview needs a popover surface whose compound tooltip arrow matches that surface, which the official single-style TooltipContent cannot provide',
      allowVisualStyling: true,
      allowDynamicDragStyle: false,
      rawElements: new Set(),
    },
  ],
  [
    `${domainUiDirectory}/PropertyControls.tsx`,
    {
      reason: 'Bluebeam-compatible property editors require reusable visual previews, mixed-value presentation, unit-aware numeric composition, and conditional multi-control groups that official single controls do not provide',
      allowVisualStyling: true,
      allowDynamicDragStyle: true,
      rawElements: new Set(),
    },
  ],
  [
    `${domainUiDirectory}/BlankPdfPagePreview.tsx`,
    {
      reason: 'the blank PDF settings popover needs an exact aspect-ratio and vector paper-pattern preview that no official shadcn control provides',
      allowVisualStyling: true,
      allowDynamicDragStyle: true,
      rawElements: new Set(['svg']),
    },
  ],
  [
    `${domainUiDirectory}/ConstructionGridOverlay.tsx`,
    {
      reason: 'the optional construction grid is page-coordinate vector canvas chrome rather than a standard interactive control',
      allowVisualStyling: true,
      allowDynamicDragStyle: false,
      rawElements: new Set(['svg']),
    },
  ],
]);
const domainUiFiles = collectFiles(domainUiDirectory).filter((filePath) => filePath.endsWith('.tsx'));

for (const filePath of domainUiFiles) {
  if (!approvedDomainUiExceptions.has(filePath)) {
    violations.push(`${filePath}: domain UI exception is not explicitly reviewed and allowlisted`);
  }
}
for (const [filePath, exception] of approvedDomainUiExceptions) {
  const { reason } = exception;
  if (!existsSync(filePath)) {
    violations.push(`${filePath}: approved domain UI exception is missing (${reason})`);
    continue;
  }
  const contents = readFileSync(filePath, 'utf8');
  if (!contents.includes('data-domain-ui-exception=')) {
    violations.push(`${filePath}: approved domain UI exception must identify itself in rendered markup (${reason})`);
  }
  if (!exception.allowVisualStyling && /\b(?:bg-|border(?:-|\b)|rounded(?:-|\b)|shadow(?:-|\b)|ring(?:-|\b)|text-)\S*/.test(contents)) {
    violations.push(`${filePath}: domain UI exceptions may customize structure and behavior, not generated Nova visuals (${reason})`);
  }
  if (!exception.allowDynamicDragStyle && /\bstyle=/.test(contents)) {
    violations.push(`${filePath}: inline styles require an explicit dynamic domain-behavior allowance (${reason})`);
  }
}

for (const filePath of compositionFiles) {
  const contents = readFileSync(filePath, 'utf8');
  if (/from\s+['"]@base-ui\/react(?:\/[^'"]*)?['"]/.test(contents)) {
    violations.push(`${filePath}: Base UI primitives must be consumed through generated components/ui boundaries`);
  }
  if (!filePath.startsWith(`${domainUiDirectory}/`)
    && /<TooltipContent\b[^>]*\bclassName="[^"]*(?:\bbg-|\btext-)/.test(contents)) {
    violations.push(`${filePath}: stock tooltip colours must not be partially overridden outside a reviewed domain UI exception`);
  }
  if (/data-\[state=/.test(contents)) {
    violations.push(`${filePath}: composition code must not depend on Radix-style data-state selectors`);
  }
}

// These are the reviewed domain-control exceptions. Standard controls belong in
// components/ui; PDF rendering, annotations, custom view icons, and their backing
// canvas elements remain application-specific UI.
const approvedRawElements = new Map([
  ['apps/desktop/src/renderer/src/app.tsx', new Set(['input'])],
  ['apps/desktop/src/renderer/src/components/AnnotationLayer.tsx', new Set(['svg', 'textarea'])],
  ['apps/desktop/src/renderer/src/components/DocumentViewport.tsx', new Set(['canvas'])],
  ['apps/desktop/src/renderer/src/components/PageThumbnailItem.tsx', new Set(['canvas'])],
  ['apps/desktop/src/renderer/src/components/PageView.tsx', new Set(['canvas'])],
  // Signature Pad requires a canvas because pointer strokes become a transparent
  // image appearance. It is PDF markup behavior, not a standard form control.
  ['apps/desktop/src/renderer/src/components/SignatureMenu.tsx', new Set(['canvas'])],
  ['apps/desktop/src/renderer/src/components/ViewerToolbar.tsx', new Set(['svg'])],
]);

for (const filePath of compositionFiles.filter((candidate) => candidate.endsWith('.tsx'))) {
  const contents = readFileSync(filePath, 'utf8');
  for (const match of contents.matchAll(/<(button|input|select|textarea|canvas|svg)\b/g)) {
    const element = match[1];
    const approvedByDomainException = approvedDomainUiExceptions.get(filePath)?.rawElements?.has(element);
    if (!approvedRawElements.get(filePath)?.has(element) && !approvedByDomainException) {
      violations.push(`${filePath}: raw <${element}> is outside the reviewed domain-control allowlist; use components/ui`);
    }
  }
}

const viewerToolbarPath = 'apps/desktop/src/renderer/src/components/ViewerToolbar.tsx';
if (existsSync(viewerToolbarPath)) {
  const viewerToolbar = readFileSync(viewerToolbarPath, 'utf8');
  for (const gestureHint of [
    'Double click to view Continuous',
    'Double click to view Single Page',
    'Double click to Fit Width',
    'Double click to Fit Page',
  ]) {
    if (!viewerToolbar.includes(gestureHint)) {
      violations.push(`${viewerToolbarPath}: missing reviewed view-control shortcut hint "${gestureHint}"`);
    }
  }
  if (!viewerToolbar.includes('onDoubleClick')) {
    violations.push(`${viewerToolbarPath}: paired Fit and Page View controls must retain their reviewed double-click shortcuts`);
  }
  if (!viewerToolbar.includes('<SplitButtonSegment')
    || !viewerToolbar.includes('<DropdownMenuGroup>\n            <DropdownMenuLabel>Mousewheel Behaviour</DropdownMenuLabel>')) {
    violations.push(`${viewerToolbarPath}: view split controls must share one state-driven surface and keep menu labels inside a Base UI group`);
  }
  if (/rounded-(?:full|\[[^\]]+\]|[A-Za-z0-9]+)/.test(viewerToolbar)) {
    violations.push(`${viewerToolbarPath}: toolbar compositions must inherit official Nova control radii`);
  }
}

const documentTabBarPath = 'apps/desktop/src/renderer/src/components/DocumentTabBar.tsx';
const closableDocumentTabPath = 'apps/desktop/src/renderer/src/components/domain-ui/ClosableDocumentTab.tsx';
if (existsSync(documentTabBarPath)) {
  const documentTabBar = readFileSync(documentTabBarPath, 'utf8');
  if (documentTabBar.includes('variant="line"')) {
    violations.push(`${documentTabBarPath}: document tabs must use the default Nova TabsList variant`);
  }
  if (!documentTabBar.includes('className="shrink-0 justify-start gap-2 rounded-none bg-background! p-0! group-data-horizontal/tabs:h-8!"')) {
    violations.push(`${documentTabBarPath}: the default Nova TabsList must match standard application button height without nested padding`);
  }
  if (!documentTabBar.includes('data-testid="document-tab-surface"')) {
    violations.push(`${documentTabBarPath}: document tabs and actions must share one full-width Nova surface`);
  }
  if (!documentTabBar.includes('<SplitButtonSegment') || !documentTabBar.includes('size="icon"')) {
    violations.push(`${documentTabBarPath}: document actions must use the same stock Nova button size as toolbar controls`);
  }
  if (!documentTabBar.includes('<Plus data-icon="inline-start" aria-hidden="true" />')
    || !documentTabBar.includes('aria-label="Open PDF"')) {
    violations.push(`${documentTabBarPath}: the compact open action must use the adopted plus icon with an explicit accessible label`);
  }
  if (!documentTabBar.includes('className="flex items-center border-b border-border bg-background p-2"')) {
    violations.push(`${documentTabBarPath}: document tabs must keep one consistent inset and a full-width shell divider`);
  }
  if (!documentTabBar.includes('className="bp-native-scroll-hidden flex min-w-0 items-center gap-2 overflow-x-auto"')
    || !documentTabBar.includes('className="flex h-8 shrink-0 items-center gap-2 bg-background"')) {
    violations.push(`${documentTabBarPath}: document tabs and adjacent actions must match the viewer toolbar group spacing`);
  }
  if (!documentTabBar.includes('<Separator')
    || !documentTabBar.includes('orientation="vertical"')
    || !documentTabBar.includes('data-testid="document-tab-actions-separator"')) {
    violations.push(`${documentTabBarPath}: a standard vertical separator must divide the newest tab from document actions`);
  }
  if (!documentTabBar.includes('<ButtonGroup aria-label="New from template controls">')
    || documentTabBar.includes('<ButtonGroupSeparator />')
    || !documentTabBar.includes('<TemplatePickerPopover')) {
    violations.push(`${documentTabBarPath}: template creation must match the stock Nova split controls used by the viewer toolbar`);
  }
}
if (existsSync(closableDocumentTabPath)) {
  const closableDocumentTab = readFileSync(closableDocumentTabPath, 'utf8');
  if (/\bdata-active=|\bjustify-start\b|\bmin-w-24\b|\bmax-w-\[|\bflex-none\b|\btouch-none\b|\bcursor-default\b/.test(closableDocumentTab)) {
    violations.push(`${closableDocumentTabPath}: document TabsTrigger customisation must stay limited to the reviewed semantic background treatment`);
  }
  if (!closableDocumentTab.includes('className="h-8! bg-background! data-active:bg-muted! group-data-[dragging]/document-tab:after:opacity-0!"')) {
    violations.push(`${closableDocumentTabPath}: document tabs must match toolbar button height and use the reviewed semantic background treatment`);
  }
  if (!closableDocumentTab.includes('className="inline-flex h-full shrink-0 items-center text-muted-foreground leading-none"')) {
    violations.push(`${closableDocumentTabPath}: the dirty marker must remain vertically centred within the document tab`);
  }
}
if (existsSync(rendererStylesPath)) {
  const rendererStyles = readFileSync(rendererStylesPath, 'utf8');
  if (/\[data-domain-ui-exception="closable-document-tab"\][^{]*\[data-slot="tabs-trigger"\]/.test(rendererStyles)) {
    violations.push(`${rendererStylesPath}: document tabs must not override stock Nova TabsTrigger states`);
  }
}

if (violations.length > 0) {
  console.error('Repository hygiene check failed:');
  for (const violation of violations) {
    console.error(`- ${violation}`);
  }
  process.exit(1);
}

console.log(`Repository hygiene check passed (${repositoryFiles.length} tracked and untracked repository files inspected).`);
