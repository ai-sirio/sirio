# Aggiornamento automatico (Sparkle 2) — Piano di implementazione

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Toast in basso a destra quando esiste una release più nuova su GitHub: Scarica → progress → Aggiorna e riavvia. Motore Sparkle 2, UI custom.

**Architecture:** State machine pura `UpdateState` in TillerCore (testabile); `UpdaterModel` in App/ implementa `SPUUserDriver` e traduce i callback Sparkle in transizioni; `UpdateToastView` come overlay `bottomTrailing` su ContentView; appcast firmato EdDSA generato in release.yml e allegato alla release.

**Tech Stack:** Swift 6 strict concurrency, SwiftUI @Observable, swift-testing (`import Testing`, `@Test`, `#expect`), Sparkle 2 via SPM, XcodeGen, GitHub Actions self-hosted.

**Spec:** `docs/superpowers/specs/2026-07-10-auto-update-design.md`

## Global Constraints

- Deployment target macOS 15.0, Swift 6.0, strict concurrency: tipi pubblici TillerCore `Sendable`, UI `@MainActor`.
- Logica testabile SOLO in `Packages/TillerCore` (il target App non ha test target).
- Test con swift-testing, MAI XCTest. Convenzione file esistenti: funzioni `@Test` top-level.
- Feed URL: `https://github.com/e-palmisano/tiller/releases/latest/download/appcast.xml`.
- Copy UI: toast "Tiller {version} disponibile", bottoni "Scarica" e "Aggiorna e riavvia", Settings "Check for Updates" / "Sei aggiornato."
- Commit convenzionali in italiano (stile repo: `feat:`, `fix:`, `ci:`, `docs:`).
- MAI committare `.tokensave/`. MAI `git push`.
- Gate finale: `Scripts/ci.sh` (nota: `TillerTerminal.writeReachesChildStdin` può flakkare in parallelo — verde se rieseguito isolato).
- Dopo modifiche a `project.yml`: rigenerare con `xcodegen generate` prima di buildare.
- Build app locale: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build ENABLE_DEBUG_DYLIB=NO` (la debug dylib non firmata rompe il codesign postbuild).

---

### Task 1: UpdateState — state machine pura

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/UpdateState.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/UpdateStateTests.swift`

**Interfaces:**
- Consumes: niente.
- Produces: `public enum UpdateState: Equatable, Sendable` con casi `idle`, `checking`, `available(version: String)`, `downloading(version: String, progress: Double)`, `readyToInstall(version: String)`, `upToDate`, `error(message: String)` e transizioni pure `manualCheckStarted()`, `found(version:)`, `notFound()`, `downloadStarted()`, `downloadProgressed(_:)`, `downloadCompleted()`, `failed(message:)`, `dismissed()` — tutte `-> UpdateState`. Usate da Task 4 (UpdaterModel), Task 5 (toast), Task 6 (Settings).

- [ ] **Step 1: Scrivi i test (falliranno: tipo inesistente)**

```swift
// Packages/TillerCore/Tests/TillerCoreTests/UpdateStateTests.swift
import Testing
@testable import TillerCore

@Test func foundShowsAvailableWithVersion() {
    #expect(UpdateState.idle.found(version: "0.3.0") == .available(version: "0.3.0"))
}

@Test func manualCheckStartsOnlyFromRestingStates() {
    #expect(UpdateState.idle.manualCheckStarted() == .checking)
    #expect(UpdateState.upToDate.manualCheckStarted() == .checking)
    #expect(UpdateState.error(message: "x").manualCheckStarted() == .checking)
    let downloading = UpdateState.downloading(version: "0.3.0", progress: 0.5)
    #expect(downloading.manualCheckStarted() == downloading)
}

@Test func notFoundAfterManualCheckShowsUpToDate() {
    #expect(UpdateState.checking.notFound() == .upToDate)
}

@Test func notFoundAfterBackgroundCheckStaysSilent() {
    #expect(UpdateState.idle.notFound() == .idle)
    #expect(UpdateState.upToDate.notFound() == .idle)
}

@Test func downloadStartsOnlyFromAvailable() {
    let available = UpdateState.available(version: "0.3.0")
    #expect(available.downloadStarted() == .downloading(version: "0.3.0", progress: 0))
    #expect(UpdateState.idle.downloadStarted() == .idle)
}

@Test func progressIsMonotoneAndClamped() {
    let state = UpdateState.downloading(version: "0.3.0", progress: 0.5)
    #expect(state.downloadProgressed(0.7) == .downloading(version: "0.3.0", progress: 0.7))
    #expect(state.downloadProgressed(0.3) == state)
    #expect(state.downloadProgressed(1.5) == .downloading(version: "0.3.0", progress: 1.0))
    #expect(state.downloadProgressed(-1) == state)
}

@Test func progressIgnoredOutsideDownloading() {
    #expect(UpdateState.idle.downloadProgressed(0.5) == .idle)
}

@Test func downloadCompletedFromDownloadingOrAvailable() {
    // .available copre il caso Sparkle "update già scaricato in un run precedente":
    // showReady arriva senza callback di download.
    #expect(UpdateState.downloading(version: "0.3.0", progress: 1).downloadCompleted()
        == .readyToInstall(version: "0.3.0"))
    #expect(UpdateState.available(version: "0.3.0").downloadCompleted()
        == .readyToInstall(version: "0.3.0"))
    #expect(UpdateState.idle.downloadCompleted() == .idle)
}

@Test func failedAlwaysShowsError() {
    #expect(UpdateState.downloading(version: "0.3.0", progress: 0.5).failed(message: "rete")
        == .error(message: "rete"))
}

@Test func dismissedAlwaysReturnsToIdle() {
    #expect(UpdateState.available(version: "0.3.0").dismissed() == .idle)
    #expect(UpdateState.error(message: "x").dismissed() == .idle)
    #expect(UpdateState.upToDate.dismissed() == .idle)
}
```

- [ ] **Step 2: Verifica che falliscano**

Run: `swift test --package-path Packages/TillerCore --filter UpdateState`
Expected: errore di compilazione "cannot find 'UpdateState'".

- [ ] **Step 3: Implementa**

```swift
// Packages/TillerCore/Sources/TillerCore/UpdateState.swift

/// State machine pura del ciclo di auto-update. Le transizioni non valide
/// per lo stato corrente ritornano lo stato invariato: i callback di Sparkle
/// possono arrivare in ordini inattesi e non devono mai corrompere la UI.
public enum UpdateState: Equatable, Sendable {
    case idle
    /// Check esplicito dell'utente in corso; i check di sistema restano silenziosi.
    case checking
    case available(version: String)
    case downloading(version: String, progress: Double)
    case readyToInstall(version: String)
    /// Esito visibile solo dopo un check manuale.
    case upToDate
    case error(message: String)

    public func manualCheckStarted() -> UpdateState {
        switch self {
        case .idle, .upToDate, .error: return .checking
        default: return self
        }
    }

    public func found(version: String) -> UpdateState {
        switch self {
        case .idle, .checking, .upToDate, .error, .available:
            return .available(version: version)
        default:
            return self
        }
    }

    public func notFound() -> UpdateState {
        switch self {
        case .checking: return .upToDate
        case .idle, .upToDate: return .idle
        default: return self
        }
    }

    public func downloadStarted() -> UpdateState {
        guard case .available(let version) = self else { return self }
        return .downloading(version: version, progress: 0)
    }

    public func downloadProgressed(_ fraction: Double) -> UpdateState {
        guard case .downloading(let version, let progress) = self else { return self }
        let clamped = min(1, max(0, fraction))
        return .downloading(version: version, progress: max(progress, clamped))
    }

    public func downloadCompleted() -> UpdateState {
        switch self {
        case .downloading(let version, _), .available(let version):
            return .readyToInstall(version: version)
        default:
            return self
        }
    }

    public func failed(message: String) -> UpdateState {
        .error(message: message)
    }

    public func dismissed() -> UpdateState {
        .idle
    }
}
```

- [ ] **Step 4: Verifica verde**

Run: `swift test --package-path Packages/TillerCore --filter UpdateState`
Expected: 10 test PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/UpdateState.swift Packages/TillerCore/Tests/TillerCoreTests/UpdateStateTests.swift
git commit -m "feat: state machine UpdateState per il ciclo di auto-update"
```

---

### Task 2: AppVersion dal bundle

Oggi `AppVersion.current` è hardcoded `"0.1.0"` mentre il bundle è 0.2.0: Settings mostra una versione falsa, inaccettabile accanto a "Sei aggiornato".

**Files:**
- Modify: `Packages/TillerCore/Sources/TillerCore/AppVersion.swift` (intero file, oggi 3 righe)
- Modify: `Packages/TillerCore/Tests/TillerCoreTests/AppVersionTests.swift`

**Interfaces:**
- Produces: `AppVersion.current: String` (invariata per i chiamanti: `App/GeneralSettingsView.swift:15`) + `AppVersion.version(fromInfo:) -> String` testabile.

- [ ] **Step 1: Riscrivi il test**

Sostituisci il contenuto di `AppVersionTests.swift` con:

```swift
import Testing
@testable import TillerCore

@Test func versionReadsShortVersionFromInfo() {
    #expect(AppVersion.version(fromInfo: ["CFBundleShortVersionString": "1.2.3"]) == "1.2.3")
}

@Test func versionFallsBackWhenInfoMissing() {
    #expect(AppVersion.version(fromInfo: nil) == "0.0.0")
    #expect(AppVersion.version(fromInfo: [:]) == "0.0.0")
}
```

- [ ] **Step 2: Verifica che fallisca**

Run: `swift test --package-path Packages/TillerCore --filter AppVersion`
Expected: errore di compilazione "no member 'version'".

- [ ] **Step 3: Implementa**

```swift
// Packages/TillerCore/Sources/TillerCore/AppVersion.swift
import Foundation

public enum AppVersion {
    /// Versione marketing dal bundle dell'app (CFBundleShortVersionString di
    /// project.yml). Nei test il bundle è quello del runner: usare
    /// version(fromInfo:) per la logica.
    public static var current: String {
        version(fromInfo: Bundle.main.infoDictionary)
    }

    public static func version(fromInfo info: [String: Any]?) -> String {
        info?["CFBundleShortVersionString"] as? String ?? "0.0.0"
    }
}
```

- [ ] **Step 4: Verde**

Run: `swift test --package-path Packages/TillerCore --filter AppVersion`
Expected: 2 test PASS.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/AppVersion.swift Packages/TillerCore/Tests/TillerCoreTests/AppVersionTests.swift
git commit -m "fix: AppVersion legge la versione dal bundle invece del literal 0.1.0"
```

---

### Task 3: Sparkle SPM + chiavi EdDSA + Info.plist

Nessun codice Swift: dipendenza, chiavi, configurazione. La chiave privata NON deve mai finire nel repo.

**Files:**
- Modify: `project.yml`
- Modify: `Scripts/check-release-version.sh`

**Interfaces:**
- Produces: modulo `Sparkle` importabile dal target App (Task 4); chiavi `SUFeedURL`, `SUPublicEDKey`, `SUEnableAutomaticChecks`, `SUScheduledCheckInterval`, `SUAutomaticallyUpdate` in Info.plist; secret repo `SPARKLE_ED_PRIVATE_KEY` (Task 7).

- [ ] **Step 1: Genera le chiavi EdDSA (una tantum)**

```bash
curl -fsSL -o /tmp/sparkle-dist.tar.xz https://github.com/sparkle-project/Sparkle/releases/download/2.8.0/Sparkle-2.8.0.tar.xz
mkdir -p /tmp/sparkle-dist && tar -xf /tmp/sparkle-dist.tar.xz -C /tmp/sparkle-dist
/tmp/sparkle-dist/bin/generate_keys
```

Expected: stampa `SUPublicEDKey` (base64, ~44 char). La chiave privata resta nel login Keychain ("Private key for signing Sparkle updates"). Annota la chiave pubblica per lo Step 3.

- [ ] **Step 2: Esporta la privata nel secret GitHub, poi elimina il file**

```bash
/tmp/sparkle-dist/bin/generate_keys -x /tmp/sparkle_ed_private_key
gh secret set SPARKLE_ED_PRIVATE_KEY --repo e-palmisano/tiller < /tmp/sparkle_ed_private_key
rm -f /tmp/sparkle_ed_private_key
gh secret list --repo e-palmisano/tiller | grep SPARKLE
```

Expected: `SPARKLE_ED_PRIVATE_KEY` in lista. Il Keychain resta l'unico backup della privata: se si perde, gli utenti esistenti non possono più aggiornare (chiave pubblica baked nelle app installate).

- [ ] **Step 3: project.yml — package, dipendenza, Info.plist**

In `packages:` aggiungi:

```yaml
  Sparkle:
    url: https://github.com/sparkle-project/Sparkle
    from: 2.8.0
```

Nelle `dependencies:` del target Tiller aggiungi:

```yaml
      - package: Sparkle
```

In `info.properties` del target Tiller: cambia `CFBundleVersion` e aggiungi le chiavi Sparkle (sostituisci `<PUBLIC_KEY>` con l'output dello Step 1):

```yaml
        CFBundleShortVersionString: "0.2.0"
        # Sparkle confronta sparkle:version = CFBundleVersion: deve crescere
        # a ogni release. Regola: identico a CFBundleShortVersionString.
        CFBundleVersion: "0.2.0"
        SUFeedURL: https://github.com/e-palmisano/tiller/releases/latest/download/appcast.xml
        SUPublicEDKey: "<PUBLIC_KEY>"
        SUEnableAutomaticChecks: true
        SUScheduledCheckInterval: 86400
        SUAutomaticallyUpdate: false
```

- [ ] **Step 4: Estendi il check di release alla CFBundleVersion**

In `Scripts/check-release-version.sh`, dopo il blocco che valida `PLIST_VERSION` (riga 29-32), aggiungi:

```bash
BUILD_VERSION=$(grep -m1 'CFBundleVersion:' "$PROJECT_YML" | sed -E 's/.*CFBundleVersion: *"([^"]+)".*/\1/')

if [ "$TAG_VERSION" != "$BUILD_VERSION" ]; then
  echo "error: tag version '$TAG_VERSION' does not match CFBundleVersion '$BUILD_VERSION' in $PROJECT_YML" >&2
  exit 1
fi
```

- [ ] **Step 5: Verifica**

```bash
Scripts/check-release-version.sh v0.2.0 project.yml
xcodegen generate
xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build ENABLE_DEBUG_DYLIB=NO
```

Expected: check stampa `0.2.0`; resolve di Sparkle e build SUCCEEDED.

- [ ] **Step 6: Commit**

```bash
git add project.yml Scripts/check-release-version.sh
git commit -m "feat: dipendenza Sparkle 2, chiavi feed EdDSA e CFBundleVersion per release"
```

---

### Task 4: UpdaterModel — SPUUserDriver custom

**Files:**
- Create: `App/Updates/UpdaterModel.swift`

**Interfaces:**
- Consumes: `UpdateState` (Task 1), modulo `Sparkle` (Task 3).
- Produces: `@MainActor @Observable final class UpdaterModel` con `state: UpdateState`, `start()`, `checkForUpdates()`, `download()`, `installAndRelaunch()`, `dismiss()`. Usata da Task 5 e 6.

- [ ] **Step 1: Implementa**

Nota firme: le signature `SPUUserDriver` sotto sono quelle di Sparkle 2.8. Se il compilatore segnala differenze (metodi mancanti/rinominati), adattare le firme al protocol mantenendo identico il mapping verso `UpdateState`. Se la conformance `@MainActor` dà errori di isolamento, avvolgere i corpi in `MainActor.assumeIsolated { }` (Sparkle chiama lo user driver sul main thread).

```swift
// App/Updates/UpdaterModel.swift
import AppKit
import Sparkle
import TillerCore

/// Possiede SPUUpdater e fa da SPUUserDriver: nessuna finestra Sparkle,
/// ogni callback diventa una transizione di UpdateState resa dal toast.
@MainActor
@Observable
final class UpdaterModel: NSObject {
    private(set) var state: UpdateState = .idle

    private var updater: SPUUpdater?
    private var updateChoice: ((SPUUserUpdateChoice) -> Void)?
    private var installReply: (() -> Void)?
    private var expectedLength: UInt64 = 0
    private var receivedLength: UInt64 = 0

    func start() {
        guard updater == nil else { return }
        let updater = SPUUpdater(
            hostBundle: .main,
            applicationBundle: .main,
            userDriver: self,
            delegate: nil
        )
        do {
            try updater.start()
            self.updater = updater
        } catch {
            state = state.failed(message: error.localizedDescription)
        }
    }

    func checkForUpdates() {
        state = state.manualCheckStarted()
        updater?.checkForUpdates()
    }

    func download() {
        updateChoice?(.install)
        updateChoice = nil
    }

    func installAndRelaunch() {
        installReply?()
        installReply = nil
    }

    func dismiss() {
        updateChoice?(.dismiss)
        updateChoice = nil
        state = state.dismissed()
    }
}

extension UpdaterModel: SPUUserDriver {
    func show(
        _ request: SPUUpdatePermissionRequest,
        reply: @escaping (SUUpdatePermissionResponse) -> Void
    ) {
        reply(SUUpdatePermissionResponse(automaticUpdateChecks: true, sendSystemProfile: false))
    }

    func showUserInitiatedUpdateCheck(cancellation: @escaping () -> Void) {}

    func showUpdateFound(
        with appcastItem: SUAppcastItem,
        state: SPUUserUpdateState,
        reply: @escaping (SPUUserUpdateChoice) -> Void
    ) {
        updateChoice = reply
        self.state = self.state.found(version: appcastItem.displayVersionString)
    }

    func showUpdateReleaseNotes(with downloadData: SPUDownloadData) {}

    func showUpdateReleaseNotesFailedToDownloadWithError(_ error: Error) {}

    func showUpdateNotFoundWithError(_ error: Error, acknowledgement: @escaping () -> Void) {
        state = state.notFound()
        acknowledgement()
    }

    func showUpdaterError(_ error: Error, acknowledgement: @escaping () -> Void) {
        state = state.failed(message: error.localizedDescription)
        acknowledgement()
    }

    func showDownloadInitiated(cancellation: @escaping () -> Void) {
        expectedLength = 0
        receivedLength = 0
        state = state.downloadStarted()
    }

    func showDownloadDidReceiveExpectedContentLength(_ expectedContentLength: UInt64) {
        expectedLength = expectedContentLength
    }

    func showDownloadDidReceiveData(ofLength length: UInt64) {
        receivedLength += length
        guard expectedLength > 0 else { return }
        state = state.downloadProgressed(Double(receivedLength) / Double(expectedLength))
    }

    func showDownloadDidStartExtractingUpdate() {}

    func showExtractionReceivedProgress(_ progress: Double) {}

    func showReady(toInstallAndRelaunch reply: @escaping (SPUUserUpdateChoice) -> Void) {
        installReply = { reply(.install) }
        state = state.downloadCompleted()
    }

    func showInstallingUpdate(
        withApplicationTerminated applicationTerminated: Bool,
        retryTerminatingApplication: @escaping () -> Void
    ) {}

    func showUpdateInstalledAndRelaunched(_ relaunched: Bool, acknowledgement: @escaping () -> Void) {
        acknowledgement()
    }

    func showUpdateInProgress() {}

    func dismissUpdateInstallation() {
        state = state.dismissed()
    }
}
```

- [ ] **Step 2: Build**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build ENABLE_DEBUG_DYLIB=NO`
Expected: BUILD SUCCEEDED (UpdaterModel non ancora referenziato: ok).

- [ ] **Step 3: Commit**

```bash
git add App/Updates/UpdaterModel.swift
git commit -m "feat: UpdaterModel con user driver Sparkle custom"
```

---

### Task 5: UpdateToastView + wiring in ContentView/TillerApp

**Files:**
- Create: `App/Updates/UpdateToastView.swift`
- Modify: `App/ContentView.swift` (proprietà + init + overlay)
- Modify: `App/TillerApp.swift` (istanza UpdaterModel + start)

**Interfaces:**
- Consumes: `UpdaterModel` (Task 4), `UpdateState` (Task 1), `AppTheme` esistente.
- Produces: `ContentView(model:updater:)` — Task 6 riusa lo stesso `updater` via `SettingsSurface`.

- [ ] **Step 1: Toast**

```swift
// App/Updates/UpdateToastView.swift
import SwiftUI
import TillerCore

/// Toast in basso a destra per il ciclo di update. Visibile solo negli stati
/// che chiedono un'azione; .checking e .upToDate vivono in Settings.
struct UpdateToastView: View {
    var updater: UpdaterModel

    var body: some View {
        switch updater.state {
        case .available(let version):
            toast(icon: "arrow.down.circle") {
                Text("Tiller \(version) disponibile")
                Button("Scarica") { updater.download() }
                    .buttonStyle(.borderedProminent)
            }
        case .downloading(let version, let progress):
            toast(icon: "arrow.down.circle") {
                Text("Download di Tiller \(version)…")
                ProgressView(value: progress)
                    .frame(width: 140)
            }
        case .readyToInstall(let version):
            toast(icon: "checkmark.circle") {
                Text("Tiller \(version) pronto")
                Button("Aggiorna e riavvia") { updater.installAndRelaunch() }
                    .buttonStyle(.borderedProminent)
            }
        case .error(let message):
            toast(icon: "exclamationmark.triangle") {
                Text(message).lineLimit(2)
            }
        case .idle, .checking, .upToDate:
            EmptyView()
        }
    }

    private func toast(icon: String, @ViewBuilder content: () -> some View) -> some View {
        HStack(spacing: 10) {
            Image(systemName: icon)
                .foregroundStyle(AppTheme.title)
            content()
            Button {
                updater.dismiss()
            } label: {
                Image(systemName: "xmark")
            }
            .buttonStyle(.plain)
            .foregroundStyle(AppTheme.subtitle)
        }
        .padding(12)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).stroke(AppTheme.hairline))
    }
}
```

- [ ] **Step 2: ContentView**

In `App/ContentView.swift`: aggiungi la proprietà e il parametro di init:

```swift
    var model: AppModel
    var updater: UpdaterModel
```

```swift
    init(model: AppModel, updater: UpdaterModel) {
        self.model = model
        self.updater = updater
        self.menuProvider = TerminalContextMenuProvider(model: model)
    }
```

Dopo il modifier `.sheet(isPresented:...)` (riga 45-50) aggiungi:

```swift
        .overlay(alignment: .bottomTrailing) {
            UpdateToastView(updater: updater)
                .padding(16)
        }
```

E in `body`, `case .settings:` diventa `SettingsSurface(model: model, updater: updater)` (parametro aggiunto nel Task 6 — per compilare questo task da solo, lascia `SettingsSurface(model: model)` e cambia la riga nel Task 6; se i task vengono eseguiti in sequenza nella stessa sessione va bene anticiparla qui e buildare a fine Task 6).

- [ ] **Step 3: TillerApp**

In `App/TillerApp.swift`:

```swift
    @State private var model = AppModel()
    @State private var updater = UpdaterModel()
```

e nel `WindowGroup`:

```swift
            ContentView(model: model, updater: updater)
                .preferredColorScheme(.dark)
                .onAppear {
                    appDelegate.model = model
                    updater.start()
                }
```

- [ ] **Step 4: Build**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build ENABLE_DEBUG_DYLIB=NO`
Expected: BUILD SUCCEEDED.

- [ ] **Step 5: Commit**

```bash
git add App/Updates/UpdateToastView.swift App/ContentView.swift App/TillerApp.swift
git commit -m "feat: toast di aggiornamento in basso a destra"
```

---

### Task 6: Check for Updates in Settings

**Files:**
- Modify: `App/GeneralSettingsView.swift`
- Modify: `App/SettingsSurface.swift`
- Modify: `App/ContentView.swift` (riga `case .settings:` se non già fatta nel Task 5)

**Interfaces:**
- Consumes: `UpdaterModel` (Task 4), `AppVersion.current` (Task 2).

- [ ] **Step 1: GeneralSettingsView**

Aggiungi la proprietà `var updater: UpdaterModel` e sostituisci la sezione About (righe 14-16) con:

```swift
            Section("About") {
                LabeledContent("Version", value: AppVersion.current)
                LabeledContent {
                    Button("Check for Updates") { updater.checkForUpdates() }
                } label: {
                    Text("Updates")
                    if case .checking = updater.state {
                        Text("Controllo in corso…")
                    } else if case .upToDate = updater.state {
                        Text("Sei aggiornato.")
                    }
                }
            }
```

- [ ] **Step 2: SettingsSurface**

Aggiungi `var updater: UpdaterModel` sotto `var model: AppModel` e cambia il detail pane: `case .general: GeneralSettingsView(updater: updater)`. In `App/ContentView.swift`: `case .settings: SettingsSurface(model: model, updater: updater)`.

- [ ] **Step 3: Build**

Run: `xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug build ENABLE_DEBUG_DYLIB=NO`
Expected: BUILD SUCCEEDED.

- [ ] **Step 4: Commit**

```bash
git add App/GeneralSettingsView.swift App/SettingsSurface.swift App/ContentView.swift
git commit -m "feat: Check for Updates in Settings con feedback inline"
```

---

### Task 7: Appcast nella pipeline release

**Files:**
- Modify: `.github/workflows/release.yml`
- Modify: `README.md` (bullet feature)

**Interfaces:**
- Consumes: secret `SPARKLE_ED_PRIVATE_KEY` (Task 3), step `Build DMG` esistente (output `steps.dmg.outputs.path`).
- Produces: asset `appcast.xml` su ogni release — il feed che `SUFeedURL` consuma.

- [ ] **Step 1: Step "Generate appcast"**

In `.github/workflows/release.yml`, tra lo step `Build DMG` e `Generate changelog`, aggiungi:

```yaml
      - name: Generate appcast
        env:
          SPARKLE_ED_PRIVATE_KEY: ${{ secrets.SPARKLE_ED_PRIVATE_KEY }}
          TAG_NAME: ${{ github.ref_name }}
          DMG_PATH: ${{ steps.dmg.outputs.path }}
        run: |
          curl -fsSL -o sparkle-dist.tar.xz https://github.com/sparkle-project/Sparkle/releases/download/2.8.0/Sparkle-2.8.0.tar.xz
          mkdir -p sparkle-dist
          tar -xf sparkle-dist.tar.xz -C sparkle-dist
          printf '%s' "$SPARKLE_ED_PRIVATE_KEY" > ed_key
          mkdir -p build/appcast
          cp "$DMG_PATH" build/appcast/
          sparkle-dist/bin/generate_appcast \
            --ed-key-file ed_key \
            --download-url-prefix "https://github.com/e-palmisano/tiller/releases/download/${TAG_NAME}/" \
            build/appcast
          rm -f ed_key
          test -f build/appcast/appcast.xml
```

- [ ] **Step 2: Allega l'appcast alla release**

Nello step `Publish release`, aggiungi l'asset:

```yaml
          gh release create "$TAG_NAME" \
            "$DMG_PATH" \
            build/appcast/appcast.xml \
            --title "Tiller $TAG_NAME" \
            --notes-file changelog.md
```

- [ ] **Step 3: Pulizia chiave anche su failure**

Nello step `Clean up keychain` (`if: always()`), aggiungi in fondo:

```bash
          rm -f ed_key
```

- [ ] **Step 4: README**

In `README.md`, nella lista feature, aggiungi:

```markdown
- **Auto-update** — toast in basso a destra quando esce una nuova release: Scarica, poi Aggiorna e riavvia (Sparkle 2, appcast firmato EdDSA su GitHub Releases).
```

- [ ] **Step 5: Verifica sintassi e commit**

Run: `ruby -ryaml -e 'YAML.load_file(".github/workflows/release.yml"); puts "yaml ok"'`
Expected: `yaml ok`.

```bash
git add .github/workflows/release.yml README.md
git commit -m "ci: genera e pubblica appcast.xml firmato nella release"
```

---

### Gate finale

- [ ] Run: `Scripts/ci.sh` — tutto verde (flake noto: `TillerTerminal.writeReachesChildStdin`, rieseguire isolato se rosso).

---

## Checklist di verifica manuale (non automatizzabile)

Il ciclo Sparkle completo richiede app firmata, appcast pubblicato e una versione installata più vecchia.

1. **Smoke test subito (build Debug):** avvia l'app → nessun toast (nessuna release > 0.2.0 con appcast). Settings → General: versione mostra `0.2.0` (fix AppVersion), bottone "Check for Updates" → "Controllo in corso…" poi "Sei aggiornato." oppure toast di errore (accettabile finché nessuna release ha ancora l'appcast: feed 404).
2. **Test end-to-end (alla prossima release):**
   - bump `CFBundleShortVersionString` E `CFBundleVersion` a `0.3.0` in project.yml, tag `v0.3.0`, push del tag → release con DMG + appcast.xml;
   - con Tiller **0.2.0+Sparkle** installata in `/Applications` (build firmata Developer ID): avvio → entro pochi secondi toast "Tiller 0.3.0 disponibile";
   - **Scarica** → progress bar; a fine download **Aggiorna e riavvia** → l'app si chiude, si reinstalla e si rilancia come 0.3.0;
   - X sul toast → sparisce; riavvio dell'app → toast riappare.
   - Nota: la 0.2.0 già pubblicata NON ha Sparkle: gli utenti 0.2.0 aggiornano a mano un'ultima volta.
3. **Quit path:** "Aggiorna e riavvia" passa da `applicationShouldTerminate` (flush scrollback + OnceGate deadline 1.5s): verificare che il riavvio non perda scrollback dei pane aperti.
