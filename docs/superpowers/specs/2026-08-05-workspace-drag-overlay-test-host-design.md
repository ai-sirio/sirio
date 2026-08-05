# Workspace drag overlay test host — design

## Context

Final CI for the separate stable-diff branch exposed a pre-existing deterministic crash in `TillerWorkspace`. The crash reproduces on the clean base commit `a9c2db056e1fc7010d288afd0e99923bf03022c3`, independently of the stable-diff implementation.

## Root cause

`NSApp` imports as `NSApplication!`. Evaluating `NSApp.currentEvent?.type` therefore implicitly unwraps a nil `NSApp` before optional chaining can evaluate `currentEvent`. A test host without an application instance crashes instead of treating the event as unavailable.

## Approved architecture

Change only `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift` so the event lookup is safe:

```swift
NSApp?.currentEvent?.type
```

When the application or current event is missing, the overlay has no accepted hover event and `hitTest` returns `nil`. The overlay therefore does not swallow clicks in a test host or any equivalent environment without an available application event.

`@MainActor` remains unchanged. AppKit remains owned by `TillerWorkspace`; no package-boundary change is introduced.

## Scope and non-goals

The implementation scope is exactly one file:

- `Packages/TillerWorkspace/Sources/TillerWorkspace/WorkspaceDragOverlay.swift`

Explicitly out of scope:

- Bootstrapping `NSApplication.shared` or creating a global application instance in tests.
- Introducing an event-provider abstraction.
- Changes to the stable-diff implementation.
- Unrelated cleanup or refactoring.

## Verification

`WorkspaceDragOverlayTests.theOverlayNeverSwallowsAClick` is deterministic RED before the fix and GREEN after it. Verification must include:

1. Run the focused test before and after the one-file change to establish the regression and its fix.
2. Run the full `TillerWorkspace` test suite.
3. Run `Scripts/ci.sh` and require the output `CI OK`.

## Acceptance criteria

- The clean-base crash is explained by the implicitly unwrapped `NSApp` access.
- `WorkspaceDragOverlay.swift` safely reads `NSApp?.currentEvent?.type`.
- Missing `NSApp` or current event produces no accepted hover event, and `hitTest` returns `nil`.
- `@MainActor` and the `TillerWorkspace` AppKit package boundary are preserved.
- No `NSApplication.shared` test bootstrap, event-provider abstraction, stable-diff change, or unrelated cleanup is added.
- Only the approved one-file implementation scope is changed.
- The focused test is GREEN, the full `TillerWorkspace` tests pass, and `Scripts/ci.sh` prints `CI OK`.
