//! Datasheet-aware values, supplier references and fact labels (ADR-0024).
//!
//! These are standalone data types, **not IR fields**: `ComponentDef` is unchanged (adding fields
//! to it is a migration-bearing shared-contract change, scheduled separately). They give every
//! consumer one vocabulary so a headline number is never mistaken for a design-grade limit:
//!
//! * a [`DatasheetValue`] says whether it is an absolute maximum, a recommended operating limit or a
//!   characteristic, which statistic (min/typ/max) it is, under which conditions, for which
//!   package/part, from which document revision and page, and whether a person has verified it;
//! * a [`SupplierRef`] is a listing (supplier part number, URL, observation time); it is separate
//!   from engineering identity and never proof of electrical suitability;
//! * a [`FactLabel`] marks how trustworthy a statement shown to an engineer or agent is.
//!
//! Nothing here evaluates a design.

use serde::{Deserialize, Serialize};

/// What kind of number a datasheet value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LimitClass {
    /// Stress rating; operation at or beyond it is not guaranteed and may damage the part.
    AbsoluteMaximum,
    /// Recommended operating condition (the range the electrical characteristics are valid in).
    RecommendedOperating,
    /// A characteristic measured or guaranteed under stated test conditions.
    Characteristic,
}

/// Which statistic of the characteristic the value is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Statistic {
    Min,
    Typ,
    Max,
}

/// How the value entered EAK.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "how", rename_all = "snake_case")]
pub enum Extraction {
    /// Read from the document by software (or a model). Unverified until a person checks it.
    ExtractedAuto,
    /// A named person checked it against the cited page.
    HumanVerified { reviewer: String, at_unix_s: u64 },
}

/// Where in which document the value is stated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasheetSource {
    pub manufacturer: String,
    /// SHA-256 (hex) of the document file the value was read from.
    pub document_sha256: String,
    /// Manufacturer revision label, e.g. `Rev. C`.
    #[serde(default)]
    pub document_revision: Option<String>,
    #[serde(default)]
    pub page: Option<u32>,
    #[serde(default)]
    pub section: Option<String>,
    /// Table or footnote reference, when the value comes from one.
    #[serde(default)]
    pub table: Option<String>,
}

/// Part scope a value applies to (values often differ per package or grade).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PartScope {
    #[serde(default)]
    pub mpn: Option<String>,
    #[serde(default)]
    pub package: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatasheetValue {
    /// Parameter name as the datasheet writes it, e.g. `Input voltage`.
    pub parameter: String,
    pub class: LimitClass,
    /// `None` for an absolute maximum or a single stated limit.
    #[serde(default)]
    pub statistic: Option<Statistic>,
    pub value: f64,
    /// Unit as written (`V`, `mA`, `°C`).
    pub unit: String,
    /// Test or operating conditions the value is valid under (empty = none stated).
    #[serde(default)]
    pub conditions: String,
    #[serde(default)]
    pub scope: PartScope,
    pub source: DatasheetSource,
    pub extraction: Extraction,
}

impl DatasheetValue {
    pub fn is_human_verified(&self) -> bool {
        matches!(self.extraction, Extraction::HumanVerified { .. })
    }

    /// True only for a human-verified absolute maximum or recommended-operating limit.
    /// A typical value, an unverified extraction or a characteristic is never design-grade for a
    /// limit check; callers must report such inputs as unverified rather than pass them.
    pub fn is_design_grade_limit(&self) -> bool {
        self.is_human_verified()
            && matches!(
                self.class,
                LimitClass::AbsoluteMaximum | LimitClass::RecommendedOperating
            )
            && self.statistic != Some(Statistic::Typ)
    }

    /// Short, honest description for display and citations.
    pub fn describe(&self) -> String {
        let stat = match self.statistic {
            Some(Statistic::Min) => " min",
            Some(Statistic::Typ) => " typ",
            Some(Statistic::Max) => " max",
            None => "",
        };
        let class = match self.class {
            LimitClass::AbsoluteMaximum => "absolute maximum",
            LimitClass::RecommendedOperating => "recommended operating",
            LimitClass::Characteristic => "characteristic",
        };
        let how = if self.is_human_verified() {
            "human-verified"
        } else {
            "extracted automatically, unverified"
        };
        let page = self
            .source
            .page
            .map(|p| format!(", p{p}"))
            .unwrap_or_default();
        format!(
            "{} {}{stat} = {} {} ({class}; {how}; {}{page})",
            self.parameter,
            self.scope.mpn.as_deref().unwrap_or("?"),
            self.value,
            self.unit,
            self.source.manufacturer
        )
    }
}

/// A supplier listing for a part. Not engineering identity and not proof of suitability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierRef {
    /// `lcsc`, ...
    pub supplier: String,
    pub supplier_part: String,
    #[serde(default)]
    pub listing_url: Option<String>,
    /// When the listing was observed (Unix seconds); stock and price are only valid then.
    pub observed_at_unix_s: u64,
    #[serde(default)]
    pub lifecycle: Option<String>,
}

/// How far a statement shown to an engineer or agent can be trusted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactLabel {
    /// Quoted from a cited source document by software.
    SourceExtracted,
    /// A model's reading or summary of sources.
    ModelInterpretation,
    /// Computed by `eak-engineering` from explicit inputs.
    Calculated,
    /// An engineering assumption someone stated.
    Assumption,
    /// A person approved it.
    HumanApproved,
    /// Nothing supports it yet. The default.
    #[default]
    Unverified,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn value(class: LimitClass, stat: Option<Statistic>, ex: Extraction) -> DatasheetValue {
        DatasheetValue {
            parameter: "Input voltage".into(),
            class,
            statistic: stat,
            value: 6.0,
            unit: "V".into(),
            conditions: "TA = 25 C".into(),
            scope: PartScope {
                mpn: Some("TPS62130".into()),
                package: Some("VQFN-16".into()),
            },
            source: DatasheetSource {
                manufacturer: "Example Semi".into(),
                document_sha256: "00".into(),
                document_revision: Some("Rev. C".into()),
                page: Some(4),
                section: Some("Absolute Maximum Ratings".into()),
                table: None,
            },
            extraction: ex,
        }
    }

    fn human() -> Extraction {
        Extraction::HumanVerified {
            reviewer: "reviewer".into(),
            at_unix_s: 1,
        }
    }

    #[test]
    fn only_human_verified_limits_are_design_grade() {
        assert!(value(LimitClass::AbsoluteMaximum, None, human()).is_design_grade_limit());
        assert!(
            !value(LimitClass::AbsoluteMaximum, None, Extraction::ExtractedAuto)
                .is_design_grade_limit()
        );
        assert!(
            !value(LimitClass::Characteristic, Some(Statistic::Max), human())
                .is_design_grade_limit()
        );
        assert!(!value(
            LimitClass::RecommendedOperating,
            Some(Statistic::Typ),
            human()
        )
        .is_design_grade_limit());
    }

    #[test]
    fn description_states_class_and_verification() {
        let d = value(LimitClass::AbsoluteMaximum, None, Extraction::ExtractedAuto).describe();
        assert!(d.contains("absolute maximum") && d.contains("unverified") && d.contains("p4"));
    }

    #[test]
    fn default_label_is_unverified_and_json_roundtrips() {
        assert_eq!(FactLabel::default(), FactLabel::Unverified);
        let v = value(LimitClass::AbsoluteMaximum, None, human());
        let s = serde_json::to_string(&v).unwrap();
        assert_eq!(serde_json::from_str::<DatasheetValue>(&s).unwrap(), v);
    }
}
