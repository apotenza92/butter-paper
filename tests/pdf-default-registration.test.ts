import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const installerSource = readFileSync(
  resolve(import.meta.dirname, '../apps/desktop/build/installer.nsh'),
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
});
