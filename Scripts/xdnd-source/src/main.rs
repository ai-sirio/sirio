//! A real wl_data_device_manager XDND drag SOURCE.
//!
//! Everything on Tiller's own side of a file drop was already traced correct against the
//! vendored pinned Zed gpui_linux checkout (F-CORE-FILE-03A): `wl_data_device` `Enter` reads
//! `text/uri-list` through one pipe, `Drop` carries a position, `window.rs` stores the parsed
//! paths, and the terminal's `on_drop::<gpui::ExternalPaths>` inserts them into the shell. What
//! was missing was a real compositor-delivered XDND drag to drive that path end to end, instead
//! of GPUI's own internal simulated-drag test harness.
//!
//! This binary is that missing half: a minimal Wayland client that becomes the XDND drag
//! *source*. It creates a tiny `zwlr_layer_shell_v1` overlay surface (deliberately NOT an
//! `xdg_toplevel` — a layer-shell surface is not part of sway's tiled layout, so it cannot
//! disturb Tiller's own window geometry or the coordinates `wayland-drive.sh` already uses), waits
//! for `wayland-drive.sh`'s persistent virtual pointer to press a button over that surface (the
//! same virtual-pointer client already proven for click/rightclick/drag in P124/P130), and on that
//! real `wl_pointer.button` press — using ITS serial, per protocol — calls
//! `wl_data_device.start_drag` offering `text/uri-list`. The virtual pointer then walks to the
//! drop target and releases; the compositor (sway/wlroots) delivers `wl_data_device.enter` /
//! `motion` / `drop` to whatever surface is under the pointer, i.e. Tiller's own window, exactly
//! as a real file manager's drag would.
//!
//! Protocol notes:
//!   - `text/uri-list` (RFC 2483, CRLF-separated) is the one MIME type Tiller's `Enter` handler
//!     reads (`FILE_LIST_MIME_TYPE` in the vendored gpui_linux checkout).
//!   - The "slow-resolving provider" clause: on `text/uri-list` the whole list arrives through one
//!     pipe in one write, so there is no per-file resolution to be slow about. `--delay-ms` instead
//!     delays *when* that single write happens after the target requests it (the `send` event),
//!     which is the closest analogue this MIME type has. Whether that is the same hazard the macOS
//!     `NSItemProvider` clause names (per-item async resolution racing a UI timeout) is a judgment
//!     call for whoever reads the drive's report — say plainly that it is not proven equivalent.
//!
//! Usage: see `xdnd-source --help`, or `docs/linux-rewrite/WAYLAND-LANE.md`'s XDND section.

use std::ffi::CString;
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use wayland_client::protocol::{
    wl_buffer, wl_compositor, wl_data_device, wl_data_device_manager, wl_data_offer,
    wl_data_source, wl_pointer, wl_registry, wl_seat, wl_shm, wl_shm_pool, wl_surface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

const BTN_LEFT: u32 = 0x110;
const SURFACE_SIZE: i32 = 10;

struct Args {
    uris: Vec<String>,
    anchor_x: i32,
    anchor_y: i32,
    delay_ms: u64,
    timeout_secs: u64,
}

fn parse_args() -> Args {
    let mut uris = Vec::new();
    let mut anchor_x = 4;
    let mut anchor_y = 4;
    let mut delay_ms = 0u64;
    let mut timeout_secs = 15u64;
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--uri" => uris.push(it.next().expect("--uri needs a value")),
            "--file" => {
                let path = it.next().expect("--file needs a value");
                let abs = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone().into());
                uris.push(format!("file://{}", abs.display()));
            }
            "--anchor-x" => anchor_x = it.next().expect("--anchor-x needs a value").parse().unwrap(),
            "--anchor-y" => anchor_y = it.next().expect("--anchor-y needs a value").parse().unwrap(),
            "--delay-ms" => delay_ms = it.next().expect("--delay-ms needs a value").parse().unwrap(),
            "--timeout-secs" => {
                timeout_secs = it.next().expect("--timeout-secs needs a value").parse().unwrap()
            }
            "--help" | "-h" => {
                eprintln!(
                    "usage: xdnd-source --uri file:///abs/path [--uri file:///abs/other ...] \
                     [--anchor-x N] [--anchor-y N] [--delay-ms N] [--timeout-secs N]\n\
                     Prints READY once its offer surface is mapped, DRAG_STARTED once a real \
                     wl_pointer.button press over that surface triggered wl_data_device.start_drag, \
                     and FINISHED/CANCELLED with the compositor's outcome. Exit 0 on FINISHED."
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("FAIL: unknown argument {other}");
                std::process::exit(2);
            }
        }
    }
    if uris.is_empty() {
        eprintln!("FAIL: at least one --uri (or --file) is required");
        std::process::exit(2);
    }
    Args {
        uris,
        anchor_x,
        anchor_y,
        delay_ms,
        timeout_secs,
    }
}

struct State {
    args: Args,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    layer_shell: Option<zwlr_layer_shell_v1::ZwlrLayerShellV1>,
    data_device_manager: Option<wl_data_device_manager::WlDataDeviceManager>,
    seat: Option<wl_seat::WlSeat>,
    pointer: Option<wl_pointer::WlPointer>,
    data_device: Option<wl_data_device::WlDataDevice>,
    surface: Option<wl_surface::WlSurface>,
    layer_surface: Option<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1>,
    configured: bool,
    pointer_over_surface: bool,
    drag_started: bool,
    source: Option<wl_data_source::WlDataSource>,
    outcome: Option<bool>, // Some(true) = dnd_finished, Some(false) = cancelled
}

fn flush_println(s: &str) {
    println!("{s}");
    let _ = std::io::stdout().flush();
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            match interface.as_str() {
                "wl_compositor" => {
                    state.compositor = Some(registry.bind(name, version.min(4), qh, ()));
                }
                "wl_shm" => {
                    state.shm = Some(registry.bind(name, version.min(1), qh, ()));
                }
                "wl_seat" => {
                    state.seat = Some(registry.bind(name, version.min(7), qh, ()));
                }
                "wl_data_device_manager" => {
                    state.data_device_manager = Some(registry.bind(name, version.min(3), qh, ()));
                }
                "zwlr_layer_shell_v1" => {
                    state.layer_shell = Some(registry.bind(name, version.min(4), qh, ()));
                }
                _ => {}
            }
        }
    }
}

impl Dispatch<wl_compositor::WlCompositor, ()> for State {
    fn event(_: &mut Self, _: &wl_compositor::WlCompositor, _: wl_compositor::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(_: &mut Self, _: &wl_shm::WlShm, _: wl_shm::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for State {
    fn event(_: &mut Self, _: &wl_shm_pool::WlShmPool, _: wl_shm_pool::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(_: &mut Self, _: &wl_buffer::WlBuffer, _: wl_buffer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_surface::WlSurface, ()> for State {
    fn event(_: &mut Self, _: &wl_surface::WlSurface, _: wl_surface::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<zwlr_layer_shell_v1::ZwlrLayerShellV1, ()> for State {
    fn event(_: &mut Self, _: &zwlr_layer_shell_v1::ZwlrLayerShellV1, _: zwlr_layer_shell_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, ()> for State {
    fn event(
        state: &mut Self,
        layer_surface: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let zwlr_layer_surface_v1::Event::Configure { serial, width, height } = event {
            layer_surface.ack_configure(serial);
            if !state.configured {
                let w = if width == 0 { SURFACE_SIZE as u32 } else { width };
                let h = if height == 0 { SURFACE_SIZE as u32 } else { height };
                attach_solid_buffer(state, w as i32, h as i32, qh);
                state.configured = true;
            }
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let caps = match capabilities {
                WEnum::Value(c) => c,
                WEnum::Unknown(_) => return,
            };
            if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter { surface, .. } => {
                if let Some(ours) = &state.surface {
                    state.pointer_over_surface = surface.id() == ours.id();
                }
            }
            wl_pointer::Event::Leave { surface, .. } => {
                if let Some(ours) = &state.surface {
                    if surface.id() == ours.id() {
                        state.pointer_over_surface = false;
                    }
                }
            }
            wl_pointer::Event::Button { serial, button, state: button_state, .. } => {
                let pressed = matches!(button_state, WEnum::Value(wl_pointer::ButtonState::Pressed));
                if pressed && button == BTN_LEFT && state.pointer_over_surface && !state.drag_started {
                    start_drag(state, serial, qh);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_data_device_manager::WlDataDeviceManager, ()> for State {
    fn event(_: &mut Self, _: &wl_data_device_manager::WlDataDeviceManager, _: wl_data_device_manager::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_data_device::WlDataDevice, ()> for State {
    fn event(
        _: &mut Self,
        _: &wl_data_device::WlDataDevice,
        _: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // This client is a drag SOURCE, not a drop target, and must implement Dispatch for this
        // object only because it holds one (get_data_device); it deliberately does nothing with
        // any event on it.
        //
        // This is not idle caution: the pointer is still over THIS client's own tiny overlay
        // surface for the first instant of every drag (start_drag fires before any `move`), so the
        // compositor self-delivers a `data_offer` + `enter` here, on this same wl_data_device, as
        // a candidate drop target. An earlier version of this handler called `id.destroy()` on
        // that self-offer the moment it arrived — before the drag ever had a chance to move
        // anywhere — and wlroots reads an unaccepted, destroyed offer on the origin surface itself
        // as "no one will take this drag" and cancels it outright. Traced live: `wl_data_device`
        // events on this object jumped straight from `enter`/`motion` to `leave` +
        // `wl_data_source.cancelled`, tens of milliseconds after `start_drag`, before this
        // function's caller had even sent the first pointer `move` waypoint. Ignoring the self-
        // offer entirely — not accepting it, not destroying it, just leaving it alone — lets the
        // compositor keep the drag alive once the pointer actually leaves this surface.
    }

    wayland_client::event_created_child!(State, wl_data_device::WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (wl_data_offer::WlDataOffer, ()),
    ]);
}

impl Dispatch<wl_data_offer::WlDataOffer, ()> for State {
    fn event(_: &mut Self, _: &wl_data_offer::WlDataOffer, _: wl_data_offer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<wl_data_source::WlDataSource, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_data_source::WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Target { mime_type } => {
                flush_println(&format!("TARGET {}", mime_type.unwrap_or_default()));
            }
            wl_data_source::Event::Send { mime_type, fd } => {
                flush_println(&format!("SEND mime={mime_type} delay_ms={}", state.args.delay_ms));
                if state.args.delay_ms > 0 {
                    std::thread::sleep(Duration::from_millis(state.args.delay_ms));
                }
                // RFC 2483: one absolute URI per line, CRLF-terminated. Tiller's Enter handler
                // only ever splits on `.lines()`, which also accepts a bare LF, but CRLF is what a
                // real file manager (e.g. Nautilus, GTK's GtkFileChooser drag source) sends.
                let mut body = String::new();
                for uri in &state.args.uris {
                    body.push_str(uri);
                    body.push_str("\r\n");
                }
                // `fd` (an OwnedFd) closes on drop at the end of this match arm, which is what
                // signals EOF to the reader — File::from takes ownership, no manual fd juggling.
                let mut file = std::fs::File::from(fd);
                if let Err(err) = file.write_all(body.as_bytes()) {
                    eprintln!("FAIL: writing text/uri-list to the offer pipe: {err}");
                }
            }
            // `dnd_finished` and `cancelled` are each documented as the terminal event for a
            // drag — but wlroots, observed live in this exact trace, sends a trailing `cancelled`
            // as its own data_source teardown notice immediately AFTER a real `dnd_finished`. GPUI
            // itself (this checkout's `client.rs`, DataSourceKind::Drag) treats them as
            // interchangeable teardown signals for the same reason: whichever arrives FIRST is the
            // true outcome, and once decided, later events on this doomed source don't get to
            // overturn it. `.destroy()` here (which gpui's own handler also does) stops the
            // compositor from sending anything further on this object at all.
            wl_data_source::Event::Cancelled => {
                flush_println("CANCELLED");
                if state.outcome.is_none() {
                    state.outcome = Some(false);
                    if let Some(source) = state.source.take() {
                        source.destroy();
                    }
                }
            }
            wl_data_source::Event::DndDropPerformed => {
                flush_println("DROP_PERFORMED");
            }
            wl_data_source::Event::DndFinished => {
                flush_println("FINISHED");
                if state.outcome.is_none() {
                    state.outcome = Some(true);
                    if let Some(source) = state.source.take() {
                        source.destroy();
                    }
                }
            }
            wl_data_source::Event::Action { dnd_action } => {
                flush_println(&format!("ACTION {dnd_action:?}"));
            }
            _ => {}
        }
    }
}

fn start_drag(state: &mut State, serial: u32, qh: &QueueHandle<State>) {
    let Some(dm) = state.data_device_manager.as_ref() else { return };
    let Some(dd) = state.data_device.as_ref() else { return };
    let Some(origin) = state.surface.as_ref() else { return };
    let source = dm.create_data_source(qh, ());
    source.offer("text/uri-list".to_string());
    source.set_actions(wl_data_device_manager::DndAction::Copy);
    dd.start_drag(Some(&source), origin, None, serial);
    state.drag_started = true;
    flush_println("DRAG_STARTED");
    // Kept so the terminal Cancelled/DndFinished handler above can call `.destroy()` on it.
    state.source = Some(source);
}

fn attach_solid_buffer(state: &mut State, width: i32, height: i32, qh: &QueueHandle<State>) {
    let Some(shm) = state.shm.as_ref() else { return };
    let Some(surface) = state.surface.as_ref() else { return };
    let stride = width * 4;
    let size = (stride * height) as usize;
    let name = CString::new("xdnd-source-buf").unwrap();
    let raw_fd = unsafe { libc::memfd_create(name.as_ptr(), 0) };
    if raw_fd < 0 {
        eprintln!("FAIL: memfd_create: {}", std::io::Error::last_os_error());
        std::process::exit(4);
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    if unsafe { libc::ftruncate(fd.as_raw_fd(), size as i64) } != 0 {
        eprintln!("FAIL: ftruncate: {}", std::io::Error::last_os_error());
        std::process::exit(4);
    }
    // Solid amber square — a visible drag icon is not required by the protocol (icon is optional
    // in start_drag), but this pixel content is what makes the ORIGIN surface itself paintable and
    // therefore eligible for pointer focus/input in the first place.
    let mut pixels = vec![0u8; size];
    for chunk in pixels.chunks_exact_mut(4) {
        chunk.copy_from_slice(&[0x20u8, 0x80, 0xd0, 0xff]); // B, G, R, A (native-endian ARGB8888)
    }
    // `File::from(OwnedFd)` takes ownership with no duplication; writing through it and then
    // converting back with `OwnedFd::from(File)` hands the same fd on to wl_shm — the request
    // itself takes ownership again, so nothing here is left needing an explicit close.
    let mut file = std::fs::File::from(fd);
    if let Err(err) = file.write_all(&pixels).and_then(|_| file.flush()) {
        eprintln!("FAIL: writing the offer-surface buffer: {err}");
        std::process::exit(4);
    }
    // `.as_fd()` matches wayland-client's own examples (`examples/simple_window.rs`): the request
    // is encoded with this borrow, and `fd` (the owning `File`, converted back below) stays alive
    // until this function returns, well past that encode.
    use std::os::fd::AsFd;
    let fd = OwnedFd::from(file);
    let pool = shm.create_pool(fd.as_fd(), size as i32, qh, ());
    let buffer = pool.create_buffer(0, width, height, stride, wl_shm::Format::Argb8888, qh, ());
    surface.attach(Some(&buffer), 0, 0);
    surface.damage_buffer(0, 0, width, height);
    surface.commit();
    pool.destroy();
}

fn main() {
    let args = parse_args();
    let timeout_secs = args.timeout_secs;

    let conn = Connection::connect_to_env().unwrap_or_else(|err| {
        eprintln!("FAIL: connecting to Wayland (WAYLAND_DISPLAY unset or unreachable?): {err}");
        std::process::exit(3);
    });
    let mut event_queue = conn.new_event_queue::<State>();
    let qh = event_queue.handle();
    let display = conn.display();
    let _registry = display.get_registry(&qh, ());

    let mut state = State {
        args,
        compositor: None,
        shm: None,
        layer_shell: None,
        data_device_manager: None,
        seat: None,
        pointer: None,
        data_device: None,
        surface: None,
        layer_surface: None,
        configured: false,
        pointer_over_surface: false,
        drag_started: false,
        source: None,
        outcome: None,
    };

    // First roundtrip: collect the globals advertised by wl_registry.
    event_queue.roundtrip(&mut state).expect("initial roundtrip");

    for (have, what) in [
        (state.compositor.is_some(), "wl_compositor"),
        (state.shm.is_some(), "wl_shm"),
        (state.seat.is_some(), "wl_seat"),
        (state.data_device_manager.is_some(), "wl_data_device_manager"),
        (state.layer_shell.is_some(), "zwlr_layer_shell_v1"),
    ] {
        if !have {
            eprintln!("FAIL: compositor did not advertise {what} (nested sway too old, or wrong WAYLAND_DISPLAY?)");
            std::process::exit(3);
        }
    }

    let compositor = state.compositor.clone().unwrap();
    let surface = compositor.create_surface(&qh, ());
    state.surface = Some(surface.clone());

    let layer_shell = state.layer_shell.clone().unwrap();
    let layer_surface = layer_shell.get_layer_surface(
        &surface,
        None,
        zwlr_layer_shell_v1::Layer::Overlay,
        "xdnd-source".to_string(),
        &qh,
        (),
    );
    layer_surface.set_size(SURFACE_SIZE as u32, SURFACE_SIZE as u32);
    layer_surface.set_anchor(zwlr_layer_surface_v1::Anchor::Top | zwlr_layer_surface_v1::Anchor::Left);
    layer_surface.set_margin(state.args.anchor_y, 0, 0, state.args.anchor_x);
    layer_surface.set_keyboard_interactivity(zwlr_layer_surface_v1::KeyboardInteractivity::None);
    state.layer_surface = Some(layer_surface);
    surface.commit();

    // Second roundtrip: wl_seat.capabilities (-> get_pointer) and zwlr_layer_surface_v1.configure
    // (-> the surface gets a real buffer and becomes eligible for pointer focus).
    event_queue.roundtrip(&mut state).expect("bind roundtrip");
    let mut waited = 0;
    while (!state.configured || state.pointer.is_none()) && waited < 100 {
        event_queue.roundtrip(&mut state).expect("configure roundtrip");
        std::thread::sleep(Duration::from_millis(50));
        waited += 1;
    }
    if !state.configured {
        eprintln!("FAIL: layer surface never received a configure event");
        std::process::exit(3);
    }
    if state.pointer.is_none() {
        eprintln!("FAIL: wl_seat never advertised pointer capability");
        std::process::exit(3);
    }

    let dm = state.data_device_manager.clone().unwrap();
    let seat = state.seat.clone().unwrap();
    state.data_device = Some(dm.get_data_device(&seat, &qh, ()));

    flush_println("READY");

    // Safety net: `blocking_dispatch` below can block on read() indefinitely if the compositor
    // never sends another event (e.g. the driver script never pressed a button, so no drag ever
    // started, so no cancelled/dnd_finished will ever arrive). A flag checked only *between*
    // dispatches would never be seen in that case, so this thread exits the process itself.
    let done = Arc::new(AtomicBool::new(false));
    {
        let done = done.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(timeout_secs));
            if !done.load(Ordering::SeqCst) {
                eprintln!("FAIL: timed out after {timeout_secs}s waiting for dnd_finished/cancelled");
                std::process::exit(6);
            }
        });
    }

    loop {
        event_queue.blocking_dispatch(&mut state).unwrap_or_else(|err| {
            eprintln!("FAIL: event dispatch: {err}");
            std::process::exit(5);
        });
        if let Some(ok) = state.outcome {
            done.store(true, Ordering::SeqCst);
            std::process::exit(if ok { 0 } else { 1 });
        }
    }
}
