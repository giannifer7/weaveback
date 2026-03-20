// src/events.rs — generated
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginEvent {
    PluginLoad,
    PluginUnload,
    ConfigChange,
}

impl fmt::Display for PluginEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PluginEvent::PluginLoad => write!(f, "plugin.load"),
            PluginEvent::PluginUnload => write!(f, "plugin.unload"),
            PluginEvent::ConfigChange => write!(f, "config.change"),
        }
    }
}
