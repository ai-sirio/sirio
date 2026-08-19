#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
#include <poll.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <wayland-client.h>
#include "wlr-virtual-pointer-client-protocol.h"

struct state {
    struct wl_seat *seat;
    struct zwlr_virtual_pointer_manager_v1 *manager;
};

static void registry_global(void *data, struct wl_registry *registry,
                            uint32_t name, const char *interface, uint32_t version) {
    struct state *state = data;
    if (strcmp(interface, wl_seat_interface.name) == 0) {
        state->seat = wl_registry_bind(registry, name, &wl_seat_interface,
                                       version < 8 ? version : 8);
    } else if (strcmp(interface, zwlr_virtual_pointer_manager_v1_interface.name) == 0) {
        state->manager = wl_registry_bind(registry, name,
            &zwlr_virtual_pointer_manager_v1_interface, version < 2 ? version : 2);
    }
}

static void registry_global_remove(void *data, struct wl_registry *registry, uint32_t name) {
    (void)data;
    (void)registry;
    (void)name;
}

static const struct wl_registry_listener registry_listener = {
    .global = registry_global,
    .global_remove = registry_global_remove,
};

static uint32_t milliseconds(void) {
    struct timespec now;
    clock_gettime(CLOCK_MONOTONIC, &now);
    return (uint32_t)(now.tv_sec * 1000ULL + now.tv_nsec / 1000000ULL);
}

static void sleep_ms(unsigned milliseconds) {
    struct timespec duration = {
        .tv_sec = milliseconds / 1000,
        .tv_nsec = (long)(milliseconds % 1000) * 1000000L,
    };
    nanosleep(&duration, NULL);
}

static void acknowledge(const char *ack_path, unsigned id) {
    FILE *ack = fopen(ack_path, "a");
    if (ack == NULL) {
        fprintf(stderr, "fopen %s: %s\n", ack_path, strerror(errno));
        return;
    }
    fprintf(ack, "%u\n", id);
    fclose(ack);
}

static void move_pointer(struct zwlr_virtual_pointer_v1 *pointer,
                         unsigned x, unsigned y, unsigned width, unsigned height) {
    zwlr_virtual_pointer_v1_motion_absolute(pointer, milliseconds(), x, y, width, height);
    zwlr_virtual_pointer_v1_frame(pointer);
}

/* Linux input-event-codes.h button values; hardcoded rather than pulling in the header, matching
 * how BTN_LEFT (0x110) was already spelled out below before this file grew a second button. */
#define BTN_LEFT_CODE 0x110u
#define BTN_RIGHT_CODE 0x111u

static void press_button(struct zwlr_virtual_pointer_v1 *pointer, uint32_t button) {
    zwlr_virtual_pointer_v1_button(pointer, milliseconds(), button, WL_POINTER_BUTTON_STATE_PRESSED);
    zwlr_virtual_pointer_v1_frame(pointer);
}

static void release_button(struct zwlr_virtual_pointer_v1 *pointer, uint32_t button) {
    zwlr_virtual_pointer_v1_button(pointer, milliseconds(), button, WL_POINTER_BUTTON_STATE_RELEASED);
    zwlr_virtual_pointer_v1_frame(pointer);
}

/* One wheel "click" conventionally reports 15 libinput units per notch (matches a real mouse
 * wheel / what wlroots' own pointer emulation uses); `steps` may be negative for the opposite
 * direction. axis_source/axis_discrete are sent alongside axis so a client reading discrete wheel
 * steps (rather than raw continuous scroll) sees the same event a physical wheel would produce. */
static void scroll_axis(struct zwlr_virtual_pointer_v1 *pointer, int32_t steps) {
    uint32_t time = milliseconds();
    wl_fixed_t value = wl_fixed_from_int(steps * 15);
    zwlr_virtual_pointer_v1_axis_source(pointer, WL_POINTER_AXIS_SOURCE_WHEEL);
    zwlr_virtual_pointer_v1_axis(pointer, time, WL_POINTER_AXIS_VERTICAL_SCROLL, value);
    zwlr_virtual_pointer_v1_axis_discrete(pointer, time, WL_POINTER_AXIS_VERTICAL_SCROLL, value, steps);
    zwlr_virtual_pointer_v1_frame(pointer);
}

int main(int argc, char **argv) {
    /* Commands arrive one per line on the fifo: "<op> <id> <x> <y> <width> <height> [<extra>]"
     * where op is move/click/rightclick/down/up/scroll and extra is scroll's signed step count.
     * See wayland-drive.sh's pointer_command() for the writer side. */
    if (argc != 4) {
        fprintf(stderr, "usage: %s <ready-file> <command-fifo> <ack-file>\n", argv[0]);
        return 2;
    }
    const char *ready_path = argv[1];
    const char *fifo_path = argv[2];
    const char *ack_path = argv[3];

    struct wl_display *display = wl_display_connect(NULL);
    if (display == NULL) {
        fprintf(stderr, "wl_display_connect: %s\n", strerror(errno));
        return 3;
    }
    struct state state = {0};
    struct wl_registry *registry = wl_display_get_registry(display);
    wl_registry_add_listener(registry, &registry_listener, &state);
    if (wl_display_roundtrip(display) < 0 || state.seat == NULL || state.manager == NULL) {
        fprintf(stderr, "missing wl_seat or zwlr_virtual_pointer_manager_v1\n");
        return 4;
    }
    struct zwlr_virtual_pointer_v1 *pointer =
        zwlr_virtual_pointer_manager_v1_create_virtual_pointer(state.manager, state.seat);
    if (wl_display_roundtrip(display) < 0) {
        fprintf(stderr, "could not create virtual pointer\n");
        return 5;
    }

    FILE *ready = fopen(ready_path, "w");
    if (ready == NULL) {
        fprintf(stderr, "fopen %s: %s\n", ready_path, strerror(errno));
        return 6;
    }
    fputs("ready\n", ready);
    fclose(ready);

    int fifo_fd = open(fifo_path, O_RDWR);
    if (fifo_fd < 0) {
        fprintf(stderr, "open %s: %s\n", fifo_path, strerror(errno));
        return 7;
    }
    FILE *commands = fdopen(fifo_fd, "r");
    if (commands == NULL) {
        fprintf(stderr, "fdopen %s: %s\n", fifo_path, strerror(errno));
        return 8;
    }
    /* Unbuffered so that poll() below is telling the truth. A buffered stream can
     * pull two commands out of the fifo in one read(), leaving the second sitting
     * in FILE's buffer where poll sees nothing readable — the loop would then
     * block waiting for input that has, in fact, already arrived. */
    setvbuf(commands, NULL, _IONBF, 0);

    /* Watch the compositor's socket alongside the fifo.
     *
     * Without this the loop is structurally blind to sway dying. The fifo is
     * opened O_RDWR at line 138 — deliberately, so that a writer closing does not
     * deliver EOF and end the session — which makes this process its own writer,
     * so fgets() blocks forever no matter what happens elsewhere. And nothing in
     * the body touches the display except to flush it, which fails silently.
     *
     * The result was a process that outlived its compositor indefinitely. Measured
     * 2026-08-19 on this machine: 184 of these alive at once with a single sway
     * left between them, the oldest 25 hours old, one per lane ever driven. The
     * label-scoped reap in wayland-drive.sh only helps a label that gets reused;
     * most lane labels are used once, so the leak has to be closed here.
     *
     * wl_display_dispatch() is safe to call unprepared here because this program
     * is single-threaded and only ever calls it when poll() has already said the
     * fd is readable, so it cannot block. */
    int wl_fd = wl_display_get_fd(display);
    int compositor_gone = 0;
    char line[128];
    for (;;) {
        struct pollfd fds[2] = {
            { .fd = fifo_fd, .events = POLLIN, .revents = 0 },
            { .fd = wl_fd,   .events = POLLIN, .revents = 0 },
        };
        if (poll(fds, 2, -1) < 0) {
            if (errno == EINTR) {
                continue;
            }
            fprintf(stderr, "poll: %s\n", strerror(errno));
            break;
        }
        /* Both endings are real and neither implies the other: a compositor that
         * exits cleanly closes the socket (POLLHUP), while one that is killed
         * mid-message leaves a protocol error that only dispatch reports. */
        if (fds[1].revents & (POLLHUP | POLLERR)) {
            fprintf(stderr, "compositor gone: hangup on the wayland socket\n");
            compositor_gone = 1;
            break;
        }
        if ((fds[1].revents & POLLIN) && wl_display_dispatch(display) < 0) {
            fprintf(stderr, "compositor gone: %s\n", strerror(errno));
            compositor_gone = 1;
            break;
        }
        if (!(fds[0].revents & POLLIN)) {
            continue;
        }
        if (fgets(line, sizeof(line), commands) == NULL) {
            break;
        }
        /* op id x y width height [extra] — extra is signed (scroll steps); every other op
         * ignores it. %d tolerates a leading '-' that %u would reject. */
        char operation[16] = {0};
        unsigned id = 0, x = 0, y = 0, width = 0, height = 0;
        int extra = 0;
        int n = sscanf(line, "%15s %u %u %u %u %u %d", operation, &id, &x, &y, &width, &height, &extra);
        int known = strcmp(operation, "move") == 0 || strcmp(operation, "click") == 0 ||
                    strcmp(operation, "rightclick") == 0 || strcmp(operation, "down") == 0 ||
                    strcmp(operation, "up") == 0 || strcmp(operation, "scroll") == 0;
        if (n < 6 || !known || width == 0 || height == 0 ||
            (strcmp(operation, "scroll") == 0 && n != 7)) {
            fprintf(stderr, "invalid pointer command: %s", line);
            continue;
        }
        move_pointer(pointer, x, y, width, height);
        wl_display_flush(display);
        if (strcmp(operation, "click") == 0) {
            sleep_ms(25);
            press_button(pointer, BTN_LEFT_CODE);
            wl_display_flush(display);
            sleep_ms(25);
            release_button(pointer, BTN_LEFT_CODE);
            wl_display_flush(display);
        } else if (strcmp(operation, "rightclick") == 0) {
            sleep_ms(25);
            press_button(pointer, BTN_RIGHT_CODE);
            wl_display_flush(display);
            sleep_ms(25);
            release_button(pointer, BTN_RIGHT_CODE);
            wl_display_flush(display);
        } else if (strcmp(operation, "down") == 0) {
            sleep_ms(25);
            press_button(pointer, BTN_LEFT_CODE);
            wl_display_flush(display);
        } else if (strcmp(operation, "up") == 0) {
            sleep_ms(25);
            release_button(pointer, BTN_LEFT_CODE);
            wl_display_flush(display);
        } else if (strcmp(operation, "scroll") == 0) {
            sleep_ms(25);
            scroll_axis(pointer, extra);
            wl_display_flush(display);
        }
        acknowledge(ack_path, id);
    }
    /* A dead compositor exits non-zero so the log says which ending happened.
     * Nothing gates on it — wayland-drive.sh only ever `kill -0`s this pid — but a
     * silent 0 is what let the orphans above look like a healthy idle process. */
    if (compositor_gone) {
        wl_display_disconnect(display);
        return 9;
    }
    zwlr_virtual_pointer_v1_destroy(pointer);
    wl_display_disconnect(display);
    return 0;
}
