# Gestione permessi macOS — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pagina permessi macOS in stile Orca (Settings + onboarding one-shot al primo avvio): stato di 6 permessi TCC, prompt di sistema triggerabili una sola volta, deep link a System Settings dopo un denial.

**Architecture:** Logica pura e testabile (enum, mapping stato→azione, `PermissionsModel` con protocol `PermissionProbe`) in `Packages/TillerCore`; chiamate TCC reali (`SystemPermissionProbe`) e UI SwiftUI nel target App sotto `App/Permissions/`. La vista è unica e riusata in Settings e nello sheet di onboarding.

**Tech Stack:** Swift 6, SwiftUI, swift-testing (`import Testing`, `@Test`, `#expect`), UserNotifications, CoreGraphics (screen capture), ApplicationServices (AX + Apple Events), Network (NWBrowser).

**Spec:** `docs/superpowers/specs/2026-07-10-macos-permissions-design.md`

## Global Constraints

- macOS deployment target: **15.0**; Swift **6.0** (strict concurrency: tutti i tipi condivisi devono essere `Sendable`).
- Convenzione codebase: **commenti in italiano**, UI copy e identificatori **in inglese**, commit message in italiano formato conventional (`feat:`, `docs:`, …).
- Test framework: **swift-testing**, NON XCTest. Test di TillerCore in `Packages/TillerCore/Tests/TillerCoreTests/`.
- Comando test TillerCore: `swift test --package-path Packages/TillerCore` (da root repo).
- Comando build app: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build | tail -5`
- Gate finale: `Scripts/ci.sh` (rigenera progetto, builda, esegue i test di tutti i package).
- Nessun prompt TCC automatico all'avvio: i prompt partono solo da azione utente esplicita.
- Il flag esistente `hasRequestedNotifyAuth` di `App/AgentNotifier.swift` NON va toccato.

---

### Task 1: Tipi permesso e mapping stato→azione (TillerCore)

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/Permissions.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/PermissionsTests.swift`

**Interfaces:**
- Consumes: niente (foglia).
- Produces: `PermissionKind` (enum, `CaseIterable`, `Identifiable`, proprietà `title/symbol/detail: String`), `PermissionStatus` (enum: `.granted/.denied/.notRequested/.checkManually`, proprietà `badgeLabel: String`), `PermissionAction` (enum: `.request/.triggerPrompt/.openSettings`, proprietà `label: String`), `PermissionPresentation.action(for:status:) -> PermissionAction`. Tutti `public`, `Sendable`, `Equatable`.

- [ ] **Step 1: Scrivi i test che falliscono**

Crea `Packages/TillerCore/Tests/TillerCoreTests/PermissionsTests.swift`:

```swift
import Testing
@testable import TillerCore

@Test func sixKindsInOrcaOrder() {
    #expect(PermissionKind.allCases == [
        .notifications, .screenRecording, .accessibility,
        .fullDiskAccess, .automation, .localNetwork,
    ])
}

@Test func badgeLabels() {
    #expect(PermissionStatus.granted.badgeLabel == "GRANTED")
    #expect(PermissionStatus.denied.badgeLabel == "DENIED")
    #expect(PermissionStatus.notRequested.badgeLabel == "NOT REQUESTED")
    #expect(PermissionStatus.checkManually.badgeLabel == "CHECK MANUALLY")
}

@Test func actionLabels() {
    #expect(PermissionAction.request.label == "Request")
    #expect(PermissionAction.triggerPrompt.label == "Trigger Prompt")
    #expect(PermissionAction.openSettings.label == "Open Settings")
}

@Test func promptableKindsRequestOnlyWhenNotRequested() {
    for kind in [PermissionKind.notifications, .screenRecording, .accessibility] {
        #expect(PermissionPresentation.action(for: kind, status: .notRequested) == .request)
        #expect(PermissionPresentation.action(for: kind, status: .granted) == .openSettings)
        #expect(PermissionPresentation.action(for: kind, status: .denied) == .openSettings)
        #expect(PermissionPresentation.action(for: kind, status: .checkManually) == .openSettings)
    }
}

@Test func fullDiskAccessAlwaysOpensSettings() {
    for status in [PermissionStatus.granted, .denied, .notRequested, .checkManually] {
        #expect(PermissionPresentation.action(for: .fullDiskAccess, status: status) == .openSettings)
    }
}

@Test func automationTriggersUntilResolved() {
    #expect(PermissionPresentation.action(for: .automation, status: .notRequested) == .triggerPrompt)
    #expect(PermissionPresentation.action(for: .automation, status: .checkManually) == .triggerPrompt)
    // Dopo un esito definitivo il prompt Apple Events non riappare: solo System Settings.
    #expect(PermissionPresentation.action(for: .automation, status: .granted) == .openSettings)
    #expect(PermissionPresentation.action(for: .automation, status: .denied) == .openSettings)
}

@Test func localNetworkAlwaysTriggers() {
    for status in [PermissionStatus.granted, .denied, .notRequested, .checkManually] {
        #expect(PermissionPresentation.action(for: .localNetwork, status: status) == .triggerPrompt)
    }
}

@Test func everyKindHasDisplayCopy() {
    for kind in PermissionKind.allCases {
        #expect(!kind.title.isEmpty)
        #expect(!kind.symbol.isEmpty)
        #expect(!kind.detail.isEmpty)
    }
}
```

- [ ] **Step 2: Esegui i test e verifica che falliscano**

Run: `swift test --package-path Packages/TillerCore 2>&1 | tail -20`
Expected: FAIL di compilazione, "cannot find 'PermissionKind' in scope".

- [ ] **Step 3: Implementazione minima**

Crea `Packages/TillerCore/Sources/TillerCore/Permissions.swift`:

```swift
import Foundation

/// I permessi macOS (TCC) rilevanti per Tiller. Servono soprattutto ai CLI
/// agent nei terminali: i processi figli ereditano il TCC envelope dell'app
/// host, quindi i permessi vanno concessi al bundle di Tiller.
public enum PermissionKind: String, CaseIterable, Identifiable, Sendable {
    case notifications
    case screenRecording
    case accessibility
    case fullDiskAccess
    case automation
    case localNetwork

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .notifications: "Notifications"
        case .screenRecording: "Screen Recording"
        case .accessibility: "Accessibility"
        case .fullDiskAccess: "Full Disk Access"
        case .automation: "Automation"
        case .localNetwork: "Local Network"
        }
    }

    /// SF Symbol per la riga.
    public var symbol: String {
        switch self {
        case .notifications: "bell.badge"
        case .screenRecording: "record.circle"
        case .accessibility: "accessibility"
        case .fullDiskAccess: "internaldrive"
        case .automation: "gearshape.2"
        case .localNetwork: "network"
        }
    }

    public var detail: String {
        switch self {
        case .notifications: "Alerts when agents finish or need input."
        case .screenRecording: "Screenshot, visual automation, and UI inspection tools."
        case .accessibility: "Keystroke injection, window control, and UI automation tools."
        case .fullDiskAccess: "Recommended when projects or worktrees touch macOS-protected folders."
        case .automation: "Apple Events for scripts that control other local apps."
        case .localNetwork: "Discovery and access for development servers on your network."
        }
    }
}

/// Stato TCC osservato. `checkManually` copre i permessi senza API di check
/// affidabile (Full Disk Access, Local Network) o stati non determinabili.
public enum PermissionStatus: Sendable, Equatable {
    case granted
    case denied
    case notRequested
    case checkManually

    public var badgeLabel: String {
        switch self {
        case .granted: "GRANTED"
        case .denied: "DENIED"
        case .notRequested: "NOT REQUESTED"
        case .checkManually: "CHECK MANUALLY"
        }
    }
}

/// L'unica azione mostrata per riga, decisa da PermissionPresentation.
public enum PermissionAction: Sendable, Equatable {
    case request
    case triggerPrompt
    case openSettings

    public var label: String {
        switch self {
        case .request: "Request"
        case .triggerPrompt: "Trigger Prompt"
        case .openSettings: "Open Settings"
        }
    }
}

/// Mapping puro (kind, status) → azione. Regole:
/// - il prompt di sistema si può triggerare solo finché non è mai stato
///   mostrato (macOS non lo ripropone dopo una risposta);
/// - Full Disk Access non ha alcuna API di richiesta: solo System Settings;
/// - Local Network non ha API di check: il trigger Bonjour resta l'unica leva.
public enum PermissionPresentation {
    public static func action(for kind: PermissionKind, status: PermissionStatus) -> PermissionAction {
        switch kind {
        case .fullDiskAccess:
            .openSettings
        case .localNetwork:
            .triggerPrompt
        case .automation:
            switch status {
            case .notRequested, .checkManually: .triggerPrompt
            case .granted, .denied: .openSettings
            }
        case .notifications, .screenRecording, .accessibility:
            status == .notRequested ? .request : .openSettings
        }
    }
}
```

- [ ] **Step 4: Esegui i test e verifica che passino**

Run: `swift test --package-path Packages/TillerCore 2>&1 | tail -5`
Expected: tutti PASS, exit 0.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/Permissions.swift Packages/TillerCore/Tests/TillerCoreTests/PermissionsTests.swift
git commit -m "feat: tipi permesso macOS e mapping stato→azione"
```

---

### Task 2: PermissionProbe + PermissionsModel (TillerCore)

**Files:**
- Create: `Packages/TillerCore/Sources/TillerCore/PermissionsModel.swift`
- Test: `Packages/TillerCore/Tests/TillerCoreTests/PermissionsModelTests.swift`

**Interfaces:**
- Consumes: `PermissionKind`, `PermissionStatus`, `PermissionAction`, `PermissionPresentation` (Task 1).
- Produces:
  - `protocol PermissionProbe: Sendable` con `func status(for kind: PermissionKind) async -> PermissionStatus` e `func perform(_ action: PermissionAction, for kind: PermissionKind) async`.
  - `@MainActor @Observable public final class PermissionsModel` con `init(probe: any PermissionProbe)`, `status(for:) -> PermissionStatus`, `action(for:) -> PermissionAction`, `refresh() async`, `performAction(for:) async`.

- [ ] **Step 1: Scrivi i test che falliscono**

Crea `Packages/TillerCore/Tests/TillerCoreTests/PermissionsModelTests.swift`:

```swift
import Testing
@testable import TillerCore

/// Probe finto: stati scriptati per kind, registra le perform ricevute.
private actor ProbeLog {
    var performed: [(PermissionAction, PermissionKind)] = []
    var statuses: [PermissionKind: PermissionStatus]
    init(statuses: [PermissionKind: PermissionStatus]) { self.statuses = statuses }
    func record(_ action: PermissionAction, _ kind: PermissionKind) {
        performed.append((action, kind))
    }
    func set(_ kind: PermissionKind, _ status: PermissionStatus) { statuses[kind] = status }
    func status(_ kind: PermissionKind) -> PermissionStatus { statuses[kind] ?? .checkManually }
}

private struct MockProbe: PermissionProbe {
    let log: ProbeLog
    func status(for kind: PermissionKind) async -> PermissionStatus {
        await log.status(kind)
    }
    func perform(_ action: PermissionAction, for kind: PermissionKind) async {
        await log.record(action, kind)
    }
}

@Test @MainActor func unknownStatusDefaultsToCheckManually() {
    let model = PermissionsModel(probe: MockProbe(log: ProbeLog(statuses: [:])))
    #expect(model.status(for: .notifications) == .checkManually)
}

@Test @MainActor func refreshLoadsEveryKind() async {
    let log = ProbeLog(statuses: [
        .notifications: .granted, .screenRecording: .denied,
        .accessibility: .notRequested, .fullDiskAccess: .checkManually,
        .automation: .notRequested, .localNetwork: .checkManually,
    ])
    let model = PermissionsModel(probe: MockProbe(log: log))
    await model.refresh()
    #expect(model.status(for: .notifications) == .granted)
    #expect(model.status(for: .screenRecording) == .denied)
    #expect(model.status(for: .accessibility) == .notRequested)
    #expect(model.status(for: .fullDiskAccess) == .checkManually)
    #expect(model.status(for: .automation) == .notRequested)
    #expect(model.status(for: .localNetwork) == .checkManually)
}

@Test @MainActor func actionDelegatesToPresentation() async {
    let log = ProbeLog(statuses: [.notifications: .notRequested])
    let model = PermissionsModel(probe: MockProbe(log: log))
    await model.refresh()
    #expect(model.action(for: .notifications) == .request)
    #expect(model.action(for: .fullDiskAccess) == .openSettings)
}

@Test @MainActor func performActionInvokesProbeThenRefreshesThatKind() async {
    let log = ProbeLog(statuses: [.screenRecording: .notRequested])
    let model = PermissionsModel(probe: MockProbe(log: log))
    await model.refresh()
    // L'utente clicka Request; il sistema (mock) ora risponde granted.
    await log.set(.screenRecording, .granted)
    await model.performAction(for: .screenRecording)
    let performed = await log.performed
    #expect(performed.count == 1)
    #expect(performed[0] == (.request, .screenRecording))
    #expect(model.status(for: .screenRecording) == .granted)
}
```

- [ ] **Step 2: Esegui i test e verifica che falliscano**

Run: `swift test --package-path Packages/TillerCore 2>&1 | tail -20`
Expected: FAIL di compilazione, "cannot find 'PermissionsModel' in scope".

- [ ] **Step 3: Implementazione minima**

Crea `Packages/TillerCore/Sources/TillerCore/PermissionsModel.swift`:

```swift
import Foundation
import Observation

/// Sonda un permesso e ne esegue l'azione. L'implementazione reale
/// (SystemPermissionProbe, nel target App) fa le chiamate TCC; nei test si
/// usa un mock. Tenere le chiamate di sistema dietro questo protocol è ciò
/// che rende PermissionsModel testabile in CI.
public protocol PermissionProbe: Sendable {
    func status(for kind: PermissionKind) async -> PermissionStatus
    func perform(_ action: PermissionAction, for kind: PermissionKind) async
}

/// Stato osservabile della pagina permessi. Nessun refresh automatico
/// all'avvio: chi presenta la vista chiama refresh() (e ri-chiama quando
/// l'app torna in foreground, per raccogliere le modifiche fatte in
/// System Settings).
@MainActor @Observable
public final class PermissionsModel {
    public private(set) var statuses: [PermissionKind: PermissionStatus] = [:]
    private let probe: any PermissionProbe

    public init(probe: any PermissionProbe) {
        self.probe = probe
    }

    public func status(for kind: PermissionKind) -> PermissionStatus {
        statuses[kind] ?? .checkManually
    }

    public func action(for kind: PermissionKind) -> PermissionAction {
        PermissionPresentation.action(for: kind, status: status(for: kind))
    }

    public func refresh() async {
        for kind in PermissionKind.allCases {
            statuses[kind] = await probe.status(for: kind)
        }
    }

    /// Esegue l'azione corrente della riga e rilegge lo stato di quel solo
    /// permesso (i prompt di sistema sono modali: al ritorno lo stato può
    /// essere cambiato).
    public func performAction(for kind: PermissionKind) async {
        await probe.perform(action(for: kind), for: kind)
        statuses[kind] = await probe.status(for: kind)
    }
}
```

- [ ] **Step 4: Esegui i test e verifica che passino**

Run: `swift test --package-path Packages/TillerCore 2>&1 | tail -5`
Expected: tutti PASS, exit 0.

- [ ] **Step 5: Commit**

```bash
git add Packages/TillerCore/Sources/TillerCore/PermissionsModel.swift Packages/TillerCore/Tests/TillerCoreTests/PermissionsModelTests.swift
git commit -m "feat: PermissionsModel osservabile con probe iniettabile"
```

---

### Task 3: SystemPermissionProbe — chiamate TCC reali (App)

**Files:**
- Create: `App/Permissions/SystemPermissionProbe.swift`

**Interfaces:**
- Consumes: `PermissionProbe`, `PermissionKind`, `PermissionStatus`, `PermissionAction` da TillerCore (Task 1-2).
- Produces: `struct SystemPermissionProbe: PermissionProbe` (usata da Task 4). Nessun test unitario: solo I/O di sistema non mockabile, verificato a build + manualmente.

- [ ] **Step 1: Implementazione**

Crea `App/Permissions/SystemPermissionProbe.swift`:

```swift
import AppKit
import UserNotifications
import ApplicationServices
import CoreGraphics
import Network
import TillerCore

/// Probe TCC reale. Solo I/O di sistema, zero logica di presentazione
/// (che vive in PermissionPresentation, testata in TillerCore).
///
/// Nota su screenRecording/accessibility: macOS non distingue "mai chiesto"
/// da "negato" via API, quindi il probe persiste un flag "già richiesto" su
/// UserDefaults: preflight negativo + flag ⇒ denied, senza flag ⇒ notRequested.
struct SystemPermissionProbe: PermissionProbe {

    // MARK: - Status

    func status(for kind: PermissionKind) async -> PermissionStatus {
        switch kind {
        case .notifications:
            let settings = await UNUserNotificationCenter.current().notificationSettings()
            return switch settings.authorizationStatus {
            case .authorized, .provisional: .granted
            case .denied: .denied
            default: .notRequested
            }

        case .screenRecording:
            if CGPreflightScreenCaptureAccess() { return .granted }
            return hasRequested(kind) ? .denied : .notRequested

        case .accessibility:
            if AXIsProcessTrusted() { return .granted }
            return hasRequested(kind) ? .denied : .notRequested

        case .fullDiskAccess:
            // Nessuna API: il canary più affidabile è il database TCC stesso,
            // leggibile solo con Full Disk Access concesso.
            let tccPath = ("~/Library/Application Support/com.apple.TCC/TCC.db" as NSString)
                .expandingTildeInPath
            return FileHandle(forReadingAtPath: tccPath) != nil ? .granted : .checkManually

        case .automation:
            return automationStatus()

        case .localNetwork:
            // Nessuna API pubblica di check su macOS.
            return .checkManually
        }
    }

    // MARK: - Perform

    func perform(_ action: PermissionAction, for kind: PermissionKind) async {
        switch action {
        case .openSettings:
            openSystemSettings(for: kind)
        case .request, .triggerPrompt:
            await requestOrTrigger(kind)
        }
    }

    private func requestOrTrigger(_ kind: PermissionKind) async {
        switch kind {
        case .notifications:
            _ = try? await UNUserNotificationCenter.current()
                .requestAuthorization(options: [.alert, .sound])

        case .screenRecording:
            markRequested(kind)
            _ = CGRequestScreenCaptureAccess()

        case .accessibility:
            markRequested(kind)
            let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true]
            _ = AXIsProcessTrustedWithOptions(options as CFDictionary)

        case .automation:
            // Apple Event innocuo a System Events: forza il prompt di consenso.
            // NSAppleScript è bloccante → fuori dal main actor.
            await Task.detached {
                let script = NSAppleScript(
                    source: "tell application \"System Events\" to count processes")
                script?.executeAndReturnError(nil)
            }.value

        case .localNetwork:
            // Breve browse Bonjour: tocca la rete locale e triggera il prompt.
            let browser = NWBrowser(
                for: .bonjour(type: "_http._tcp", domain: nil),
                using: NWParameters())
            browser.start(queue: .global())
            try? await Task.sleep(for: .seconds(2))
            browser.cancel()

        case .fullDiskAccess:
            // Mai raggiunto: PermissionPresentation mappa FDA solo su openSettings.
            openSystemSettings(for: kind)
        }
    }

    // MARK: - Automation (Apple Events)

    private func automationStatus() -> PermissionStatus {
        // Il consenso Automation è per-target: si sonda System Events, il
        // bersaglio più comune degli script degli agenti.
        var addr = AEAddressDesc()
        let bundleID = "com.apple.systemevents"
        let created = bundleID.utf8CString.withUnsafeBufferPointer { buf in
            AECreateDesc(typeApplicationBundleID, buf.baseAddress, buf.count - 1, &addr)
        }
        guard created == noErr else { return .checkManually }
        defer { AEDisposeDesc(&addr) }

        return switch AEDeterminePermissionToAutomateTarget(
            &addr, typeWildCard, typeWildCard, false) {
        case noErr: .granted
        case -1743: .denied        // errAEEventNotPermitted
        case -1744: .notRequested  // errAEEventWouldRequireUserConsent
        default: .checkManually    // es. procNotFound: System Events non attivo
        }
    }

    // MARK: - System Settings deep link

    private func openSystemSettings(for kind: PermissionKind) {
        let url: String = switch kind {
        case .notifications: "x-apple.systempreferences:com.apple.Notifications-Settings.extension"
        case .screenRecording: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
        case .accessibility: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
        case .fullDiskAccess: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles"
        case .automation: "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"
        case .localNetwork: "x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork"
        }
        if let url = URL(string: url) { NSWorkspace.shared.open(url) }
    }

    // MARK: - Flag "già richiesto" (per distinguere denied da notRequested)

    private func requestedKey(_ kind: PermissionKind) -> String {
        "permissions.requested.\(kind.rawValue)"
    }

    private func hasRequested(_ kind: PermissionKind) -> Bool {
        UserDefaults.standard.bool(forKey: requestedKey(kind))
    }

    private func markRequested(_ kind: PermissionKind) {
        UserDefaults.standard.set(true, forKey: requestedKey(kind))
    }
}
```

- [ ] **Step 2: Build e verifica**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -5`
Expected: `BUILD SUCCEEDED`.

- [ ] **Step 3: Commit**

```bash
git add App/Permissions/SystemPermissionProbe.swift
git commit -m "feat: SystemPermissionProbe con chiamate TCC reali"
```

---

### Task 4: Pagina Permissions nelle Settings (App)

**Files:**
- Create: `App/Permissions/PermissionsSettingsView.swift`
- Modify: `App/AppRoute.swift` (enum `SettingsCategory`, righe 11-31 circa: nuovo case)
- Modify: `App/SettingsSurface.swift` (`detailPane`, righe 63-70: nuovo case)

**Interfaces:**
- Consumes: `PermissionsModel`, `PermissionKind`, `PermissionStatus`, `PermissionAction` (TillerCore), `SystemPermissionProbe` (Task 3), `AppTheme` (esistente in App).
- Produces: `struct PermissionsSettingsView: View` (riusata dal Task 5), case `SettingsCategory.permissions`.

- [ ] **Step 1: Aggiungi il case a SettingsCategory**

In `App/AppRoute.swift`, l'enum attuale è:

```swift
enum SettingsCategory: String, CaseIterable, Identifiable {
    case aiProviders
    case general
    case appearance
```

Aggiungi `case permissions` dopo `case general`, e i rami nelle proprietà `title` e `symbol` (stesso stile switch esistente):

```swift
    case permissions
```

In `title`: `case .permissions: "Permissions"`.
In `symbol`: `case .permissions: "lock.shield"`.

- [ ] **Step 2: Crea la vista**

Crea `App/Permissions/PermissionsSettingsView.swift`:

```swift
import SwiftUI
import AppKit
import TillerCore

/// Pagina permessi macOS in stile Orca: banner informativo + una riga per
/// permesso con badge di stato e azione singola. Riusata sia come sezione
/// Settings sia dentro lo sheet di onboarding al primo avvio.
struct PermissionsSettingsView: View {
    @State private var model = PermissionsModel(probe: SystemPermissionProbe())

    var body: some View {
        Form {
            Section {
                LabeledContent {
                    Button("Refresh") {
                        Task { await model.refresh() }
                    }
                } label: {
                    Text("Terminal tools inherit Tiller's macOS privacy envelope.")
                    Text("Use these controls when a CLI or agent in a pane needs macOS privacy access. Tiller does not ask at startup.")
                }
            }

            Section("macOS Permissions") {
                ForEach(PermissionKind.allCases) { kind in
                    row(for: kind)
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .task { await model.refresh() }
        // L'utente torna da System Settings → rileggi gli stati.
        .onReceive(NotificationCenter.default.publisher(
            for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await model.refresh() }
        }
    }

    private func row(for kind: PermissionKind) -> some View {
        let status = model.status(for: kind)
        return LabeledContent {
            Button(model.action(for: kind).label) {
                Task { await model.performAction(for: kind) }
            }
        } label: {
            HStack(spacing: 8) {
                Image(systemName: kind.symbol)
                    .frame(width: 20)
                Text(kind.title)
                statusBadge(status)
            }
            Text(kind.detail)
        }
    }

    private func statusBadge(_ status: PermissionStatus) -> some View {
        Text(status.badgeLabel)
            .font(.caption2.weight(.semibold))
            .padding(.horizontal, 6)
            .padding(.vertical, 2)
            .foregroundStyle(badgeColor(status))
            .background(badgeColor(status).opacity(0.15))
            .clipShape(Capsule())
    }

    private func badgeColor(_ status: PermissionStatus) -> Color {
        switch status {
        case .granted: .green
        case .denied: .red
        case .notRequested, .checkManually: .secondary
        }
    }
}
```

Nota Swift: se `Color.secondary` non compila come `Color` (è `ShapeStyle`), usa `Color(nsColor: .secondaryLabelColor)` nei due casi grigi.

- [ ] **Step 3: Aggancia il detailPane**

In `App/SettingsSurface.swift`, dentro `detailPane`, aggiungi il ramo:

```swift
        case .permissions: PermissionsSettingsView()
```

- [ ] **Step 4: Build e verifica**

Run: `xcodegen generate && xcodebuild -project Tiller.xcodeproj -scheme Tiller -configuration Debug -derivedDataPath DerivedData CODE_SIGNING_ALLOWED=NO build 2>&1 | tail -5`
Expected: `BUILD SUCCEEDED`.

- [ ] **Step 5: Test TillerCore ancora verdi**

Run: `swift test --package-path Packages/TillerCore 2>&1 | tail -5`
Expected: tutti PASS.

- [ ] **Step 6: Commit**

```bash
git add App/Permissions/PermissionsSettingsView.swift App/AppRoute.swift App/SettingsSurface.swift
git commit -m "feat: sezione Permissions nelle Settings con stato e azioni TCC"
```

---

### Task 5: Onboarding one-shot al primo avvio + README

**Files:**
- Create: `App/Permissions/PermissionsOnboardingSheet.swift`
- Modify: `App/ContentView.swift` (body, righe 23-44 circa: aggiungi `.sheet` e `@AppStorage`)
- Modify: `README.md` (sezione Features, righe 26-36)

**Interfaces:**
- Consumes: `PermissionsSettingsView` (Task 4).
- Produces: `struct PermissionsOnboardingSheet: View` con `var onContinue: () -> Void`; flag UserDefaults `hasSeenPermissionsOnboarding`.

- [ ] **Step 1: Crea lo sheet**

Crea `App/Permissions/PermissionsOnboardingSheet.swift`:

```swift
import SwiftUI

/// Sheet mostrato una sola volta al primo avvio: stessa lista permessi della
/// pagina Settings più un bottone Continue. Il chiamante persiste il flag
/// `hasSeenPermissionsOnboarding`; qui nessuno stato.
struct PermissionsOnboardingSheet: View {
    var onContinue: () -> Void

    var body: some View {
        VStack(spacing: 0) {
            PermissionsSettingsView()
                .frame(width: 560, height: 460)
            Divider()
            HStack {
                Text("You can change these anytime in Settings → Permissions.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                Spacer()
                Button("Continue", action: onContinue)
                    .keyboardShortcut(.defaultAction)
            }
            .padding(12)
        }
    }
}
```

- [ ] **Step 2: Presentalo al primo avvio**

In `App/ContentView.swift`:

1. Aggiungi la property tra le altre `@AppStorage` (dopo la riga 11):

```swift
    @AppStorage("hasSeenPermissionsOnboarding") private var hasSeenPermissionsOnboarding = false
```

2. Nel `body`, aggiungi il modifier sulla `Group` esistente, dopo `.alert(...)`:

```swift
        .sheet(isPresented: .init(
            get: { !hasSeenPermissionsOnboarding },
            set: { if !$0 { hasSeenPermissionsOnboarding = true } }
        )) {
            PermissionsOnboardingSheet { hasSeenPermissionsOnboarding = true }
        }
```

- [ ] **Step 3: Aggiorna il README**

In `README.md`, sezione `## Features`, aggiungi in fondo all'elenco (dopo la riga "Provider usage tracking"):

```markdown
- 🔐 **macOS permissions page** — one-shot onboarding + Settings section to grant the TCC permissions agents inherit (notifications, screen recording, accessibility, full disk access, automation, local network)
```

- [ ] **Step 4: Build e verifica finale completa**

Run: `Scripts/ci.sh 2>&1 | tail -10`
Expected: `BUILD SUCCEEDED` e `CI OK` (tutti i package verdi).

- [ ] **Step 5: Commit**

```bash
git add App/Permissions/PermissionsOnboardingSheet.swift App/ContentView.swift README.md
git commit -m "feat: onboarding permessi one-shot al primo avvio"
```

---

## Verifica manuale post-implementazione (checklist utente)

Non automatizzabile — richiede prompt TCC reali:

1. Cancella i flag: `defaults delete dev.tiller.Tiller hasSeenPermissionsOnboarding` (bundle ID effettivo: verificare con `mdls -name kMDItemCFBundleIdentifier` sull'app buildata).
2. Lancia l'app → lo sheet permessi appare; "Continue" lo chiude; riavvio → non riappare.
3. Settings → Permissions: sei righe con badge coerenti.
4. "Request" su Notifications/Screen Recording/Accessibility → prompt di sistema (una volta sola); dopo risposta il badge si aggiorna al refresh/foreground.
5. "Trigger Prompt" su Automation → prompt Apple Events per System Events.
6. "Open Settings" apre il pannello giusto di System Settings.
