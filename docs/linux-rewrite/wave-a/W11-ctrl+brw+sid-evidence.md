# W11-ctrl+brw+sid — evidence log

## F-CTRL-NOTIFY-03 — ledger line 442, currently FAILED — defective

**Drove:** Wayland lane (`wavea-W11-ctrl+brw+sid`). Started `dbus-monitor --session
"interface='org.freedesktop.Notifications',member='Notify'"` in the background, then sent
`ctl notification.create title=W11NOTIFYTEST body=W11NOTIFYBODY` over the control socket.

**Observed:** the control reply was `{"id":"drive","ok":true,"result":{}}` (unchanged from
prior evidence — `create` still reports success unconditionally). But this time the
`dbus-monitor` capture, running concurrently, recorded a real `Notify` method call:

```
method call ... path=/org/freedesktop/Notifications; interface=org.freedesktop.Notifications; member=Notify
   string "Tiller"
   ...
   string "W11NOTIFYTEST"
   string "W11NOTIFYBODY"
```

This is the exact conjunct the ledger recorded as false ("the posting conjunct never happens").
It now happens: `notification.create` reaches the D-Bus `Notify` call with the title/body I sent,
confirming commit a5d09d7 wired `record_notification` to `notification_poster` as triage
described.

**Claim:** exercised-working (reclassify confirmed — the defect described in the ledger no
longer reproduces; live D-Bus capture with a positive discriminator (my own title/body strings)
shows the posting conjunct now firing).

Captures: `reference/linux-progress/wavea-W11-ctrl+brw+sid/notify03/01-baseline.png`,
`02-after-create.png` (chrome only — no visible desktop-notification UI in this headless
compositor, D-Bus capture is the load-bearing evidence).
