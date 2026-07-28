import { quoteDesktopExecArgument } from './defaultPdfApp';

describe('default PDF app integration', () => {
  it('quotes Linux desktop entry executable arguments safely', () => {
    expect(quoteDesktopExecArgument('/opt/Butter Paper/app`$"')).toBe(
      '"/opt/Butter Paper/app\\`\\$\\""',
    );
  });
});
