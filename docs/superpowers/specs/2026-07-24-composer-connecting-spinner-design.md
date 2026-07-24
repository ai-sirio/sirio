# Composer connecting spinner

## Goal

Show a loading spinner in place of the composer's circular send button while a
chat is starting up — the `.connecting` state only. Every other state keeps
today's behavior.

## Context

`App/Chat/ChatComposerView.swift` renders the send/stop control at the end of
`controlBar`:

```swift
if isPrompting { stopButton } else { sendButton }
```

`ChatController.ChatState` (`App/Chat/ChatController.swift`) is
`idle, connecting, ready, prompting, needsAuth, disconnected(message:)`.

Today during `.connecting` the send button stays visible but disabled
(`canSend` is false because `canInteract` requires `.ready` or `.prompting`),
so it shows a greyed-out arrow with no indication that the agent is starting.

## Design

Turn the binary control into a three-way switch:

```swift
if isConnecting { loadingButton }
else if isPrompting { stopButton }
else { sendButton }
```

### New members (in `ChatComposerView`)

- `isConnecting: Bool { controller.state == .connecting }` — next to the
  existing `isPrompting` computed property.
- `loadingButton: some View` — an indeterminate `ProgressView()` scaled down,
  centered inside a 26x26 circle filled with `.quaternary`, matching the visual
  footprint of `sendButton` / `stopButton`. Not a `Button` — it is
  non-interactive.

### Behavior

- Spinner shows **only** in `.connecting`.
- `.prompting` is unchanged — `stopButton` as today.
- Other non-ready states (`.idle`, `.disconnected`, `.needsAuth`) keep the
  disabled `sendButton` (already handled by `canSend == false`).

### Notes

- macOS circular `ProgressView` already respects the system's reduce-motion
  setting, so no manual `reduceMotion` handling is needed (unlike the custom
  `contextUsageIndicator` ring, which animates a stroke and gates on
  `reduceMotion` explicitly).
- The `.quaternary` circle without accent color reads as non-actionable; the
  spinner itself is the signal, so no extra disabled styling is required.

## Testing

Pure view change, no isolatable logic beyond the state flag. Verification:

1. `Scripts/ci.sh` prints `CI OK` (build passes).
2. Manual smoke: open a chat -> spinner shows while the agent starts ->
   turns into the arrow send button once `.ready`.
