#!/bin/bash
# F-PRJ-14 repro recipe: Avatar tab's "Choose PNG..." picker, driven through
# the SAME portal fix as F-PRJ-18 (see f-prj-18-repro-recipe.sh for the
# XDG_CURRENT_DESKTOP root-cause writeup -- it applies identically here,
# since both pickers go through the exact same GPUI `cx.prompt_for_paths`
# API, which both resolve to org.freedesktop.portal.FileChooser.OpenFile
# under the hood -- see rust/vendor/gpui_linux/src/linux/platform.rs:396,
# `prompt_for_paths`, and rust/crates/tiller_ui/src/project_identity.rs:593,
# `choose_local_png`).
#
# With the portal already fixed (XDG_CURRENT_DESKTOP=GNOME in the router's
# own exec environment -- confirmed present in the running Tiller process's
# own /proc/<pid>/environ too, so this is not an env-mismatch issue), a
# `dbus-monitor --session` trace around the "Choose PNG..." click shows the
# full, successful call chain:
#
#   method call ... member=OpenFile
#     (app, sender :1.19 -> org.freedesktop.portal.Desktop, interface
#      org.freedesktop.portal.FileChooser)
#   method call ... member=OpenFile
#     (router, sender :1.27 -> :1.29 [the gtk backend], interface
#      org.freedesktop.impl.portal.FileChooser)
#   method call ... member=Change (destination ca.desrt.dconf.Writer.user)
#     (gtk backend persisting the dialog's remembered window geometry)
#   -> a real GTK "Open File" window appears, titled "Open File", with the
#      accept button reading "Choose a PNG" (the app's own
#      `prompt: Some("Choose a PNG".into())`).
#
# One drive-tooling caveat worth recording: an early attempt clicking the
# button's screen-space center, (161, 271), produced ZERO D-Bus traffic on
# two separate tries (3s and 5s waits, dbus-monitor listening the whole
# time) -- looking like the button was inert. A different set of
# coordinates a few pixels off ((100, 271), (100, 261), (250, 271)) worked
# immediately, and a positive control in the same sub-panel (typing into the
# neighbouring "GitHub user or repository" text field) confirmed general
# click/keyboard delivery was fine throughout -- so the (161, 271) misses
# were a virtual-pointer/hit-test precision artifact of this drive
# environment, not a defect in the app: once a click actually lands on the
# element, the picker fires every time. Do not conclude "button is dead"
# from one failed click in this environment; corroborate with a
# `dbus-monitor` trace and/or a nearby-coordinate retry before concluding
# anything is broken.
#
# From the open "Open File" dialog: click the search icon, type the target
# PNG's bare filename (search is scoped to whatever folder you last browsed
# into -- browse to the parent folder first via the sidebar's "/" entry and
# double-click into it, same as the F-PRJ-18 recipe, if the file will not be
# in "Recenti"), select the single match (GTK renders a live thumbnail
# preview of the actual PNG bytes, confirming it opened real file content,
# not just matched a name), click the accept button.
#
# Confirms end-to-end: the app's own Avatar tab immediately shows a
# swatch preview of the chosen PNG and a "Current: <filename>.png" label
# (from `apply_local_png` in project_identity.rs, which re-reads the file,
# checks the PNG signature and the MAX_AVATAR_PNG_BYTES size cap before
# committing) -- not just a path string landing in a text field.
