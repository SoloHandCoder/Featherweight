//! Core data types shared across the engine, journal and catalog.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Hive {
    Hklm,
    Hkcu,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum RegData {
    Dword(u32),
    Qword(u64),
    String(String),
    /// REG_EXPAND_SZ; stored verbatim, never expanded.
    ExpandString(String),
    Binary(Vec<u8>),
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StartType {
    Disabled,
    Manual,
    Automatic,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    Safe,
    Advanced,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Remove,
    Background,
    Gaming,
    Cleanup,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    Everyday,
    Gaming,
}

/// Identifies one piece of system state an action touches.
#[derive(Serialize, Deserialize, Clone, PartialEq, Eq, Hash, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ValueKey {
    Registry { hive: Hive, path: String, name: String },
    Service { name: String },
    Task { path: String },
    Appx { package: String },
    PowerScheme,
    Script { tweak_id: String, index: usize },
}

/// Captured prior state of a `ValueKey`, used for undo.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Snapshot {
    Reg(Option<RegData>),
    Service(StartType),
    Task(bool),
    Appx(bool),
    Power(String),
    Script(String),
}
