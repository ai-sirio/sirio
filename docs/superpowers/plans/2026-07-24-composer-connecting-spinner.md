# Composer Connecting Spinner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show a loading spinner in place of the chat composer's circular send button while the chat is in the `.connecting` state.

**Architecture:** Pure SwiftUI view change in `App/Chat/ChatComposerView.swift`. Add one computed flag and one non-interactive spinner view, then turn the existing binary send/stop switch in `controlBar` into a three-way switch. No model, controller, or persistence changes.

**Tech Stack:** Swift 6, SwiftUI, macOS 15+. Verification via `Scripts/ci.sh` (xcodebuild + swift test).

## Global Constraints

- UI-facing strings must be English (this change adds no user-visible strings; the spinner has no label).
- Value types for models; no mutation of existing state. This task only reads `controller.state`.
- Commit messages: Conventional Commits, lower-case imperative subject.
- `Scripts/ci.sh` must print `CI OK` before the work is considered done.

---

### Task 1: Connecting spinner in the composer control bar

**Files:**
- Modify: `App/Chat/ChatComposerView.swift` (add `isConnecting` near line 25; add `loadingButton` near the `sendButton`/`stopButton` block around line 287-315; change `controlBar` send/stop switch at lines 123-127)

**Interfaces:**
- Consumes: `controller.state` of type `ChatController.ChatState` (enum: `idle, connecting, ready, prompting, needsAuth, disconnected(message:)`), defined in `App/Chat/ChatController.swift:11`.
- Produces: nothing consumed by other tasks (single-task plan).

**Note on testing:** `ChatComposerView` is a SwiftUI view with no isolatable logic beyond a state read; the project has no view-snapshot test harness. Verification is the compiler (the new `switch`/view must type-check) plus `Scripts/ci.sh` and a manual smoke check. There is no unit test to write for this task — do not fabricate a test target.

- [ ] **Step 1: Add the `isConnecting` computed flag**

In `App/Chat/ChatComposerView.swift`, find (around line 25):

```swift
    private var isPrompting: Bool { controller.state == .prompting }
```

Add directly below it:

```swift
    private var isConnecting: Bool { controller.state == .connecting }
```

- [ ] **Step 2: Add the `loadingButton` view**

In the same file, find the `stopButton` computed property (around line 302-315). Immediately after its closing brace, add:

```swift
    private var loadingButton: some View {
        ProgressView()
            .controlSize(.small)
            .frame(width: 26, height: 26)
            .background(.quaternary, in: Circle())
            .help("Starting the agent…")
    }
```

- [ ] **Step 3: Turn the control-bar switch three-way**

In `controlBar` (around lines 123-127), replace:

```swift
            if isPrompting {
                stopButton
            } else {
                sendButton
            }
```

with:

```swift
            if isConnecting {
                loadingButton
            } else if isPrompting {
                stopButton
            } else {
                sendButton
            }
```

- [ ] **Step 4: Build to verify it type-checks**

Run: `Scripts/ci.sh`
Expected: prints `CI OK`. If the PTY test `spawnCapturesOutput` fails flakily, re-run `Scripts/ci.sh` (known flaky, may need several retries) until it prints `CI OK`.

- [ ] **Step 5: Commit**

```bash
cd /Users/enzopiopalmisano/Desktop/Progetti/tiller
git add App/Chat/ChatComposerView.swift
git commit -m "feat: show loading spinner in composer while connecting"
```

- [ ] **Step 6: Manual smoke check**

Open Tiller, start a chat, and confirm: while the agent is connecting the send button is replaced by a small spinner inside a grey circle; once the chat reaches `.ready` the spinner becomes the arrow send button; during an active turn the stop button still appears.
