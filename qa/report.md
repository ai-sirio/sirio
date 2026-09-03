# QA report — Sirio

Stato: **in corso** (aggiornato man mano). La checklist operativa è in `qa/checklist.md`.

## Ambiente reale vs. piano

Il piano prevedeva un Mac con computer-use dentro herdr. La sessione è girata invece su:

- **Windows 11 Pro (10.0.28000)**, due monitor (primario 3440×1440, secondario 2560×1440 a sinistra).
- Sessione Claude Code dentro un pane herdr 0.8.2 (`w2:p1`), repo `D:\Progetti\sirio\sirio` su `main`.
- **Computer-use**: nessun tool nativo disponibile; sostituito da uno script PowerShell
  (`SendInput` per mouse/tastiera, `Graphics.CopyFromScreen` per gli screenshot, letti come immagini).
  Tutto ciò che è riportato come osservato è stato visto in uno screenshot; dove non è stato
  possibile osservare, è indicato esplicitamente.
- **Toolchain**: all'avvio della sessione non esistevano più né rustup/cargo, né MSVC, né Windows SDK
  (erano presenti il 29 agosto). Reinstallati: rustup `stable-x86_64-pc-windows-msvc` (rustc 1.98.1),
  VS 2022 Build Tools (workload VCTools, MSVC 14.44), sccache 0.17.0 prebuilt; Zig 0.15.2 già
  presente in `D:\toolchains\zig`. Prima build `cargo build -p sirio -p sirio_control`: 4m39s, 0 errori.
- Il binario preesistente (`sirio.exe` del 29/08) andava in panic all'avvio
  (`Error creating DirectWriteTextSystem`, os error 3): la build debug di gpui compila gli shader HLSL a
  runtime dal path del registry cargo, che era stato cancellato. Artefatto ambientale, non bug di Sirio.
- **Instabilità di sistema durante la sessione**: alle 20:54–21:07 sul PC girava un installer driver
  ASUS/AMD da `G:\BIN` (crash WER di `Instv2.exe`, `AsusSetup.exe`, poi `Explorer.EXE` in `amdxx64.dll`
  alle 21:07:15). Il primo avvio di Sirio (21:03) è caduto in quella finestra: le anomalie viste allora
  (dialog `cmd.exe 0xc0000142`, uscita silenziosa dell'app alle ~21:07) sono state riverificate dopo la
  fine dell'installazione prima di essere considerate.

## Funzionalità coperte

(vedi checklist)

## Issue aperte / chiuse

| Issue | Titolo | Severità | Agente | Tentativi | Stato |
|-------|--------|----------|--------|-----------|-------|

## Fix mergiate per agente

| Agente | Issue | Tentativi |
|--------|-------|-----------|

## Problemi notati ma non classificati come bug

- Log all'avvio: `[control] listening on /tmp\.local/state\Sirio\control.sock` — path con separatori
  misti su Windows (da verificare con il test del control socket).
