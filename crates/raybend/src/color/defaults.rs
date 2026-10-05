//! Device defaults affect only a still-unresolved new photo. Once written into
//! its develop stack, the portable PhotoColorState is independent of these rules.
use super::{PhotoColorState, ProfileId, SourceColor, OutputColor};
use serde::{Deserialize, Serialize};

pub const SETTINGS_KEY: &str = "color.defaults.v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum UntaggedInput {
    #[default]
    Srgb,
    RgbIcc { profile_id: ProfileId },
    RequireAssignment,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ColorDefaults {
    #[serde(default)]
    pub untagged_input: UntaggedInput,
    #[serde(default)]
    pub output: OutputColor,
}

impl ColorDefaults {
    /// Source evidence always wins over the untagged rule. RAW must already
    /// carry the actual matrix identity from the worker; RGB defaults cannot
    /// invent or replace a camera calibration.
    pub fn resolve_bitmap(&self, evidence: Option<SourceColor>) -> Result<PhotoColorState, &'static str> {
        let source = match evidence {
            Some(SourceColor::EmbeddedIcc { profile_id }) => SourceColor::EmbeddedIcc { profile_id },
            Some(SourceColor::TaggedSrgb) => SourceColor::TaggedSrgb,
            Some(SourceColor::AssumedSrgb) | None => match &self.untagged_input {
                UntaggedInput::Srgb => SourceColor::AssumedSrgb,
                UntaggedInput::RgbIcc { profile_id } => SourceColor::AssignedRgbIcc { profile_id: profile_id.clone() },
                UntaggedInput::RequireAssignment => return Err("untagged bitmap requires an explicit input assignment"),
            },
            _ => return Err("bitmap defaults cannot resolve this source kind"),
        };
        Ok(PhotoColorState::new_pipeline(source))
    }
}

pub fn load(conn: &rusqlite::Connection) -> crate::Result<ColorDefaults> {
    use rusqlite::OptionalExtension;
    let json: Option<String> = conn.query_row("SELECT value FROM settings WHERE key=?1", [SETTINGS_KEY], |row| row.get(0)).optional()?;
    json.map_or_else(|| Ok(ColorDefaults::default()), |json| serde_json::from_str(&json)
        .map_err(|e| crate::Error::Unsupported(format!("色彩默认规则损坏：{e}"))))
}

pub fn save(conn: &rusqlite::Connection, defaults: &ColorDefaults, now_ms: i64) -> crate::Result<()> {
    conn.execute("INSERT INTO settings(key,value,updated_at) VALUES(?1,?2,?3) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at",
        rusqlite::params![SETTINGS_KEY, serde_json::to_string(defaults)?, now_ms])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_roundtrip_without_retroactively_changing_frozen_photo() {
        let mut conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::store::migration::apply(&mut conn, crate::store::migration::DbKind::App, crate::store::migration::Backups::none(), 0).unwrap();
        let original = load(&conn).unwrap();
        let frozen = original.resolve_bitmap(None).unwrap();
        let id = ProfileId::of_bytes(b"profile");
        let changed = ColorDefaults { untagged_input: UntaggedInput::RgbIcc { profile_id: id.clone() }, output: OutputColor::DisplayP3 };
        save(&conn, &changed, 1).unwrap();
        assert_eq!(load(&conn).unwrap(), changed);
        assert_eq!(frozen.source, SourceColor::AssumedSrgb);
        assert_eq!(changed.resolve_bitmap(None).unwrap().source, SourceColor::AssignedRgbIcc { profile_id: id.clone() });
        let embedded = SourceColor::EmbeddedIcc { profile_id: ProfileId::of_bytes(b"embedded") };
        assert_eq!(changed.resolve_bitmap(Some(embedded.clone())).unwrap().source, embedded);
        assert!(changed.resolve_bitmap(Some(SourceColor::RawCameraMatrix { matrix_id: id })).is_err());
        let require = ColorDefaults { untagged_input: UntaggedInput::RequireAssignment, ..Default::default() };
        assert!(require.resolve_bitmap(None).is_err());
        assert!(require.resolve_bitmap(Some(SourceColor::TaggedSrgb)).is_ok());
        conn.execute("UPDATE settings SET value='invalid' WHERE key=?1", [SETTINGS_KEY]).unwrap();
        assert!(load(&conn).is_err());
    }
}
