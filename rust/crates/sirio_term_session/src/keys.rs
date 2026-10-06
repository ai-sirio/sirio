use libghostty_vt::{Terminal, key};

/// Encode the named keys accepted by `panel.key` against the session's current
/// terminal modes.
pub(super) fn encode(
    terminal: &Terminal<'_, '_>,
    encoder: &mut key::Encoder<'_>,
    name: &str,
) -> Result<Vec<u8>, String> {
    let key_value = match name {
        "enter" | "return" => key::Key::Enter,
        "tab" => key::Key::Tab,
        "escape" => key::Key::Escape,
        "backspace" => key::Key::Backspace,
        "delete" => key::Key::Delete,
        "up" => key::Key::ArrowUp,
        "down" => key::Key::ArrowDown,
        "left" => key::Key::ArrowLeft,
        "right" => key::Key::ArrowRight,
        _ => return Err(name.to_string()),
    };

    let mut event = key::Event::new().map_err(|error| error.to_string())?;
    event
        .set_action(key::Action::Press)
        .set_key(key_value)
        .set_mods(key::Mods::empty())
        .set_consumed_mods(key::Mods::empty())
        .set_utf8(None::<String>);

    encoder
        .set_options_from_terminal(terminal)
        .set_macos_option_as_alt(key::OptionAsAlt::True);
    let mut bytes = Vec::new();
    encoder
        .encode_to_vec(&event, &mut bytes)
        .map_err(|error| error.to_string())?;
    Ok(bytes)
}
