/// Linux-resolved settings that do not depend on a UI toolkit or storage
/// backend. Persisted values enter through `from_values`; environment policy
/// is applied last so deployment can disable the control socket.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsPolicy {
    pub refresh_interval_secs: u64,
    pub control_socket_enabled: bool,
    pub resume_sessions: bool,
    pub auto_name_tabs: bool,
    pub translucent_sidebar: bool,
    pub session_retention_days: u32,
    pub mount_cap: u32,
    pub ui_font_size: f32,
    pub terminal_font_size: f32,
    pub sidebar_width: f32,
    pub right_panel_width: f32,
}

impl Default for SettingsPolicy {
    fn default() -> Self {
        Self {
            refresh_interval_secs: 300,
            control_socket_enabled: true,
            resume_sessions: true,
            auto_name_tabs: true,
            translucent_sidebar: true,
            session_retention_days: 30,
            mount_cap: 8,
            ui_font_size: 13.0,
            terminal_font_size: 13.0,
            sidebar_width: 240.0,
            right_panel_width: 320.0,
        }
    }
}

impl SettingsPolicy {
    pub fn from_values(mut self) -> Self {
        self.refresh_interval_secs = self.refresh_interval_secs.clamp(60, 3_600);
        self.session_retention_days = self.session_retention_days.clamp(1, 365);
        self.mount_cap = self.mount_cap.clamp(1, 64);
        self.ui_font_size = self.ui_font_size.clamp(10.0, 20.0);
        self.terminal_font_size = self.terminal_font_size.clamp(9.0, 24.0);
        self.sidebar_width = self.sidebar_width.clamp(160.0, 480.0);
        self.right_panel_width = self.right_panel_width.clamp(220.0, 640.0);
        self
    }

    pub fn with_environment_override(mut self) -> Self {
        if let Some(value) = std::env::var_os("TILLER_SOCKET_ENABLE") {
            match value.to_string_lossy().trim().to_ascii_lowercase().as_str() {
                "0" | "false" | "no" | "off" => self.control_socket_enabled = false,
                "1" | "true" | "yes" | "on" => self.control_socket_enabled = true,
                _ => {}
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_are_clamped_to_safe_ranges() {
        let settings = SettingsPolicy {
            refresh_interval_secs: 1,
            session_retention_days: 0,
            mount_cap: 100,
            ui_font_size: 100.0,
            terminal_font_size: 1.0,
            sidebar_width: 1.0,
            right_panel_width: 1_000.0,
            ..Default::default()
        }
        .from_values();
        assert_eq!(settings.refresh_interval_secs, 60);
        assert_eq!(settings.session_retention_days, 1);
        assert_eq!(settings.mount_cap, 64);
        assert_eq!(settings.ui_font_size, 20.0);
        assert_eq!(settings.terminal_font_size, 9.0);
        assert_eq!(settings.sidebar_width, 160.0);
        assert_eq!(settings.right_panel_width, 640.0);
    }

    #[test]
    fn socket_environment_override_is_explicit_and_invalid_values_do_not_guess() {
        let previous = std::env::var_os("TILLER_SOCKET_ENABLE");
        unsafe { std::env::set_var("TILLER_SOCKET_ENABLE", "off") };
        assert!(
            !SettingsPolicy::default()
                .with_environment_override()
                .control_socket_enabled
        );
        unsafe { std::env::set_var("TILLER_SOCKET_ENABLE", "unexpected") };
        assert!(
            SettingsPolicy::default()
                .with_environment_override()
                .control_socket_enabled
        );
        match previous {
            Some(value) => unsafe { std::env::set_var("TILLER_SOCKET_ENABLE", value) },
            None => unsafe { std::env::remove_var("TILLER_SOCKET_ENABLE") },
        }
    }
}
