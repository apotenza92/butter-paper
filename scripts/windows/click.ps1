# Clicks the left mouse button at screen pixel X Y through Windows' own input
# queue, for the review driver's os-click lines (BP_REVIEW_OS_CLICK_CMD).
# Usage: click.ps1 X Y
param([int]$X, [int]$Y)
Add-Type -TypeDefinition @"
using System;
using System.Runtime.InteropServices;
public static class ButterPaperReviewMouse {
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
}
"@
# Device pixels, as the driver measures them, not scaled ones.
[ButterPaperReviewMouse]::SetProcessDPIAware() | Out-Null
[ButterPaperReviewMouse]::SetCursorPos($X, $Y) | Out-Null
Start-Sleep -Milliseconds 120
[ButterPaperReviewMouse]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)
Start-Sleep -Milliseconds 80
[ButterPaperReviewMouse]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
