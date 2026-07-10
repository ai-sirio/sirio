# Aggiornamento automatico — Design

**Data:** 2026-07-10
**Stato:** approvato

## Obiettivo

Sistema di auto-update in-app: quando esiste una versione più recente su
GitHub Releases, appare un toast in basso a destra con il numero di versione
e il bottone **Scarica**; finito il download il bottone diventa **Aggiorna e
riavvia**, che installa e rilancia l'app. Motore: Sparkle 2. UI: interamente
custom (nessuna finestra Sparkle).

## Contesto distribuzione

La pipeline release esiste già: tag `v*.*.*` → `release.yml` su runner
self-hosted → build firmata Developer ID, notarizzata, DMG pubblicato su
GitHub Releases (`e-palmisano/tiller`). L'auto-update si aggancia a questa
pipeline senza cambiarne la struttura.

## Architettura

Pattern del repo: logica pura e testabile in `Packages/TillerCore`, I/O e
SwiftUI in `App/`. Nessun nuovo package.

- **`UpdateState`** (TillerCore) — state machine pura del ciclo update:
  `idle → available(version) → downloading(progress) → readyToInstall →
  error(message)`. Espone le transizioni (update trovato, download avviato,
  progress, download completato, dismiss, errore) come metodi puri.
  Dismiss riporta a `idle`; nessuna persistenza (il toast riappare al
  check successivo).
- **`UpdaterModel`** (App/, `@Observable @MainActor`) — possiede
  `SPUUpdater` e implementa `SPUUserDriver`, il protocol con cui Sparkle 2
  delega la UI all'host: ogni callback (update trovato, progress, pronto a
  installare, errore) viene tradotto in una transizione di `UpdateState`.
  Espone `checkForUpdates()` per il check manuale.
- **`UpdateToastView`** (App/) — overlay in basso a destra sul contenuto
  principale (ZStack in ContentView). Rende gli stati di `UpdateState`:
  - `available`: "Tiller X.Y disponibile" + bottone **Scarica** + X di
    chiusura;
  - `downloading`: progress bar;
  - `readyToInstall`: bottone **Aggiorna e riavvia** (Sparkle installa e
    rilancia);
  - `error`: messaggio breve + X.
  - `idle`: nessun toast.

## Semantica dei check

- Check automatico all'avvio e ogni 24 ore (scheduler nativo Sparkle:
  `SUEnableAutomaticChecks` true, `SUScheduledCheckInterval` 86400).
  Silenzioso se non ci sono update.
- Solo controllo automatico, download sempre su azione utente
  (`SUAutomaticallyUpdate` false: il download parte solo dal bottone
  Scarica).
- Check manuale: bottone **Check for Updates** in Settings → General, con
  feedback inline "Sei aggiornato" quando non c'è niente; se c'è un update
  appare il toast standard.
- Chiusura del toast (X): nessuno stato persistito, riappare al check
  successivo (prossimo avvio o +24h).

## Pipeline release

- **Una tantum:** generazione coppia chiavi EdDSA con `generate_keys` di
  Sparkle. Chiave pubblica in Info.plist (`SUPublicEDKey`); chiave privata
  nei GitHub secrets del repo (`SPARKLE_ED_PRIVATE_KEY`).
- **Per release (step in `release.yml`, dopo il build del DMG):**
  `generate_appcast` firma il DMG con la chiave privata e produce
  `appcast.xml`, allegato agli asset della release insieme al DMG.
- **Feed URL** in Info.plist (`SUFeedURL`):
  `https://github.com/e-palmisano/tiller/releases/latest/download/appcast.xml`
  — URL stabile che punta sempre all'asset dell'ultima release; ogni
  release porta con sé il proprio appcast, zero hosting extra.

## Sicurezza

- La firma EdDSA di Sparkle protegge il canale di distribuzione: un
  artefatto non firmato con la chiave privata del progetto viene rifiutato
  anche se scaricato dal feed legittimo.
- Developer ID + notarizzazione (già in pipeline) restano invariati e
  soddisfano Gatekeeper al primo avvio post-update.
- La chiave privata EdDSA vive solo nei GitHub secrets; mai committata.

## Test

- Unit test su `UpdateState` in TillerCore: tutte le transizioni valide,
  dismiss da ogni stato visibile, errore durante download, progress
  monotono.
- `UpdaterModel` e Sparkle reale non testabili in CI (richiedono feed e
  bundle firmati): confinati in App/. Verifica manuale con una release
  fittizia (checklist nel piano di implementazione).

## Fuori scope

- Delta update.
- Canale beta / pre-release.
- "Salta questa versione".
- Toggle per disattivare i check automatici.
- Download automatico in background senza azione utente.
