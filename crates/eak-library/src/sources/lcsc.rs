//! The LCSC adapter slot. **Not connected**: there is no LCSC integration in EAK, and EAK does
//! not use unofficial or unauthorised LCSC endpoints. The slot exists so a UI can list LCSC as
//! a known supplier and show it as unavailable. It is always `Disabled`, never calls anything,
//! and returns no records.

use crate::part::{PartKey, PartRecord, SourceId};
use crate::source::{
    Capabilities, PartQuery, PartSource, SourceAnswer, SourceError, SourceKind, SourceStatus,
};

/// The LCSC slot (see the module documentation).
#[derive(Debug, Clone, Copy, Default)]
pub struct LcscSlot;

impl LcscSlot {
    pub const ID: &'static str = "supplier:lcsc";
    pub const STATUS_TEXT: &'static str = "not connected";

    fn not_connected() -> SourceError {
        SourceError::Unavailable("supplier:lcsc: not connected; no request was made".into())
    }
}

impl PartSource for LcscSlot {
    fn id(&self) -> SourceId {
        SourceId::new(Self::ID)
    }
    fn kind(&self) -> SourceKind {
        SourceKind::Network
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities::default()
    }
    fn status(&self) -> SourceStatus {
        SourceStatus::Disabled
    }
    fn status_text(&self) -> String {
        Self::STATUS_TEXT.to_string()
    }
    fn terms(&self) -> String {
        "LCSC: not connected; no LCSC adapter is implemented".into()
    }
    fn search(&self, _q: &PartQuery) -> Result<SourceAnswer, SourceError> {
        Err(Self::not_connected())
    }
    fn fetch(&self, _key: &PartKey) -> Result<Option<PartRecord>, SourceError> {
        Err(Self::not_connected())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::resolve;

    #[test]
    fn lcsc_slot_is_disabled_not_connected_and_never_answers() {
        let s = LcscSlot;
        assert_eq!(s.status(), SourceStatus::Disabled);
        assert_eq!(s.status_text(), "not connected");
        assert_eq!(s.capabilities(), Capabilities::default());
        assert!(matches!(
            s.search(&PartQuery::new("TPS2051")),
            Err(SourceError::Unavailable(_))
        ));
        let r = resolve(&[&s], &PartQuery::new("TPS2051"));
        assert!(r.parts.is_empty() && r.suggestions.is_empty());
        assert_eq!(r.sources.len(), 1);
        assert_eq!(r.sources[0].status, SourceStatus::Disabled);
    }
}
