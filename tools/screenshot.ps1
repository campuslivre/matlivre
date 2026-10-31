# Captures a MatLivre window for the README: opens a file in the release build, optionally clicks
# toolbar buttons, runs it (F5) and scrolls, and saves the window as a PNG.
#   powershell -NoProfile -ExecutionPolicy Bypass -File tools\screenshot.ps1 -File examples\welcome.mlx -Out docs\welcome.png -Run
# -Click takes window coordinates ("x,y" items); -Scroll wheel notches over the document.
param(
    [Parameter(Mandatory)] [string]$File,
    [Parameter(Mandatory)] [string]$Out,
    [switch]$Run,
    [string[]]$Click = @(),
    [int]$Scroll = 0,
    [int]$Width = 1500,
    [int]$Height = 950,
    [double]$Settle = 6
)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing, System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public static class Win {
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
    [DllImport("user32.dll")] public static extern bool MoveWindow(IntPtr h, int x, int y, int w, int hh, bool repaint);
    [DllImport("dwmapi.dll")] public static extern int DwmGetWindowAttribute(IntPtr h, int attr, out RECT r, int size);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
    [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr after, int x, int y, int w, int hh, uint flags);
    [DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint msg, IntPtr w, IntPtr l);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
    public delegate bool EnumProc(IntPtr h, IntPtr l);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc f, IntPtr l);
    /// The largest visible top-level window of process `pid` (the app also owns small helper windows).
    public static IntPtr MainOf(uint pid) {
        IntPtr best = IntPtr.Zero; long area = 0;
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            RECT r;
            if (p == pid && IsWindowVisible(h) && GetWindowRect(h, out r)) {
                long a = (long)(r.R - r.L) * (r.B - r.T);
                if (a > area) { area = a; best = h; }
            }
            return true;
        }, IntPtr.Zero);
        return best;
    }
}
"@
[Win]::SetProcessDPIAware() | Out-Null
$root = Split-Path $PSScriptRoot
$exe = Join-Path $root 'target\release\matlivre.exe'
$path = (Resolve-Path $File).Path
$p = Start-Process $exe -ArgumentList "`"$path`"" -PassThru
try {
    $sw = [Diagnostics.Stopwatch]::StartNew()
    do {
        if ($sw.Elapsed.TotalSeconds -gt 30) { throw 'no window' }
        Start-Sleep -Milliseconds 200
        $h = [Win]::MainOf([uint32]$p.Id)
        $r0 = New-Object Win+RECT
        $big = $h -ne [IntPtr]::Zero -and [Win]::GetWindowRect($h, [ref]$r0) -and ($r0.R - $r0.L) -gt 200
    } until ($big)
    # The app places itself during its first frames (its saved size and position).
    Start-Sleep -Seconds 2
    # Topmost at a fixed place and size, so screenshots are comparable.
    [Win]::SetWindowPos($h, [IntPtr](-1), 40, 40, $Width, $Height, 0x40) | Out-Null
    Start-Sleep -Milliseconds 800
    $lp = { param($x, $y) [IntPtr](([int64]$y -shl 16) -bor ([int64]$x -band 0xFFFF)) }
    foreach ($c in $Click) {
        $x, $y = $c.Split(',') | ForEach-Object { [int]$_ }
        [Win]::PostMessage($h, 0x200, [IntPtr]0, (& $lp $x $y)) | Out-Null
        [Win]::PostMessage($h, 0x201, [IntPtr]1, (& $lp $x $y)) | Out-Null
        Start-Sleep -Milliseconds 60
        [Win]::PostMessage($h, 0x202, [IntPtr]0, (& $lp $x $y)) | Out-Null
        Start-Sleep -Milliseconds 400
    }
    if ($Run) {
        # F5 (Run all), posted to the window: focus is not needed.
        # lParam carries the scan code (0x3F), which winit reads.
        [Win]::PostMessage($h, 0x100, [IntPtr]0x74, [IntPtr]0x003F0001) | Out-Null
        Start-Sleep -Milliseconds 50
        [Win]::PostMessage($h, 0x101, [IntPtr]0x74, [IntPtr]([int64]0xC03F0001)) | Out-Null
    }
    Start-Sleep -Seconds $Settle
    if ($Scroll -ne 0) {
        # Wheel over the middle of the document (screen coordinates), one notch at a time.
        $mx = 40 + [int]($Width * 0.45); $my = 40 + [int]($Height * 0.5)
        [Win]::PostMessage($h, 0x200, [IntPtr]0, (& $lp ([int]($Width * 0.45)) ([int]($Height * 0.5)))) | Out-Null
        for ($i = 0; $i -lt [math]::Abs($Scroll); $i++) {
            $delta = if ($Scroll -gt 0) { -120 } else { 120 }
            [Win]::PostMessage($h, 0x20A, [IntPtr](([int64]($delta -band 0xFFFF)) -shl 16), (& $lp $mx $my)) | Out-Null
            Start-Sleep -Milliseconds 80
        }
        Start-Sleep -Seconds 1
    }
    # Park the pointer over an empty corner, so no hover toolbar shows.
    [Win]::PostMessage($h, 0x200, [IntPtr]0, (& $lp ($Width - 60) ($Height - 80))) | Out-Null
    Start-Sleep -Milliseconds 500
    # The visible frame (without the invisible resize border), read just before capturing.
    $r = New-Object Win+RECT
    [Win]::DwmGetWindowAttribute($h, 9, [ref]$r, 16) | Out-Null
    $w = $r.R - $r.L
    $hh = $r.B - $r.T
    $bmp = New-Object System.Drawing.Bitmap $w, $hh
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($r.L, $r.T, 0, 0, (New-Object System.Drawing.Size $w, $hh))
    $g.Dispose()
    $outPath = [IO.Path]::GetFullPath((Join-Path (Get-Location) $Out))
    $bmp.Save($outPath, [System.Drawing.Imaging.ImageFormat]::Png)
    $bmp.Dispose()
    "saved $outPath ($w x $hh)"
} finally {
    if (-not $p.HasExited) { $p.Kill() }
}
