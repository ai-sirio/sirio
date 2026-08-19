/* Is an X11 unmap issued on GDK's connection visible to the server before
 * something flushes it?
 *
 * This is the premise the F-BRW tab-close fix rests on, and it cannot be
 * observed from inside the Rust process: every Rust-level assertion about
 * `set_visible(false)` passes whether or not the request ever left the client.
 *
 * wry 0.56.1 (`webkitgtk/mod.rs`) unmaps a child webview with a bare
 * `XUnmapWindow` on the display it got from `gdk_x11_display_get_xdisplay`,
 * and destroys it with a bare `XDestroyWindow` on the same one. Xlib appends
 * both to that connection's output buffer and returns. So the question that
 * decides whether the fix is a cure or a coincidence is: who drains it?
 *
 * The program answers it with two connections. One is GDK's, and does exactly
 * what wry does. The other is an independent client asking the server what it
 * actually believes — the same thing `xwininfo` reports, and the same thing the
 * user sees, since a window the server still holds as `IsViewable` is a window
 * still on screen.
 *
 * Prints three facts for the caller to assert on:
 *
 *   control=      the child, freshly mapped and synced — the positive control
 *   after_unmap=  the server's view right after the unmap, with nothing flushed
 *   after_flush=  the server's view after `flush_native_window_ops`'s recipe
 *
 * `after_unmap=IsViewable` is the bug in one line. If it ever reads
 * `IsUnmapped`, Xlib is delivering eagerly on this platform, the diagnosis
 * behind the fix is wrong, and the caller must fail rather than pass.
 */
#include <X11/Xlib.h>
#include <gdk/gdkx.h>
#include <gtk/gtk.h>
#include <stdio.h>

static int probe_failed = 0;

/* Xlib's default error handler calls exit(). A BadWindow on the probe
 * connection is a result we want to report, not a way to die. */
static int swallow_error(Display *display, XErrorEvent *error) {
    (void)display;
    (void)error;
    probe_failed = 1;
    return 0;
}

static const char *map_state_name(int state) {
    switch (state) {
        case IsUnmapped:
            return "IsUnmapped";
        case IsUnviewable:
            return "IsUnviewable";
        case IsViewable:
            return "IsViewable";
        default:
            return "unknown";
    }
}

/* Asks the *server*, over a connection that has nothing to do with GDK's, so
 * the answer cannot be an artefact of our own client-side bookkeeping. */
static const char *server_view_of(Display *probe, Window window) {
    XWindowAttributes attrs;
    probe_failed = 0;
    if (!XGetWindowAttributes(probe, window, &attrs) || probe_failed) {
        return "gone";
    }
    return map_state_name(attrs.map_state);
}

int main(void) {
    if (!gtk_init_check(NULL, NULL)) {
        fprintf(stderr, "gtk_init_check failed — no usable display\n");
        return 2;
    }

    GdkDisplay *gdk_display = gdk_display_get_default();
    if (gdk_display == NULL || !GDK_IS_X11_DISPLAY(gdk_display)) {
        fprintf(stderr, "the default GdkDisplay is not an X11 one\n");
        return 2;
    }
    Display *dpy = gdk_x11_display_get_xdisplay(GDK_X11_DISPLAY(gdk_display));

    int screen = DefaultScreen(dpy);
    Window root = RootWindow(dpy, screen);
    Window parent =
        XCreateSimpleWindow(dpy, root, 0, 0, 400, 300, 0, 0, BlackPixel(dpy, screen));
    XMapWindow(dpy, parent);
    /* A child of a mapped toplevel, which is the shape wry builds: `IsViewable`
     * means this window *and* every ancestor is mapped. */
    Window child =
        XCreateSimpleWindow(dpy, parent, 0, 0, 200, 150, 0, 0, WhitePixel(dpy, screen));
    XMapWindow(dpy, child);
    /* Round-trip, so the buffer is empty and the server's view is settled
     * before the measurement starts. Everything after this point that reaches
     * the server does so because something flushed it. */
    XSync(dpy, False);

    XSetErrorHandler(swallow_error);
    Display *probe = XOpenDisplay(NULL);
    if (probe == NULL) {
        fprintf(stderr, "could not open a second connection\n");
        return 2;
    }

    printf("child=0x%lx\n", (unsigned long)child);
    printf("control=%s\n", server_view_of(probe, child));

    /* What wry's `set_visible(false)` does, on the connection it does it on,
     * and nothing else. */
    XUnmapWindow(dpy, child);
    printf("after_unmap=%s\n", server_view_of(probe, child));

    /* The recipe from `flush_native_window_ops` in
     * rust/crates/tiller_ui/src/browser.rs — `gtk::main_iteration_do` /
     * `gtk::events_pending` / `gdk::Display::flush` are these three functions.
     * Kept in the same order and the same two rounds. */
    for (int round = 0; round < 2; round++) {
        while (gtk_events_pending()) {
            gtk_main_iteration_do(FALSE);
        }
        gdk_display_flush(gdk_display);
    }
    printf("after_flush=%s\n", server_view_of(probe, child));

    XCloseDisplay(probe);
    return 0;
}
