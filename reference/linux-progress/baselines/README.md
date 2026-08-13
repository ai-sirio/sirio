# Baselines

`Scripts/linux-shot.sh` always writes to `../shot.png`, so every agent that runs it
overwrites whatever the last one captured. Anything worth comparing against later has to be
copied here under a dated name, or it does not survive the next pass.

- `2026-08-13-pre-cosmic.png` — the app immediately before the Pop!_OS COSMIC conversion
  began (1470x833, 8610 colours). Dark neutral surfaces, **orange** accent, sidebar with
  projects/worktrees, tab strip, chat composer, right-hand Files tree with git dots, usage
  figures in the status bar.

  Keep it. It is the only before-picture of the visual change, and it independently
  confirms `F-TAB-01`: the tab strip renders icon, title and close, and **no dirty
  indicator**.
