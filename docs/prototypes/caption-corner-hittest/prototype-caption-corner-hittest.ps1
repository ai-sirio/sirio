# PROTOTIPO - usa e getta. Ticket #289 (angolo) e #297 (DPI / monitor) sulla mappa #281.
#
#   ./prototype-caption-corner-hittest.ps1 [-Monitor <n>]
#
# Sposta la finestra di sirio sul monitor <n> (0 = primo enumerato), la massimizza e
# misura dove finiscono i caption button e cosa risponde l'hit-test.
#
# TRE TRAPPOLE, tutte gia' gestite qui, tutte capaci di produrre numeri PLAUSIBILI e
# SBAGLIATI invece di un errore:
#
# 1. Get-Process sirio puo' restituire piu' istanze, una senza finestra: filtrare su
#    MainWindowHandle -ne 0.
# 2. L'hit-test di gpui IGNORA le coordinate del messaggio. In gpui_windows/src/events.rs:947
#        let area = callback();     // nessun argomento
#    risponde per la posizione VIVA del cursore. Sondare con SendMessage passando le
#    coordinate nell'lParam legge HTCAPTION ovunque e sembra un fallimento del pulsante.
#    Va spostato il cursore sul punto e atteso ~700ms (a 260ms si legge il punto precedente).
# 3. Un processo DPI-unaware legge coordinate VIRTUALIZZATE: un 4K a 175% viene riportato
#    come 2194x1234 invece di 3840x2160, e ogni conto sui pixel e' sbagliato di 1.75x senza
#    che nulla protesti. SetProcessDpiAwarenessContext(-4) va chiamato per PRIMO.

param([int]$Monitor = 0)

$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Probe {
  [DllImport("user32.dll")] public static extern bool SetProcessDpiAwarenessContext(IntPtr v);
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr SendMessageTimeout(IntPtr h, uint m, IntPtr w, IntPtr l, uint f, uint t, out IntPtr r);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr h);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int cx, int cy, uint f);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern bool EnumDisplayMonitors(IntPtr h, IntPtr c, MonitorEnumProc p, IntPtr d);
  [DllImport("shcore.dll")] public static extern int GetDpiForMonitor(IntPtr m, int t, out uint x, out uint y);
  public delegate bool MonitorEnumProc(IntPtr m, IntPtr hdc, ref RECT r, IntPtr d);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
}
'@

# TRAPPOLA 3 - per primo, prima di qualunque lettura di coordinate.
$aware = [Probe]::SetProcessDpiAwarenessContext([IntPtr](-4))   # PER_MONITOR_AWARE_V2
if (-not $aware) { Write-Warning "DPI awareness non impostata: le coordinate saranno virtualizzate." }

function HtName([int]$c) {
  switch ($c) {
    -1 { 'HTTRANSPARENT' } 0 { 'HTNOWHERE' } 1 { 'HTCLIENT' } 2 { 'HTCAPTION' }
    8 { 'HTMINBUTTON' } 9 { 'HTMAXBUTTON' } 10 { 'HTLEFT' } 11 { 'HTRIGHT' }
    12 { 'HTTOP' } 13 { 'HTTOPLEFT' } 14 { 'HTTOPRIGHT' } 18 { 'HTBORDER' } 20 { 'HTCLOSE' }
    default { "altro($c)" }
  }
}
function HitTest([IntPtr]$hwnd, [int]$x, [int]$y) {
  $o = [IntPtr]::Zero
  $ok = [Probe]::SendMessageTimeout($hwnd, 0x0084, [IntPtr]::Zero, [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF)), 2, 2000, [ref]$o)
  if ($ok -eq [IntPtr]::Zero) { return $null }
  return [int]$o
}
Add-Type -AssemblyName System.Drawing
function PixelAt([int]$x, [int]$y) {
  $b = New-Object System.Drawing.Bitmap 1, 1
  $g = [System.Drawing.Graphics]::FromImage($b)
  $g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size 1, 1)); $g.Dispose()
  $c = $b.GetPixel(0, 0); $b.Dispose()
  return ("#{0:X2}{1:X2}{2:X2}" -f $c.R, $c.G, $c.B)
}

# --- monitor, in pixel FISICI ---
$script:mons = @()
$cb = [Probe+MonitorEnumProc]{
  param($m, $hdc, [ref]$r, $d)
  $dx = 0; $dy = 0; [void][Probe]::GetDpiForMonitor($m, 0, [ref]$dx, [ref]$dy)
  $rc = $r.Value
  $script:mons += [pscustomobject]@{ L=$rc.Left; T=$rc.Top; R=$rc.Right; B=$rc.Bottom; DPI=[int]$dx; scala=[math]::Round($dx/96, 3) }
  return $true
}
[void][Probe]::EnumDisplayMonitors([IntPtr]::Zero, [IntPtr]::Zero, $cb, [IntPtr]::Zero)
Write-Host "=== monitor (pixel fisici) ==="
for ($i = 0; $i -lt $script:mons.Count; $i++) {
  $m = $script:mons[$i]
  Write-Host ("[{0}] [{1},{2} .. {3},{4}]  {5}x{6}  DPI={7} ({8}%)" -f $i, $m.L, $m.T, $m.R, $m.B, ($m.R-$m.L), ($m.B-$m.T), $m.DPI, [math]::Round($m.scala*100))
}
if ($Monitor -ge $script:mons.Count) { throw "monitor $Monitor inesistente" }
$tgt = $script:mons[$Monitor]

# --- TRAPPOLA 1 - una sola istanza, quella con la finestra ---
$withWin = @(Get-Process sirio -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 })
if ($withWin.Count -eq 0) { throw "nessuna istanza di sirio con finestra." }
if ($withWin.Count -gt 1) { throw ("{0} istanze con finestra: lasciane una sola." -f $withWin.Count) }
$hwnd = $withWin[0].MainWindowHandle

# --- portare la finestra sul monitor scelto e massimizzarla ---
[void][Probe]::ShowWindow($hwnd, 9)      # SW_RESTORE
Start-Sleep -Milliseconds 500
[void][Probe]::SetWindowPos($hwnd, [IntPtr]::Zero, ($tgt.L + 120), ($tgt.T + 120), 1200, 800, 0x0004)
Start-Sleep -Milliseconds 900            # lascia arrivare WM_DPICHANGED
[void][Probe]::ShowWindow($hwnd, 3)      # SW_MAXIMIZE
Start-Sleep -Milliseconds 900
[void][Probe]::SetForegroundWindow($hwnd)
Start-Sleep -Milliseconds 500

$r = New-Object Probe+RECT
[void][Probe]::GetWindowRect($hwnd, [ref]$r)
$dpi = [Probe]::GetDpiForWindow($hwnd)
$scale = $dpi / 96.0

Write-Host ""
Write-Host ("=== monitor [{0}] ===" -f $Monitor)
Write-Host ("massimizzata: {0}   in primo piano: {1}" -f [Probe]::IsZoomed($hwnd), ([Probe]::GetForegroundWindow() -eq $hwnd))
Write-Host ("finestra: [{0},{1} .. {2},{3}]  {4}x{5}" -f $r.Left, $r.Top, $r.Right, $r.Bottom, ($r.Right-$r.Left), ($r.Bottom-$r.Top))
Write-Host ("DPI finestra: {0} (scala {1}x)" -f $dpi, $scale)
Write-Host ("sbordo: destra={0}px  alto={1}px" -f ($r.Right - $tgt.R), ($tgt.T - $r.Top))

# metriche LOGICHE 36x38 -> fisiche
$btn    = [int][math]::Round(36 * $scale)
$barMid = $r.Top + [int][math]::Round(19 * $scale)
$closeR = $r.Right; $closeL = $closeR - $btn
$maxL   = $closeL - $btn
$minL   = $maxL - $btn
Write-Host ("pulsante atteso: {0}px fisici (36 logici x {1})" -f $btn, $scale)
Write-Host ("chiudi [{0}, {1})   massimizza [{2}, {3})   minimizza [{4}, {5})" -f $closeL, $closeR, $maxL, $closeL, $minL, $maxL)

# --- c'e' una barriera per il cursore sul bordo destro di questo monitor? ---
$barrier = $true
foreach ($m in $script:mons) {
  if ($m -ne $tgt -and $tgt.R -ge $m.L -and $tgt.R -lt $m.R -and $barMid -ge $m.T -and $barMid -lt $m.B) { $barrier = $false }
}
Write-Host ("barriera del cursore sul bordo destro: {0}" -f $(if ($barrier) { 'SI (angolo di Fitts disponibile)' } else { 'NO - un altro monitor confina qui, il cursore prosegue' }))

# --- sondaggi: cursore sul punto, POI hit-test (TRAPPOLA 2) ---
$scrRight = $tgt.R - 1
$points = @(
  @{ n = 'bordo destro del monitor, alto'; x = $scrRight;  y = $tgt.T;  want = 'HTCLOSE' },
  @{ n = 'bordo destro, meta barra';       x = $scrRight;  y = $barMid; want = 'HTCLOSE' },
  @{ n = 'chiudi, centro visibile';        x = $closeL + [int]($btn/2); y = $barMid; want = 'HTCLOSE' },
  @{ n = 'chiudi, primo pixel';            x = $closeL;    y = $barMid; want = 'HTCLOSE' },
  @{ n = 'massimizza, centro';             x = $maxL + [int]($btn/2);   y = $barMid; want = 'HTMAXBUTTON' },
  @{ n = 'minimizza, centro';              x = $minL + [int]($btn/2);   y = $barMid; want = 'HTMINBUTTON' },
  @{ n = 'barra, fuori dai pulsanti';      x = $minL - (2*$btn);        y = $barMid; want = 'HTCAPTION' }
)
Write-Host ""
Write-Host "=== cursore spostato sul punto, poi WM_NCHITTEST (assestamento 700ms) ==="
# RISCALDAMENTO. Dopo il ciclo ripristina/sposta/massimizza il PRIMO sondaggio arriva
# troppo presto anche con 700ms e restituisce un falso HTCLIENT sull'angolo (misurato:
# 4 sondaggi consecutivi sullo stesso punto danno poi HTCLOSE, e la striscia y=0..31 e'
# tutta HTCLOSE). Un giro a vuoto prima del ciclo lo elimina.
[void][Probe]::SetCursorPos(($closeL + [int]($btn/2)), $barMid)
Start-Sleep -Milliseconds 1500
$rows = foreach ($p in $points) {
  [void][Probe]::SetCursorPos($p.x, $p.y)
  Start-Sleep -Milliseconds 700
  $cur = New-Object Probe+POINT; [void][Probe]::GetCursorPos([ref]$cur)
  $moved = if ($cur.X -ne $p.x -or $cur.Y -ne $p.y) { "{0},{1}" -f $cur.X, $cur.Y } else { '' }
  $name = $(if ($null -eq ($c = HitTest $hwnd $cur.X $cur.Y)) { 'NESSUNA RISPOSTA' } else { HtName $c })
  [pscustomobject]@{
    punto = $p.n; 'x,y' = ("{0},{1}" -f $p.x, $p.y); 'finito a' = $moved
    atteso = $p.want; ottenuto = $name
    ok = $(if ($name -eq $p.want) { 'SI' } else { 'NO' })
    'pixel sotto il cursore' = PixelAt $cur.X $cur.Y
  }
}
$rows | Format-Table -AutoSize | Out-String -Width 220 | Write-Host

# --- ritaglio dell'angolo, col cursore su chiudi ---
[void][Probe]::SetCursorPos(($closeL + [int]($btn/2)), $barMid)
Start-Sleep -Milliseconds 800
$w = [int](200 * $scale); $h = [int](44 * $scale)
$crop = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($crop)
$g.CopyFromScreen(($tgt.R - $w), $tgt.T, 0, 0, (New-Object System.Drawing.Size $w, $h)); $g.Dispose()
$out = Join-Path $PSScriptRoot ("corner-hover-mon{0}.png" -f $Monitor)
$crop.Save($out, [System.Drawing.Imaging.ImageFormat]::Png); $crop.Dispose()
Write-Host ("ritaglio ({0}x{1}) salvato: {2}" -f $w, $h, $out)
