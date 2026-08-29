# PROTOTIPO - usa e getta. Ticket #289 sulla mappa #281.
#
# Domanda: massimizzata, la finestra sborda di 8px oltre ogni bordo dello schermo.
# Il pulsante chiudi sta a filo del bordo DELLA FINESTRA, quindi i suoi 8px piu' a
# destra cadono fuori dallo schermo. Puntare l'angolo esatto dello schermo colpisce
# comunque il pulsante chiudi (il caso di Fitts)?
#
# LEZIONE DELLA v1 (sbagliata): sondare WM_NCHITTEST con SendMessage passando le
# coordinate nell'lParam NON funziona. gpui_windows/src/events.rs:947 fa
#     let area = callback();     // nessun argomento
# cioe' chiede "quale controllo e' sotto il mouse ADESSO", ignorando l'lParam per
# quella decisione. Un sondaggio sintetico risponde quindi per la posizione reale
# del cursore, non per il punto richiesto: la v1 leggeva HTCAPTION ovunque perche'
# il cursore stava fermo sulla zona di trascinamento.
#
# v2: spostare davvero il cursore sul punto, lasciar girare il message loop, POI
# sondare. Cosi' i due strumenti (codice HT e pixel dell'hover) misurano lo stesso
# punto e possono essere confrontati.

$ErrorActionPreference = 'Stop'

Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class Probe {
  [DllImport("user32.dll", SetLastError=true)]
  public static extern IntPtr SendMessageTimeout(IntPtr hWnd, uint Msg, IntPtr wParam, IntPtr lParam, uint flags, uint timeout, out IntPtr result);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool IsZoomed(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr WindowFromPoint(POINT p);
  [DllImport("user32.dll")] public static extern IntPtr MonitorFromWindow(IntPtr hWnd, uint flags);
  [DllImport("user32.dll")] public static extern bool GetMonitorInfo(IntPtr hMonitor, ref MONITORINFO mi);
  [DllImport("user32.dll")] public static extern uint GetDpiForWindow(IntPtr hWnd);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }
  [StructLayout(LayoutKind.Sequential)] public struct MONITORINFO { public int cbSize; public RECT rcMonitor; public RECT rcWork; public uint dwFlags; }
}
'@

function HtName([int]$c) {
  switch ($c) {
    -2 { 'HTERROR' } -1 { 'HTTRANSPARENT' } 0 { 'HTNOWHERE' } 1 { 'HTCLIENT' }
    2 { 'HTCAPTION' } 3 { 'HTSYSMENU' } 4 { 'HTGROWBOX' } 5 { 'HTMENU' }
    8 { 'HTMINBUTTON' } 9 { 'HTMAXBUTTON' } 10 { 'HTLEFT' } 11 { 'HTRIGHT' }
    12 { 'HTTOP' } 13 { 'HTTOPLEFT' } 14 { 'HTTOPRIGHT' } 15 { 'HTBOTTOM' }
    16 { 'HTBOTTOMLEFT' } 17 { 'HTBOTTOMRIGHT' } 18 { 'HTBORDER' } 20 { 'HTCLOSE' }
    21 { 'HTHELP' } default { "sconosciuto($c)" }
  }
}

function HitTest([IntPtr]$hwnd, [int]$x, [int]$y) {
  $lp = [IntPtr](($y -shl 16) -bor ($x -band 0xFFFF))
  $out = [IntPtr]::Zero
  $ok = [Probe]::SendMessageTimeout($hwnd, 0x0084, [IntPtr]::Zero, $lp, 0x0002, 2000, [ref]$out)
  if ($ok -eq [IntPtr]::Zero) { return $null }
  return [int]$out
}

Add-Type -AssemblyName System.Drawing
function PixelAt([int]$x, [int]$y) {
  $bmp = New-Object System.Drawing.Bitmap 1, 1
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.CopyFromScreen($x, $y, 0, 0, (New-Object System.Drawing.Size 1, 1))
  $g.Dispose()
  $c = $bmp.GetPixel(0, 0)
  $bmp.Dispose()
  return ("#{0:X2}{1:X2}{2:X2}" -f $c.R, $c.G, $c.B)
}

# --- 1. Trovare LA finestra (l'insidia di #286: piu' istanze, una senza finestra) ---
$procs = @(Get-Process sirio -ErrorAction SilentlyContinue)
if ($procs.Count -eq 0) { throw "sirio non e' in esecuzione." }
$withWin = @($procs | Where-Object { $_.MainWindowHandle -ne 0 })
Write-Host ("processi sirio: {0}  |  con finestra: {1}" -f $procs.Count, $withWin.Count)
if ($withWin.Count -eq 0) { throw "nessuna istanza di sirio espone una finestra." }
if ($withWin.Count -gt 1) { throw ("{0} istanze con finestra: chiudile e lasciane una sola." -f $withWin.Count) }
$hwnd = $withWin[0].MainWindowHandle
Write-Host ("hwnd: 0x{0:X}  pid: {1}" -f [int64]$hwnd, $withWin[0].Id)

# --- 2. Massimizzare e portare in primo piano (l'hover richiede di stare sopra) ---
[void][Probe]::ShowWindow($hwnd, 3)   # SW_MAXIMIZE
Start-Sleep -Milliseconds 700
[void][Probe]::SetForegroundWindow($hwnd)
Start-Sleep -Milliseconds 400
$fgOk = ([Probe]::GetForegroundWindow() -eq $hwnd)

$zoomed = [Probe]::IsZoomed($hwnd)
$r = New-Object Probe+RECT
[void][Probe]::GetWindowRect($hwnd, [ref]$r)
$mi = New-Object Probe+MONITORINFO
$mi.cbSize = [System.Runtime.InteropServices.Marshal]::SizeOf($mi)
$mon = [Probe]::MonitorFromWindow($hwnd, 2)  # MONITOR_DEFAULTTONEAREST
[void][Probe]::GetMonitorInfo($mon, [ref]$mi)
$dpi = [Probe]::GetDpiForWindow($hwnd)

Write-Host ""
Write-Host "=== geometria ==="
Write-Host ("massimizzata: {0}   sirio in primo piano: {1}" -f $zoomed, $fgOk)
Write-Host ("finestra:  L={0} T={1} R={2} B={3}   ({4}x{5})" -f $r.Left, $r.Top, $r.Right, $r.Bottom, ($r.Right-$r.Left), ($r.Bottom-$r.Top))
Write-Host ("monitor:   L={0} T={1} R={2} B={3}   ({4}x{5})" -f $mi.rcMonitor.Left, $mi.rcMonitor.Top, $mi.rcMonitor.Right, $mi.rcMonitor.Bottom, ($mi.rcMonitor.Right-$mi.rcMonitor.Left), ($mi.rcMonitor.Bottom-$mi.rcMonitor.Top))
Write-Host ("sbordo a destra: {0}px   in alto: {1}px" -f ($r.Right - $mi.rcMonitor.Right), ($mi.rcMonitor.Top - $r.Top))
Write-Host ("DPI finestra: {0} ({1}%)" -f $dpi, [math]::Round($dpi/96*100))

# --- 3. Sondare, spostando davvero il cursore ---
$btn = 36
$closeR = $r.Right; $closeL = $closeR - $btn
$maxR = $closeL;    $maxL  = $maxR - $btn
$minR = $maxL;      $minL  = $minR - $btn
$barMid = $r.Top + 19
$scrRight = $mi.rcMonitor.Right - 1
$scrTop = $mi.rcMonitor.Top

Write-Host ""
Write-Host "=== aree attese (coordinate schermo) ==="
Write-Host ("chiudi     [{0}, {1})   visibile solo fino a x={2}" -f $closeL, $closeR, $scrRight)
Write-Host ("massimizza [{0}, {1})" -f $maxL, $maxR)
Write-Host ("minimizza  [{0}, {1})" -f $minL, $minR)

$points = @(
  @{ n = 'ANGOLO schermo (caso di Fitts)'; x = $scrRight;  y = $scrTop; want = 'HTCLOSE' },
  @{ n = 'bordo destro, meta barra';       x = $scrRight;  y = $barMid; want = 'HTCLOSE' },
  @{ n = 'chiudi, centro visibile';        x = $closeL+14; y = $barMid; want = 'HTCLOSE' },
  @{ n = 'chiudi, primo pixel';            x = $closeL;    y = $barMid; want = 'HTCLOSE' },
  @{ n = 'chiudi, riga 0 dello schermo';   x = $closeL+14; y = $scrTop; want = 'HTCLOSE' },
  @{ n = 'oltre il bordo (il cursore si ferma)'; x = $closeR-2; y = $barMid; want = 'HTCLOSE' },
  @{ n = 'massimizza, centro';             x = $maxL+18;   y = $barMid; want = 'HTMAXBUTTON' },
  @{ n = 'minimizza, centro';              x = $minL+18;   y = $barMid; want = 'HTMINBUTTON' },
  @{ n = 'barra, a sinistra dei pulsanti'; x = $minL-60;   y = $barMid; want = 'HTCAPTION' }
)

Write-Host ""
Write-Host "=== cursore spostato sul punto, poi WM_NCHITTEST + pixel ==="
$rows = foreach ($p in $points) {
  [void][Probe]::SetCursorPos($p.x, $p.y)
  Start-Sleep -Milliseconds 260          # lascia girare il message loop di gpui
  $cur = New-Object Probe+POINT
  [void][Probe]::GetCursorPos([ref]$cur)
  $clamped = if ($cur.X -ne $p.x -or $cur.Y -ne $p.y) { "{0},{1}" -f $cur.X, $cur.Y } else { '' }

  $code = HitTest $hwnd $cur.X $cur.Y
  $name = if ($null -eq $code) { 'NESSUNA RISPOSTA' } else { HtName $code }

  # pixel al centro del pulsante chiudi visibile, per vedere l'hover acceso o spento
  $px = PixelAt ($scrRight - 13) ($barMid)

  [pscustomobject]@{
    punto = $p.n; 'x,y' = ("{0},{1}" -f $p.x, $p.y); bloccato = $clamped
    atteso = $p.want; ottenuto = $name
    ok = $(if ($name -eq $p.want) { 'SI' } else { 'NO' })
    'pixel chiudi' = $px
  }
}
$rows | Format-Table -AutoSize | Out-String -Width 220 | Write-Host

# --- 4. Ritaglio dell'angolo col cursore parcheggiato li' ---
[void][Probe]::SetCursorPos($scrRight, $scrTop)
Start-Sleep -Milliseconds 800
$pt = New-Object Probe+POINT
$pt.X = $scrRight; $pt.Y = $scrTop
Write-Host ("finestra sotto l'angolo e' sirio? {0}" -f ([Probe]::WindowFromPoint($pt) -eq $hwnd))

$w = 200; $h = 44
$crop = New-Object System.Drawing.Bitmap $w, $h
$g = [System.Drawing.Graphics]::FromImage($crop)
$g.CopyFromScreen(($mi.rcMonitor.Right - $w), $scrTop, 0, 0, (New-Object System.Drawing.Size $w, $h))
$g.Dispose()
$outPng = Join-Path $PSScriptRoot 'corner-hover.png'
$crop.Save($outPng, [System.Drawing.Imaging.ImageFormat]::Png)
$crop.Dispose()
Write-Host ("ritaglio salvato: {0}" -f $outPng)
