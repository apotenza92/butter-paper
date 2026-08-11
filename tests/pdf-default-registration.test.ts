import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const installerSource = readFileSync(
  resolve(import.meta.dirname, '../apps/desktop/build/installer.nsh'),
  'utf8',
);
const macRegistrationSource = readFileSync(
  resolve(import.meta.dirname, '../apps/desktop/src/main/applicationRegistration.ts'),
  'utf8',
);

describe('Windows PDF handler registration', () => {
  it('advertises PDF capability without replacing the current default during install', () => {
    expect(installerSource).toContain('Software\\Classes\\.pdf\\OpenWithProgids');
    expect(installerSource).toContain('Software\\RegisteredApplications');
    expect(installerSource).toContain('${BP_CAPABILITIES_KEY}\\FileAssociations');
    expect(installerSource).not.toMatch(/WriteRegStr SHELL_CONTEXT "Software\\Classes\\\.pdf" ""/);
  });

  it('removes only Butter Paper registration during uninstall', () => {
    expect(installerSource).toContain(
      'DeleteRegValue SHELL_CONTEXT "Software\\Classes\\.pdf\\OpenWithProgids" "${BP_PDF_PROG_ID}"',
    );
    expect(installerSource).not.toMatch(/DeleteRegKey SHELL_CONTEXT "Software\\Classes\\\.pdf"/);
  });

  it('defers the shell association refresh to the new installer during updates', () => {
    const uninstall = installerSource.split('!macro customUnInstall', 2)[1];
    expect(uninstall).toContain('${ifNot} ${isUpdated}');
    expect(uninstall).toContain('SHChangeNotify');
  });
});

describe('macOS PDF handler registration', () => {
  it('keeps stable and beta identities isolated while removing stale registrations', () => {
    expect(macRegistrationSource).toContain('com.butterpaper.desktop');
    expect(macRegistrationSource).toContain('com.butterpaper.desktop.beta');
    expect(macRegistrationSource).toContain('lsregister');
    expect(macRegistrationSource).toContain("['-u', registration.path]");
    expect(macRegistrationSource).toContain("['-f', currentApplicationPath]");
    expect(macRegistrationSource).toContain('isInstalledApplicationPath');
    expect(macRegistrationSource).toContain("['/Applications', resolve(homedir(), 'Applications')]");
  });
});
