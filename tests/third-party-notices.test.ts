import { readFileSync } from 'node:fs';

describe('desktop third-party notices', () => {
  it('ships the QRCode, dijkstrajs, Signature Pad, and Allura license notices as a desktop resource', () => {
    const notices = readFileSync('apps/desktop/THIRD_PARTY_NOTICES.md', 'utf8');
    const builderConfig = readFileSync('apps/desktop/electron-builder.config.cjs', 'utf8');

    expect(notices).toContain('Copyright (c) 2012 Ryan Day');
    expect(notices).toContain('Copyright (C) 2008 Wyatt Baldwin');
    expect(notices).toContain('Copyright (c) 2018 Szymon Nowak');
    expect(notices).toContain('Copyright 2010 The Allura Project Authors');
    expect(notices).toContain('SIL OPEN FONT LICENSE Version 1.1');
    expect(builderConfig).toContain("from: 'THIRD_PARTY_NOTICES.md'");
    expect(builderConfig).toContain("to: 'THIRD_PARTY_NOTICES.md'");
  });
});
