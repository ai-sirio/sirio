# Workspace Drag Overlay Test Host Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Prevent `WorkspaceDragOverlay.hitTest(_:)` from crashing when a test host has no `NSApp`, while preserving transparent hit testing and the existing AppKit package boundary.

**Architecture:** Keep the existing `@MainActor` `WorkspaceDragOverlay` implementation and make only its current-event lookup safely optional-chain `NSApp`. If either the application or current event is absent, `DividerCursorHitPolicy.acceptsHit(eventType:)` receives `nil`, so the overlay returns `nil` and does not swallow the click.

**Tech Stack:** Swift 6, AppKit, macOS 15+, Swift Testing, Swift Package Manager, repository CI script.

## Global Constraints

- Modify only `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift`.
- Do not modify `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceDragOverlayTests.swift`; the approved implementation requires no test change.
- Preserve `@MainActor`.
- Use the exact safe event lookup `NSApp?.currentEvent?.type`.
- Do not use `NSApplication.shared`, bootstrap an application in tests, introduce an event-provider abstraction, change stable-diff behavior, or perform unrelated cleanup/refactoring.
- RED is deterministic on both the feature branch and clean base: `WorkspaceDragOverlayTests.theOverlayNeverSwallowsAClick` fails with signal 5 at `WorkspaceDragOverlay.swift:44` because `NSApp` is nil.
- Run the focused test before and after the source change, then the full `TillerWorkspace` test suite and `Scripts/ci.sh`.
- Require `Scripts/ci.sh` to print `CI OK`.
- If committing, stage only `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift` and use `fix: handle missing app during overlay hit testing`.

---

### Task 1: Make overlay hit testing safe when the application is absent

**Files:**
- Modify: `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift:43-49`
- Test: `Packages/TillerWorkspace/Tests/TillerWorkspaceTests/WorkspaceDragOverlayTests.swift:7-13` (existing test; do not modify)

**Interfaces:**
- Consumes: `WorkspaceDragOverlay.hitTest(_ point: NSPoint) -> NSView?`, `DividerCursorHitPolicy.acceptsHit(eventType:)`, and AppKit's optional `NSApp`/`currentEvent` lookup.
- Produces: The same public `WorkspaceDragOverlay` type and `@MainActor` isolation, with `hitTest(_:)` returning `nil` whenever the event type is unavailable instead of implicitly unwrapping a missing `NSApp`.

- [ ] **Step 1: Run the focused test to verify the approved RED state**

Run from the worktree root:

```bash
cd Packages/TillerWorkspace && swift test --filter WorkspaceDragOverlayTests.theOverlayNeverSwallowsAClick
```

Expected outcome: FAIL with signal 5, reporting the crash at `WorkspaceDragOverlay.swift:44` because `NSApp` is nil in the test host. This confirms that the existing test is exercising the regression before the implementation change. Do not modify the test.

- [ ] **Step 2: Change only the event lookup in `hitTest(_:)`**

In `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift`, preserve the surrounding method, guard, return type, and `@MainActor` declaration. Change the guard's event lookup from:

```swift
guard DividerCursorHitPolicy.acceptsHit(eventType: NSApp.currentEvent?.type) else {
    return nil
}
```

to exactly:

```swift
guard DividerCursorHitPolicy.acceptsHit(eventType: NSApp?.currentEvent?.type) else {
    return nil
}
```

Do not add `NSApplication.shared`, application bootstrapping, dependency injection, test changes, stable-diff changes, or unrelated cleanup. The implicitly unwrapped `NSApp` is the crash source; optional chaining makes both a missing application and a missing current event produce a nil event type. The existing policy guard then rejects the hover hit, and `hitTest(_:)` returns `nil`.

- [ ] **Step 3: Run the focused test to verify GREEN**

Run:

```bash
cd Packages/TillerWorkspace && swift test --filter WorkspaceDragOverlayTests.theOverlayNeverSwallowsAClick
```

Expected outcome: PASS for `WorkspaceDragOverlayTests.theOverlayNeverSwallowsAClick`, with no signal 5 crash. The test host has no application event, so `NSApp?.currentEvent?.type` evaluates to `nil`, the policy guard returns early, and the overlay does not swallow the click.

- [ ] **Step 4: Run the complete `TillerWorkspace` test suite**

Run:

```bash
cd Packages/TillerWorkspace && swift test
```

Expected outcome: PASS for the full `TillerWorkspace` test suite, including the focused regression test and the existing flipped-overlay and clearing-feedback tests. No test source changes are expected.

- [ ] **Step 5: Run the repository verification gate**

Run from the worktree root:

```bash
Scripts/ci.sh
```

Expected outcome: all repository checks pass and the command ends with `CI OK`.

- [ ] **Step 6: Review the diff and commit only the approved source file**

Confirm that the only implementation change is the one-token optional-chain safety fix in `WorkspaceDragOverlay.hitTest(_:)`, and that `@MainActor`, the existing policy guard, the AppKit import, and all tests remain unchanged. Then stage and commit only the source file:

```bash
git diff -- Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift

git add Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift
git commit -m "fix: handle missing app during overlay hit testing"
```

Expected outcome: the commit contains only `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift` and has the exact message `fix: handle missing app during overlay hit testing`. Do not stage or commit the plan document, the test file, or any stable-diff/unrelated file.

## Self-Review

- **Spec coverage:** Complete. The plan identifies the exact one-file implementation scope, preserves `@MainActor` and the AppKit boundary, records the exact `NSApp?.currentEvent?.type` expression, explains the implicitly unwrapped `NSApp` crash, states the nil-event-to-`hitTest`-`nil` behavior, keeps the existing test unchanged, includes the deterministic RED command and expected signal 5 failure, includes the focused GREEN command, full package test command, repository CI command with required `CI OK`, and the source-only commit step with the exact message.
- **Placeholder scan:** Complete. The plan contains no `TBD`, `TODO`, “implement later,” “add appropriate error handling,” “write tests for the above,” or other placeholder instructions. All commands, paths, snippets, outcomes, interfaces, and scope are concrete.
- **Type consistency:** Complete. `hitTest(_:)` remains `NSView?` with `NSPoint`; `DividerCursorHitPolicy.acceptsHit(eventType:)` continues to receive the optional event type produced by `NSApp?.currentEvent?.type`; `@MainActor` and the existing `WorkspaceDragOverlay` type are unchanged.
- **Scope check:** Complete. This is one small TDD task in one implementation file. The test is referenced for verification only and is explicitly not modified.
