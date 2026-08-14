#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <fcntl.h>
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

int main(int argc, char **argv) {
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

    char line[128];
    while (fgets(line, sizeof(line), commands) != NULL) {
        char operation[8] = {0};
        unsigned id = 0, x = 0, y = 0, width = 0, height = 0;
        if (sscanf(line, "%7s %u %u %u %u %u", operation, &id, &x, &y, &width, &height) != 6 ||
            (strcmp(operation, "move") != 0 && strcmp(operation, "click") != 0) ||
            width == 0 || height == 0) {
            fprintf(stderr, "invalid pointer command: %s", line);
            continue;
        }
        move_pointer(pointer, x, y, width, height);
        wl_display_flush(display);
        if (strcmp(operation, "click") == 0) {
            sleep_ms(25);
            zwlr_virtual_pointer_v1_button(pointer, milliseconds(), 0x110,
                                            WL_POINTER_BUTTON_STATE_PRESSED);
            zwlr_virtual_pointer_v1_frame(pointer);
            wl_display_flush(display);
            sleep_ms(25);
            zwlr_virtual_pointer_v1_button(pointer, milliseconds(), 0x110,
                                            WL_POINTER_BUTTON_STATE_RELEASED);
            zwlr_virtual_pointer_v1_frame(pointer);
            wl_display_flush(display);
        }
        acknowledge(ack_path, id);
    }
    zwlr_virtual_pointer_v1_destroy(pointer);
    wl_display_disconnect(display);
    return 0;
}
