//! Structured project requirements (schema v9, ADR-0057).
//!
//! The requirement record is part of the authoritative project. Every value carries its unit,
//! status, source, owner, the wall-clock time it was recorded and the revision whose command
//! recorded it (the operation journal names the actor of that revision). It changes only through
//! the typed requirement commands of `eak-runtime` (`SetRequirementField`, `ClearRequirementField`,
//! `UpsertRequirementItem`, `DeleteRequirementItem`).
//!
//! Status semantics. Nothing is inferred and nothing is defaulted:
//! * `UNKNOWN` – nobody knows the value yet. It carries no value. A field with no entry at all is
//!   UNKNOWN too: a missing value is never zero and never a default.
//! * `NEEDS_CONFIRMATION` – a working value (a client statement not yet played back, an
//!   engineer's assumption, or an agent's proposal) with its source, the owner who must confirm
//!   it, and the risk if it is wrong. Good enough for provisional work such as the block
//!   plan or part candidates, never for sizing, safety spacing or release.
//! * `CONFIRMED` – a value confirmed by a named owner, with its source. Only a person sets it
//!   (the engine refuses it from an agent).
//! * `DERIVED` – computed from other requirement fields, which it names. It is firm only while
//!   every input is firm and none changed after it was derived (otherwise it is stale); an UNKNOWN
//!   input is reported as an issue. An agent never records it.
//! * `NOT_APPLICABLE` – the field does not apply to this product. It carries no value and needs a
//!   reason. Only fields the catalogue allows may be not applicable, and only a person says so.
//!
//! [`evaluate`] is a pure, deterministic function of the record: completeness per section, the
//! downstream operations each missing or unconfirmed value blocks, and targeted questions for
//! the missing values that materially change architecture or safety. The DESK requirement
//! taxonomy (`docs/00_REQUIREMENTS.md`, ten dimensions) is the ten sections of [`Section`].

use crate::ir::{new_id, Id};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Longest free text accepted in a value, note, owner or source reference (characters).
pub const MAX_TEXT: usize = 2000;

// ------------------------------------------------------------------ record

/// How much is known about one requirement value (see the module documentation).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReqStatus {
    Unknown,
    NeedsConfirmation,
    Confirmed,
    Derived,
    NotApplicable,
}

impl ReqStatus {
    pub const ALL: [ReqStatus; 5] = [
        ReqStatus::Unknown,
        ReqStatus::NeedsConfirmation,
        ReqStatus::Confirmed,
        ReqStatus::Derived,
        ReqStatus::NotApplicable,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ReqStatus::Unknown => "UNKNOWN",
            ReqStatus::NeedsConfirmation => "NEEDS_CONFIRMATION",
            ReqStatus::Confirmed => "CONFIRMED",
            ReqStatus::Derived => "DERIVED",
            ReqStatus::NotApplicable => "NOT_APPLICABLE",
        }
    }

    /// Statuses an agent may never record: it never confirms a requirement, declares one not
    /// applicable, or certifies its own computation as DERIVED (the AI never self-certifies; an
    /// agent's computed value is NEEDS_CONFIRMATION with source `calculation` or `agent_proposal`).
    pub fn human_only(self) -> bool {
        matches!(
            self,
            ReqStatus::Confirmed | ReqStatus::NotApplicable | ReqStatus::Derived
        )
    }
}

/// What a quantity measures. A value's unit must measure what its field asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Measure {
    Voltage,
    Current,
    Power,
    Charge,
    Length,
    Temperature,
    Frequency,
    DataRate,
    Time,
    Ratio,
    Mass,
    Acceleration,
    Count,
    DataSize,
    Money,
}

impl Measure {
    /// Whether a negative value is meaningful (a -12 V rail, a -40 degC limit, a reverse
    /// current). Every other measure must be zero or more.
    pub fn signed(self) -> bool {
        matches!(
            self,
            Measure::Voltage | Measure::Current | Measure::Temperature
        )
    }
}

/// A requirement unit. Serialised as its symbol (`"V"`, `"mA"`, `"degC"`, `"USD"`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReqUnit {
    #[serde(rename = "V")]
    Volt,
    #[serde(rename = "mV")]
    Millivolt,
    #[serde(rename = "kV")]
    Kilovolt,
    #[serde(rename = "A")]
    Ampere,
    #[serde(rename = "mA")]
    Milliampere,
    #[serde(rename = "uA")]
    Microampere,
    #[serde(rename = "W")]
    Watt,
    #[serde(rename = "mW")]
    Milliwatt,
    #[serde(rename = "mAh")]
    MilliampereHour,
    #[serde(rename = "m")]
    Metre,
    #[serde(rename = "mm")]
    Millimetre,
    #[serde(rename = "um")]
    Micrometre,
    #[serde(rename = "degC")]
    DegreeCelsius,
    #[serde(rename = "Hz")]
    Hertz,
    #[serde(rename = "kHz")]
    Kilohertz,
    #[serde(rename = "MHz")]
    Megahertz,
    #[serde(rename = "GHz")]
    Gigahertz,
    #[serde(rename = "bps")]
    BitPerSecond,
    #[serde(rename = "kbps")]
    KilobitPerSecond,
    #[serde(rename = "Mbps")]
    MegabitPerSecond,
    #[serde(rename = "s")]
    Second,
    #[serde(rename = "ms")]
    Millisecond,
    #[serde(rename = "h")]
    Hour,
    #[serde(rename = "day")]
    Day,
    #[serde(rename = "year")]
    Year,
    #[serde(rename = "percent")]
    Percent,
    #[serde(rename = "g")]
    Gram,
    #[serde(rename = "gn")]
    StandardGravity,
    #[serde(rename = "pcs")]
    Piece,
    #[serde(rename = "KiB")]
    Kibibyte,
    #[serde(rename = "MiB")]
    Mebibyte,
    #[serde(rename = "USD")]
    Usd,
    #[serde(rename = "EUR")]
    Eur,
    #[serde(rename = "GBP")]
    Gbp,
    #[serde(rename = "INR")]
    Inr,
    #[serde(rename = "CNY")]
    Cny,
}

impl ReqUnit {
    pub const ALL: [ReqUnit; 36] = [
        ReqUnit::Volt,
        ReqUnit::Millivolt,
        ReqUnit::Kilovolt,
        ReqUnit::Ampere,
        ReqUnit::Milliampere,
        ReqUnit::Microampere,
        ReqUnit::Watt,
        ReqUnit::Milliwatt,
        ReqUnit::MilliampereHour,
        ReqUnit::Metre,
        ReqUnit::Millimetre,
        ReqUnit::Micrometre,
        ReqUnit::DegreeCelsius,
        ReqUnit::Hertz,
        ReqUnit::Kilohertz,
        ReqUnit::Megahertz,
        ReqUnit::Gigahertz,
        ReqUnit::BitPerSecond,
        ReqUnit::KilobitPerSecond,
        ReqUnit::MegabitPerSecond,
        ReqUnit::Second,
        ReqUnit::Millisecond,
        ReqUnit::Hour,
        ReqUnit::Day,
        ReqUnit::Year,
        ReqUnit::Percent,
        ReqUnit::Gram,
        ReqUnit::StandardGravity,
        ReqUnit::Piece,
        ReqUnit::Kibibyte,
        ReqUnit::Mebibyte,
        ReqUnit::Usd,
        ReqUnit::Eur,
        ReqUnit::Gbp,
        ReqUnit::Inr,
        ReqUnit::Cny,
    ];

    pub fn measure(self) -> Measure {
        use ReqUnit::*;
        match self {
            Volt | Millivolt | Kilovolt => Measure::Voltage,
            Ampere | Milliampere | Microampere => Measure::Current,
            Watt | Milliwatt => Measure::Power,
            MilliampereHour => Measure::Charge,
            Metre | Millimetre | Micrometre => Measure::Length,
            DegreeCelsius => Measure::Temperature,
            Hertz | Kilohertz | Megahertz | Gigahertz => Measure::Frequency,
            BitPerSecond | KilobitPerSecond | MegabitPerSecond => Measure::DataRate,
            Second | Millisecond | Hour | Day | Year => Measure::Time,
            Percent => Measure::Ratio,
            Gram => Measure::Mass,
            StandardGravity => Measure::Acceleration,
            Piece => Measure::Count,
            Kibibyte | Mebibyte => Measure::DataSize,
            Usd | Eur | Gbp | Inr | Cny => Measure::Money,
        }
    }

    /// The serialised symbol.
    pub fn symbol(self) -> &'static str {
        use ReqUnit::*;
        match self {
            Volt => "V",
            Millivolt => "mV",
            Kilovolt => "kV",
            Ampere => "A",
            Milliampere => "mA",
            Microampere => "uA",
            Watt => "W",
            Milliwatt => "mW",
            MilliampereHour => "mAh",
            Metre => "m",
            Millimetre => "mm",
            Micrometre => "um",
            DegreeCelsius => "degC",
            Hertz => "Hz",
            Kilohertz => "kHz",
            Megahertz => "MHz",
            Gigahertz => "GHz",
            BitPerSecond => "bps",
            KilobitPerSecond => "kbps",
            MegabitPerSecond => "Mbps",
            Second => "s",
            Millisecond => "ms",
            Hour => "h",
            Day => "day",
            Year => "year",
            Percent => "percent",
            Gram => "g",
            StandardGravity => "gn",
            Piece => "pcs",
            Kibibyte => "KiB",
            Mebibyte => "MiB",
            Usd => "USD",
            Eur => "EUR",
            Gbp => "GBP",
            Inr => "INR",
            Cny => "CNY",
        }
    }
}

/// A requirement value, always with its unit when it is a quantity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ReqValue {
    /// One figure, optionally with a symmetric tolerance in percent.
    Number {
        value: f64,
        unit: ReqUnit,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tolerance_pct: Option<f64>,
    },
    /// Limits: at least one of min, nom, max, in order.
    Range {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        nom: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
        unit: ReqUnit,
    },
    Text {
        text: String,
    },
    Flag {
        value: bool,
    },
}

/// Who or what a value comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Client,
    Engineer,
    Document,
    Standard,
    Datasheet,
    Measurement,
    Assumption,
    /// A calculation by a person or a deterministic evaluator (name it in the reference).
    Calculation,
    /// A value an agent proposed; it stays NEEDS_CONFIRMATION until a person confirms it.
    AgentProposal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReqSource {
    pub kind: SourceKind,
    /// Exactly where: the person, document and section, standard clause, datasheet table.
    pub reference: String,
}

/// One recorded requirement field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReqEntry {
    pub status: ReqStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<ReqValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<ReqSource>,
    /// Requirement paths a DERIVED value is computed from.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
    /// The person responsible for the value (who answers or confirms it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// What goes wrong if a NEEDS_CONFIRMATION value turns out wrong (required for that status:
    /// DESK's "PROVISIONAL, risk noted").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub risk_if_wrong: Option<String>,
    /// Rationale, the question still open, or why it is not applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// When the entry was recorded, in milliseconds since the Unix epoch.
    pub updated_at_ms: u64,
    /// The project revision whose command recorded the entry (the journal names its actor).
    pub updated_revision: u64,
}

impl ReqEntry {
    /// Same content, ignoring when and at which revision it was recorded.
    pub fn same_content(&self, o: &ReqEntry) -> bool {
        self.status == o.status
            && self.value == o.value
            && self.source == o.source
            && self.derived_from == o.derived_from
            && self.owner == o.owner
            && self.risk_if_wrong == o.risk_if_wrong
            && self.note == o.note
    }
}

/// The kind of a repeated requirement item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReqItemKind {
    /// A supply rail. Rails form a tree through `parent` (the rail it is generated from).
    Rail,
    /// An external interface or connector.
    Interface,
}

impl ReqItemKind {
    pub fn label(self) -> &'static str {
        match self {
            ReqItemKind::Rail => "Rail",
            ReqItemKind::Interface => "Interface",
        }
    }
    fn id_prefix(self) -> &'static str {
        match self {
            ReqItemKind::Rail => "rail",
            ReqItemKind::Interface => "ifc",
        }
    }
    /// The fields every item of this kind has.
    pub fn fields(self) -> &'static [FieldDef] {
        match self {
            ReqItemKind::Rail => RAIL_FIELDS,
            ReqItemKind::Interface => INTERFACE_FIELDS,
        }
    }
}

/// A rail or an interface with its own requirement fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReqItem {
    pub id: Id,
    pub kind: ReqItemKind,
    pub name: String,
    /// Rails only: the rail this one is generated from (`None` = fed by the board input).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Id>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, ReqEntry>,
}

/// The structured requirement record of a project (`Project::requirement_set`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct RequirementSet {
    /// Project-level fields by catalogue key ([`FIELDS`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, ReqEntry>,
    /// Rails and interfaces by id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub items: BTreeMap<Id, ReqItem>,
}

// ------------------------------------------------------------------ catalogue

/// The requirement sections (Execution brief §8.1; DESK `00_REQUIREMENTS.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Section {
    Function,
    Supply,
    PowerBudget,
    Interfaces,
    Mechanical,
    Environmental,
    SafetyRegulatory,
    ManufacturingTest,
    CostSupply,
    LifecycleFirmware,
}

impl Section {
    pub const ALL: [Section; 10] = [
        Section::Function,
        Section::Supply,
        Section::PowerBudget,
        Section::Interfaces,
        Section::Mechanical,
        Section::Environmental,
        Section::SafetyRegulatory,
        Section::ManufacturingTest,
        Section::CostSupply,
        Section::LifecycleFirmware,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Section::Function => "Function and performance",
            Section::Supply => "Supplies and rail tree",
            Section::PowerBudget => "Power budget",
            Section::Interfaces => "Interfaces and connectors",
            Section::Mechanical => "Outline, height, mounting and enclosure",
            Section::Environmental => "Environment",
            Section::SafetyRegulatory => "Safety, regulatory and isolation",
            Section::ManufacturingTest => "Quantity, fabrication, assembly, test and programming",
            Section::CostSupply => "BOM target, lifecycle and second source",
            Section::LifecycleFirmware => "Firmware, serviceability and expansion",
        }
    }
}

/// The shape of value a field takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueKind {
    Number,
    Range,
    /// A number or a range.
    NumberOrRange,
    Text,
    Flag,
    /// A list of items of one kind: satisfied once one item exists. Its own entry can only be
    /// UNKNOWN (with an owner or note) or NOT_APPLICABLE (none exist, and none are needed).
    Items(ReqItemKind),
}

/// One catalogue field.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct FieldDef {
    pub key: &'static str,
    pub section: Section,
    pub label: &'static str,
    pub kind: ValueKind,
    /// What a quantity measures; `None` for text, flags and item lists.
    pub measure: Option<Measure>,
    /// May be declared NOT_APPLICABLE.
    pub na_allowed: bool,
    /// A missing value materially changes architecture or safety: EAK asks about it.
    pub material: bool,
    /// The targeted question for the engineer or client.
    pub question: &'static str,
}

const fn def(
    key: &'static str,
    section: Section,
    label: &'static str,
    kind: ValueKind,
    measure: Option<Measure>,
    na_allowed: bool,
    material: bool,
    question: &'static str,
) -> FieldDef {
    FieldDef {
        key,
        section,
        label,
        kind,
        measure,
        na_allowed,
        material,
        question,
    }
}

use Measure as M;
use Section as S;
use ValueKind as K;

/// Project-level fields, in presentation order.
#[rustfmt::skip]
pub const FIELDS: &[FieldDef] = &[
    // Function and performance
    def("function.description", S::Function, "Function", K::Text, None, false, true,
        "What must the board measure, control, display or communicate?"),
    def("function.performance", S::Function, "Performance targets", K::Text, None, false, true,
        "What accuracy, resolution, update rate or response time must it achieve?"),
    def("function.operating_modes", S::Function, "Operating modes", K::Text, None, false, true,
        "Which operating modes exist (off, sleep, active, charging, fault), and what must be off at reset and in a fault?"),
    // Supplies and power budget
    def("supply.sources", S::Supply, "Power sources", K::Text, None, false, true,
        "What powers the board (USB, battery chemistry and size, adapter, PoE, solar)?"),
    def("supply.input_voltage", S::Supply, "Input voltage (min / nom / max)", K::Range, Some(M::Voltage), false, true,
        "What is the input voltage range, including worst-case transients and the end-of-battery minimum?"),
    def("supply.reverse_polarity", S::Supply, "Reverse polarity possible", K::Flag, None, true, true,
        "Can the supply be connected the wrong way round?"),
    def("supply.hot_plug", S::Supply, "Hot-plugged supply", K::Flag, None, true, false,
        "Is the supply connected or disconnected while powered?"),
    def("supply.rails", S::Supply, "Rail tree", K::Items(ReqItemKind::Rail), None, false, true,
        "Which supply rails does the board need, and which rail is each one generated from?"),
    // Power budget
    def("power_budget.peak_current", S::PowerBudget, "Peak input current", K::Number, Some(M::Current), false, true,
        "What is the peak input current in the worst operating mode?"),
    def("power_budget.average_current", S::PowerBudget, "Average input current", K::Number, Some(M::Current), false, true,
        "What is the average input current over a typical duty cycle?"),
    def("power_budget.sleep_current", S::PowerBudget, "Sleep current", K::Number, Some(M::Current), true, false,
        "What is the allowed current in sleep or standby?"),
    def("power_budget.total_power", S::PowerBudget, "Power budget", K::Number, Some(M::Power), false, true,
        "What total power may the board draw?"),
    def("power_budget.battery_life", S::PowerBudget, "Battery life target", K::Number, Some(M::Time), true, true,
        "How long must the board run on one battery charge?"),
    // Interfaces and connectors
    def("interfaces.list", S::Interfaces, "External interfaces", K::Items(ReqItemKind::Interface), None, true, true,
        "Which external connections (connectors, cables, buttons, displays, radios) does the board have?"),
    // Outline, height, mounting and enclosure
    def("mechanical.max_length", S::Mechanical, "Maximum board length", K::Number, Some(M::Length), false, true,
        "What is the largest board length that still fits?"),
    def("mechanical.max_width", S::Mechanical, "Maximum board width", K::Number, Some(M::Length), false, true,
        "What is the largest board width that still fits?"),
    def("mechanical.board_thickness", S::Mechanical, "Board thickness", K::NumberOrRange, Some(M::Length), false, false,
        "What board thickness is required or allowed?"),
    def("mechanical.max_height_top", S::Mechanical, "Maximum component height (top)", K::Number, Some(M::Length), false, true,
        "How tall may the tallest part on the top side be?"),
    def("mechanical.max_height_bottom", S::Mechanical, "Maximum component height (bottom)", K::Number, Some(M::Length), false, true,
        "How tall may the tallest part on the bottom side be (0 if the bottom must stay empty)?"),
    def("mechanical.mounting", S::Mechanical, "Mounting", K::Text, None, true, false,
        "How is the board mounted (holes and their positions, standoffs, rails, adhesive)?"),
    def("mechanical.enclosure", S::Mechanical, "Enclosure", K::Text, None, true, true,
        "Which enclosure holds the board, and is it sealed or ventilated?"),
    def("mechanical.connector_positions", S::Mechanical, "Connector, button and LED positions", K::Text, None, true, false,
        "Where must connectors, buttons and LED windows sit, with what tolerance?"),
    def("mechanical.max_weight", S::Mechanical, "Maximum weight", K::Number, Some(M::Mass), true, false,
        "Is there a weight limit for the assembled board?"),
    // Environment
    def("environment.operating_temp", S::Environmental, "Operating temperature", K::Range, Some(M::Temperature), false, true,
        "What ambient temperature range must the board operate in?"),
    def("environment.storage_temp", S::Environmental, "Storage temperature", K::Range, Some(M::Temperature), false, false,
        "What temperature range must it survive unpowered?"),
    def("environment.humidity", S::Environmental, "Relative humidity", K::Range, Some(M::Ratio), false, false,
        "What relative humidity range does it see?"),
    def("environment.condensation", S::Environmental, "Condensation possible", K::Flag, None, false, true,
        "Can condensation form on the board?"),
    def("environment.ingress", S::Environmental, "Dust and water ingress", K::Text, None, true, true,
        "What dust and water exposure (IP rating) must it withstand?"),
    def("environment.vibration", S::Environmental, "Vibration and shock", K::Text, None, true, false,
        "What vibration or shock must it survive?"),
    def("environment.altitude", S::Environmental, "Maximum altitude", K::Number, Some(M::Length), false, true,
        "What is the highest altitude it operates at (clearances grow above 2000 m)?"),
    // Safety, regulatory and isolation
    def("safety.markets", S::SafetyRegulatory, "Markets and marks", K::Text, None, false, true,
        "Which markets and marks apply (CE, UKCA, FCC, ...)?"),
    def("safety.sector", S::SafetyRegulatory, "Product sector", K::Text, None, false, true,
        "Is it consumer, industrial, medical, automotive or another regulated sector?"),
    def("safety.max_working_voltage", S::SafetyRegulatory, "Highest working voltage", K::Number, Some(M::Voltage), false, true,
        "What is the highest working voltage anywhere on the board, including mains or high-voltage parts?"),
    def("safety.isolation", S::SafetyRegulatory, "Isolation", K::Text, None, false, true,
        "What isolation is required between circuits (none, functional, basic, reinforced), and between which?"),
    def("safety.standards", S::SafetyRegulatory, "Safety standards", K::Text, None, true, true,
        "Which safety standards must the product meet?"),
    def("safety.radio_certification", S::SafetyRegulatory, "Radio certification", K::Text, None, true, false,
        "Does the product transmit, and with which radio certification (or a pre-certified module)?"),
    def("safety.battery_transport", S::SafetyRegulatory, "Battery transport", K::Text, None, true, false,
        "Does a battery ship with it, and under which transport rules (UN 38.3)?"),
    // Quantity, fabrication, assembly, test and programming
    def("manufacturing.quantity", S::ManufacturingTest, "Build quantity", K::Number, Some(M::Count), false, true,
        "How many boards per build (prototype, pilot, production volume)?"),
    def("manufacturing.fabricator", S::ManufacturingTest, "PCB fabricator", K::Text, None, false, false,
        "Which fabricator makes the bare board?"),
    def("manufacturing.assembler", S::ManufacturingTest, "Assembler", K::Text, None, false, true,
        "Who assembles the board, and with which part library (basic or preferred parts)?"),
    def("manufacturing.assembly_process", S::ManufacturingTest, "Assembly process", K::Text, None, false, true,
        "One- or two-sided assembly, SMT only or with through-hole?"),
    def("manufacturing.test_method", S::ManufacturingTest, "Test method", K::Text, None, false, false,
        "How is each board tested (test pads, fixture, functional test)?"),
    def("manufacturing.programming_method", S::ManufacturingTest, "Programming method", K::Text, None, true, false,
        "How is firmware loaded (header, pogo pads, pre-programmed parts)?"),
    // BOM target, lifecycle and second source
    def("cost.bom_target", S::CostSupply, "BOM cost target", K::Number, Some(M::Money), false, true,
        "What is the target BOM cost per board?"),
    def("cost.bom_target_volume", S::CostSupply, "Volume of the BOM target", K::Number, Some(M::Count), false, false,
        "At which build volume does the BOM cost target apply?"),
    def("supply_chain.lifecycle", S::CostSupply, "Required part availability", K::Number, Some(M::Time), false, false,
        "For how long must the parts stay available?"),
    def("supply_chain.second_source", S::CostSupply, "Second sources", K::Text, None, true, false,
        "Which parts need a second source?"),
    def("supply_chain.availability", S::CostSupply, "Stock and availability rule", K::Text, None, true, false,
        "Which stock rule applies (for example stock of at least ten times the build quantity)?"),
    // Firmware, serviceability and expansion
    def("firmware.update_method", S::LifecycleFirmware, "Firmware updates", K::Text, None, true, false,
        "How is firmware updated in the field (OTA, cable, never)?"),
    def("firmware.memory_min", S::LifecycleFirmware, "Minimum program memory", K::Number, Some(M::DataSize), true, false,
        "How much program memory must the controller have, with margin?"),
    def("lifecycle.serviceability", S::LifecycleFirmware, "Serviceability", K::Text, None, true, false,
        "Is the board serviced in the field, and what must a technician reach?"),
    def("lifecycle.variants", S::LifecycleFirmware, "Variants", K::Text, None, true, false,
        "Which product variants share this board (fitted or not-fitted options)?"),
    def("lifecycle.expansion", S::LifecycleFirmware, "Expansion", K::Text, None, true, false,
        "Which future expansion must the board allow (spare pins, headers)?"),
];

/// Fields of every rail.
#[rustfmt::skip]
pub const RAIL_FIELDS: &[FieldDef] = &[
    def("voltage", S::Supply, "Voltage (min / nom / max)", K::Range, Some(M::Voltage), false, true,
        "What are the minimum, nominal and maximum voltages of this rail?"),
    def("current_peak", S::Supply, "Peak current", K::Number, Some(M::Current), false, true,
        "What peak current does this rail deliver?"),
    def("current_continuous", S::Supply, "Continuous current", K::Number, Some(M::Current), false, true,
        "What continuous current does this rail deliver?"),
    def("current_average", S::Supply, "Average current", K::Number, Some(M::Current), true, false,
        "What average current does this rail deliver over the duty cycle?"),
    def("loads", S::Supply, "Loads", K::Text, None, false, false,
        "Which parts are powered from this rail?"),
];

/// Fields of every interface.
#[rustfmt::skip]
pub const INTERFACE_FIELDS: &[FieldDef] = &[
    def("protocol", S::Interfaces, "Protocol", K::Text, None, false, true,
        "Which protocol or signal type does this interface carry?"),
    def("connector", S::Interfaces, "Connector", K::Text, None, false, true,
        "Which connector family and part is required, or is it free to choose?"),
    def("pinout", S::Interfaces, "External pinout", K::Text, None, true, false,
        "Is the pinout fixed by someone else, and what is it?"),
    def("voltage_level", S::Interfaces, "Signal voltage level", K::NumberOrRange, Some(M::Voltage), true, true,
        "What signal voltage levels does this interface use?"),
    def("data_rate", S::Interfaces, "Data rate", K::Number, Some(M::DataRate), true, false,
        "What data rate does this interface run at?"),
    def("cable_length", S::Interfaces, "Cable length", K::Number, Some(M::Length), true, false,
        "How long is the cable attached to this interface?"),
    def("esd_level", S::Interfaces, "ESD exposure", K::Number, Some(M::Voltage), true, true,
        "What ESD level must this interface withstand (for example 8 kV contact)?"),
];

/// Catalogue definition of a project-level field.
pub fn field_def(key: &str) -> Option<&'static FieldDef> {
    FIELDS.iter().find(|d| d.key == key)
}

/// Catalogue definition of a field of an item kind.
pub fn item_field_def(kind: ReqItemKind, key: &str) -> Option<&'static FieldDef> {
    kind.fields().iter().find(|d| d.key == key)
}

// ------------------------------------------------------------------ paths and edits

/// A requirement path: a project field key (`supply.input_voltage`) or an item field
/// (`<item id>/<field>`, e.g. `rail_1a2b/voltage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReqPath<'a> {
    Field(&'a str),
    Item { id: &'a str, field: &'a str },
}

pub fn parse_path(path: &str) -> Result<ReqPath<'_>, String> {
    let mut parts = path.split('/');
    let first = parts.next().unwrap_or("");
    let second = parts.next();
    if parts.next().is_some() || first.is_empty() || second.is_some_and(str::is_empty) {
        return Err(format!(
            "requirement path {path:?} is not `<field>` or `<item id>/<field>`"
        ));
    }
    Ok(match second {
        None => ReqPath::Field(first),
        Some(field) => ReqPath::Item { id: first, field },
    })
}

/// Why a requirement edit was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReqEditError {
    NotFound(String),
    Invalid(String),
    Precondition(String),
    /// The edit would change nothing.
    Unchanged(String),
}

pub fn edit_message(e: &ReqEditError) -> String {
    match e {
        ReqEditError::NotFound(m) => format!("not found: {m}"),
        ReqEditError::Invalid(m) | ReqEditError::Precondition(m) | ReqEditError::Unchanged(m) => {
            m.clone()
        }
    }
}

fn clean(s: Option<&str>) -> Option<String> {
    s.map(str::trim).filter(|s| !s.is_empty()).map(String::from)
}

/// The key an entry is stored under in its map (the field part of the path).
fn entry_key(path: &str) -> Result<String, ReqEditError> {
    match parse_path(path).map_err(ReqEditError::Invalid)? {
        ReqPath::Field(k) => Ok(k.to_string()),
        ReqPath::Item { field, .. } => Ok(field.to_string()),
    }
}

impl RequirementSet {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty() && self.items.is_empty()
    }

    /// The catalogue definition a path names; the item must exist.
    pub fn def_of(&self, path: &str) -> Result<&'static FieldDef, ReqEditError> {
        match parse_path(path).map_err(ReqEditError::Invalid)? {
            ReqPath::Field(k) => field_def(k).ok_or_else(|| {
                ReqEditError::NotFound(format!("requirement field {k} (not in the catalogue)"))
            }),
            ReqPath::Item { id, field } => {
                let item = self
                    .items
                    .get(id)
                    .ok_or_else(|| ReqEditError::NotFound(format!("requirement item {id}")))?;
                item_field_def(item.kind, field).ok_or_else(|| {
                    ReqEditError::NotFound(format!(
                        "{} field {field} (not in the catalogue)",
                        item.kind.label().to_lowercase()
                    ))
                })
            }
        }
    }

    /// The recorded entry at a path, if any.
    pub fn entry(&self, path: &str) -> Option<&ReqEntry> {
        match parse_path(path).ok()? {
            ReqPath::Field(k) => self.fields.get(k),
            ReqPath::Item { id, field } => self.items.get(id)?.fields.get(field),
        }
    }

    fn slot(&mut self, path: &str) -> Option<&mut BTreeMap<String, ReqEntry>> {
        match parse_path(path).ok()? {
            ReqPath::Field(_) => Some(&mut self.fields),
            ReqPath::Item { id, .. } => self.items.get_mut(id).map(|i| &mut i.fields),
        }
    }

    /// Items of one kind, by id.
    pub fn items_of(&self, kind: ReqItemKind) -> impl Iterator<Item = &ReqItem> {
        self.items.values().filter(move |i| i.kind == kind)
    }

    /// Whether the list field of `kind` is recorded NOT_APPLICABLE.
    fn list_not_applicable(&self, kind: ReqItemKind) -> bool {
        FIELDS
            .iter()
            .filter(|d| d.kind == ValueKind::Items(kind))
            .any(|d| {
                self.fields
                    .get(d.key)
                    .is_some_and(|e| e.status == ReqStatus::NotApplicable)
            })
    }

    /// Record an entry at a path. The owner, risk and note are trimmed (blank = none).
    pub fn set(&mut self, path: &str, mut entry: ReqEntry) -> Result<(), ReqEditError> {
        let d = self.def_of(path)?;
        entry.owner = clean(entry.owner.as_deref());
        entry.risk_if_wrong = clean(entry.risk_if_wrong.as_deref());
        entry.note = clean(entry.note.as_deref());
        let mut problems = entry_problems(d, path, &entry);
        if let ValueKind::Items(kind) = d.kind {
            if entry.status == ReqStatus::NotApplicable && self.items_of(kind).next().is_some() {
                problems.push(format!(
                    "{path}: cannot be NOT_APPLICABLE while {} items exist",
                    kind.label().to_lowercase()
                ));
            }
        }
        for input in &entry.derived_from {
            if let Err(m) = self.def_of(input) {
                problems.push(format!(
                    "{path}: derived from {input}: {}",
                    edit_message(&m)
                ));
            }
        }
        if !problems.is_empty() {
            return Err(ReqEditError::Invalid(problems.join("; ")));
        }
        if self.entry(path).is_some_and(|old| old.same_content(&entry)) {
            return Err(ReqEditError::Unchanged(format!(
                "requirement {path} already has that content"
            )));
        }
        let key = entry_key(path)?;
        let slot = self
            .slot(path)
            .ok_or_else(|| ReqEditError::NotFound(format!("requirement {path}")))?;
        slot.insert(key, entry);
        Ok(())
    }

    /// Remove the entry at a path: the field is UNKNOWN again.
    pub fn clear(&mut self, path: &str) -> Result<(), ReqEditError> {
        self.def_of(path)?;
        if let Some(user) = self.referrers(path).first() {
            return Err(ReqEditError::Precondition(format!(
                "{user} is derived from {path}; change it first"
            )));
        }
        let key = entry_key(path)?;
        match self.slot(path).and_then(|s| s.remove(&key)) {
            Some(_) => Ok(()),
            None => Err(ReqEditError::Unchanged(format!(
                "requirement {path} has no recorded entry"
            ))),
        }
    }

    /// Paths whose DERIVED value names `target` (a path, or an item id for any of its fields).
    fn referrers(&self, target: &str) -> Vec<String> {
        let names = |input: &str| {
            input == target
                || (!target.contains('/')
                    && input.split_once('/').is_some_and(|(id, _)| id == target))
        };
        let mut out = vec![];
        for (path, e) in self.entries() {
            if e.derived_from.iter().any(|i| names(i.as_str())) {
                out.push(path);
            }
        }
        out
    }

    /// Create (`id` = `None`; returns the new id) or rename / re-parent an item.
    pub fn upsert_item(
        &mut self,
        id: Option<&str>,
        kind: ReqItemKind,
        name: &str,
        parent: Option<&str>,
    ) -> Result<Option<Id>, ReqEditError> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 200 {
            return Err(ReqEditError::Invalid(
                "item name must be 1 to 200 characters".into(),
            ));
        }
        let parent = parent.map(str::trim).filter(|p| !p.is_empty());
        if let Some(pid) = parent {
            if kind != ReqItemKind::Rail {
                return Err(ReqEditError::Invalid(
                    "only a rail has a parent (the rail it is generated from)".into(),
                ));
            }
            match self.items.get(pid) {
                Some(p) if p.kind == ReqItemKind::Rail => {}
                Some(_) => {
                    return Err(ReqEditError::Invalid(format!("parent {pid} is not a rail")))
                }
                None => return Err(ReqEditError::NotFound(format!("parent rail {pid}"))),
            }
            let looped =
                id.is_some_and(|me| me == pid || self.rail_ancestors(pid).iter().any(|a| a == me));
            if looped {
                return Err(ReqEditError::Invalid(format!(
                    "rail {pid} cannot be the parent: the rail tree would form a loop"
                )));
            }
        }
        let Some(id) = id else {
            if self.list_not_applicable(kind) {
                return Err(ReqEditError::Precondition(format!(
                    "the {} list is recorded NOT_APPLICABLE; clear it first",
                    kind.label().to_lowercase()
                )));
            }
            let new = new_id(kind.id_prefix());
            self.items.insert(
                new.clone(),
                ReqItem {
                    id: new.clone(),
                    kind,
                    name: name.into(),
                    parent: parent.map(String::from),
                    fields: BTreeMap::new(),
                },
            );
            return Ok(Some(new));
        };
        let item = self
            .items
            .get_mut(id)
            .ok_or_else(|| ReqEditError::NotFound(format!("requirement item {id}")))?;
        if item.kind != kind {
            return Err(ReqEditError::Invalid(format!(
                "{id} is a {}; an item's kind never changes",
                item.kind.label().to_lowercase()
            )));
        }
        let parent = parent.map(String::from);
        if item.name == name && item.parent == parent {
            return Err(ReqEditError::Unchanged(format!(
                "{id} already has that name and parent"
            )));
        }
        item.name = name.into();
        item.parent = parent;
        Ok(None)
    }

    /// Rails above `id` in the rail tree. A loop ends the walk; `id` itself is listed last when
    /// it is its own ancestor.
    fn rail_ancestors(&self, id: &str) -> Vec<String> {
        let mut out: Vec<String> = vec![];
        let mut cur = self.items.get(id).and_then(|i| i.parent.clone());
        while let Some(p) = cur {
            if out.contains(&p) {
                break;
            }
            let back_to_start = p == id;
            cur = self.items.get(&p).and_then(|i| i.parent.clone());
            out.push(p);
            if back_to_start {
                break;
            }
        }
        out
    }

    /// Remove an item. Refused while a rail is generated from it or a DERIVED value elsewhere
    /// names one of its fields.
    pub fn delete_item(&mut self, id: &str) -> Result<(), ReqEditError> {
        if !self.items.contains_key(id) {
            return Err(ReqEditError::NotFound(format!("requirement item {id}")));
        }
        if let Some(child) = self
            .items
            .values()
            .find(|i| i.parent.as_deref() == Some(id))
        {
            return Err(ReqEditError::Precondition(format!(
                "rail {} ({}) is generated from {id}; re-parent or delete it first",
                child.name, child.id
            )));
        }
        let own = format!("{id}/");
        if let Some(user) = self
            .referrers(id)
            .into_iter()
            .find(|u| !u.starts_with(&own))
        {
            return Err(ReqEditError::Precondition(format!(
                "{user} is derived from a field of {id}; change it first"
            )));
        }
        self.items.remove(id);
        Ok(())
    }

    /// Every recorded entry with its path (project fields, then items by id).
    pub fn entries(&self) -> Vec<(String, &ReqEntry)> {
        let mut out: Vec<(String, &ReqEntry)> =
            self.fields.iter().map(|(k, e)| (k.clone(), e)).collect();
        for (id, item) in &self.items {
            for (k, e) in &item.fields {
                out.push((format!("{id}/{k}"), e));
            }
        }
        out
    }

    /// Structural problems (integrity): unknown keys, entries that break their status rules,
    /// dangling references and rail-tree loops.
    pub fn problems(&self) -> Vec<String> {
        let mut e = vec![];
        for (k, entry) in &self.fields {
            match field_def(k) {
                None => e.push(format!("requirement field {k} is not in the catalogue")),
                Some(d) => {
                    e.extend(entry_problems(d, k, entry));
                    if let ValueKind::Items(kind) = d.kind {
                        if entry.status == ReqStatus::NotApplicable
                            && self.items_of(kind).next().is_some()
                        {
                            e.push(format!("{k} is NOT_APPLICABLE but items of it exist"));
                        }
                    }
                }
            }
        }
        for (id, item) in &self.items {
            if &item.id != id {
                e.push(format!("requirement item key {id} != id {}", item.id));
            }
            if item.name.trim().is_empty() {
                e.push(format!("requirement item {id} has no name"));
            }
            if let Some(p) = &item.parent {
                let ok = item.kind == ReqItemKind::Rail
                    && self
                        .items
                        .get(p)
                        .is_some_and(|x| x.kind == ReqItemKind::Rail);
                if !ok {
                    e.push(format!("requirement item {id}: parent {p} is not a rail"));
                }
                if self.rail_ancestors(id).iter().any(|a| a == id) {
                    e.push(format!("rail {id} is part of a loop in the rail tree"));
                }
            }
            for (k, entry) in &item.fields {
                let path = format!("{id}/{k}");
                match item_field_def(item.kind, k) {
                    None => e.push(format!("requirement {path} is not in the catalogue")),
                    Some(d) => e.extend(entry_problems(d, &path, entry)),
                }
            }
        }
        for (path, entry) in self.entries() {
            for input in &entry.derived_from {
                if let Err(m) = self.def_of(input) {
                    e.push(format!(
                        "{path}: derived from {input}: {}",
                        edit_message(&m)
                    ));
                }
            }
        }
        e
    }
}

fn text_problem(what: &str, s: &str) -> Option<String> {
    if s.trim().is_empty() {
        Some(format!("{what} is empty"))
    } else if s.chars().count() > MAX_TEXT {
        Some(format!("{what} is longer than {MAX_TEXT} characters"))
    } else {
        None
    }
}

fn kind_name(k: ValueKind) -> &'static str {
    match k {
        ValueKind::Number => "number",
        ValueKind::Range => "range",
        ValueKind::NumberOrRange => "number or range",
        ValueKind::Text => "text",
        ValueKind::Flag => "flag",
        ValueKind::Items(_) => "item list",
    }
}

/// Why `v` is not a valid value for `d`.
pub fn value_problem(d: &FieldDef, v: &ReqValue) -> Option<String> {
    let unit_ok = |u: ReqUnit| d.measure == Some(u.measure());
    let figure_ok = |x: f64| x.is_finite() && (x >= 0.0 || d.measure.is_some_and(Measure::signed));
    let wrong_unit = |u: ReqUnit| {
        let wanted = d.measure.map_or_else(
            || "nothing".to_string(),
            |m| format!("{m:?}").to_lowercase(),
        );
        format!("unit {} does not measure {wanted}", u.symbol())
    };
    match (d.kind, v) {
        (ValueKind::Text, ReqValue::Text { text }) => text_problem("text", text),
        (ValueKind::Flag, ReqValue::Flag { .. }) => None,
        (
            ValueKind::Number | ValueKind::NumberOrRange,
            ReqValue::Number {
                value,
                unit,
                tolerance_pct,
            },
        ) => {
            if !unit_ok(*unit) {
                Some(wrong_unit(*unit))
            } else if !figure_ok(*value) {
                Some(format!("{value} is not a valid figure here"))
            } else if tolerance_pct.is_some_and(|t| !(t.is_finite() && t >= 0.0)) {
                Some("tolerance must be a finite percentage of zero or more".into())
            } else {
                None
            }
        }
        (
            ValueKind::Range | ValueKind::NumberOrRange,
            ReqValue::Range {
                min,
                nom,
                max,
                unit,
            },
        ) => {
            let limits: Vec<f64> = [*min, *nom, *max].into_iter().flatten().collect();
            if !unit_ok(*unit) {
                Some(wrong_unit(*unit))
            } else if limits.is_empty() {
                Some("a range needs at least one of min, nom, max".into())
            } else if !limits.iter().all(|x| figure_ok(*x)) {
                Some("a range limit is not a valid figure here".into())
            } else if !limits.windows(2).all(|w| w[0] <= w[1]) {
                Some("range limits must be in order min <= nom <= max".into())
            } else {
                None
            }
        }
        _ => Some(format!("{} takes a {} value", d.key, kind_name(d.kind))),
    }
}

/// Problems of one entry against its definition and its status rules.
pub fn entry_problems(d: &FieldDef, path: &str, e: &ReqEntry) -> Vec<String> {
    let mut found: Vec<String> = vec![];
    if matches!(d.kind, ValueKind::Items(_))
        && !matches!(e.status, ReqStatus::Unknown | ReqStatus::NotApplicable)
    {
        found.push(
            "an item list is filled by adding items; only UNKNOWN or NOT_APPLICABLE can be recorded"
                .into(),
        );
    }
    match (e.status, &e.value) {
        (ReqStatus::Unknown | ReqStatus::NotApplicable, Some(_)) => found.push(format!(
            "{} carries no value (a missing value is never a number)",
            e.status.as_str()
        )),
        (ReqStatus::NeedsConfirmation | ReqStatus::Confirmed | ReqStatus::Derived, None) => {
            found.push(format!("{} needs a value", e.status.as_str()))
        }
        (_, Some(v)) => found.extend(value_problem(d, v)),
        _ => {}
    }
    match e.status {
        ReqStatus::NeedsConfirmation | ReqStatus::Confirmed if e.source.is_none() => {
            found.push(format!("{} needs a source", e.status.as_str()))
        }
        ReqStatus::Confirmed if e.owner.is_none() => {
            found.push("CONFIRMED needs the owner who confirmed it".into())
        }
        ReqStatus::NeedsConfirmation if e.owner.is_none() => {
            found.push("NEEDS_CONFIRMATION needs the owner who must confirm it".into())
        }
        _ => {}
    }
    match (e.status, &e.risk_if_wrong) {
        (ReqStatus::NeedsConfirmation, None) => found.push(
            "NEEDS_CONFIRMATION needs risk_if_wrong: what goes wrong if the value is wrong".into(),
        ),
        (ReqStatus::NeedsConfirmation, Some(_)) | (_, None) => {}
        (_, Some(_)) => found.push("only a NEEDS_CONFIRMATION value carries risk_if_wrong".into()),
    }
    match e.status {
        ReqStatus::NotApplicable if !d.na_allowed => {
            found.push("this field always applies; it cannot be NOT_APPLICABLE".into())
        }
        ReqStatus::NotApplicable if e.note.is_none() => {
            found.push("NOT_APPLICABLE needs a reason in the note".into())
        }
        _ => {}
    }
    if e.status == ReqStatus::Derived {
        if e.derived_from.is_empty() {
            found.push("DERIVED needs the fields it is derived from".into());
        }
    } else if !e.derived_from.is_empty() {
        found.push("only a DERIVED value names derived_from".into());
    }
    if e.derived_from.iter().any(|i| i == path) {
        found.push("a value cannot be derived from itself".into());
    }
    if let Some(s) = &e.source {
        found.extend(text_problem("source reference", &s.reference));
    }
    found.extend(e.owner.as_deref().and_then(|s| text_problem("owner", s)));
    found.extend(
        e.risk_if_wrong
            .as_deref()
            .and_then(|s| text_problem("risk_if_wrong", s)),
    );
    found.extend(e.note.as_deref().and_then(|s| text_problem("note", s)));
    found.into_iter().map(|m| format!("{path}: {m}")).collect()
}

// ------------------------------------------------------------------ evaluation

/// How far a value can be relied on. Ordered: a later level is stronger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// UNKNOWN or no entry.
    Missing,
    /// NEEDS_CONFIRMATION, or DERIVED from a value that is not firm.
    Provisional,
    /// CONFIRMED, NOT_APPLICABLE, DERIVED from firm values, or an item list with items.
    Firm,
}

/// A downstream engineering operation that depends on requirement values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Architecture,
    PartSelection,
    SchematicCapture,
    PowerSizing,
    BoardOutline,
    Placement,
    Routing,
    SafetySpacing,
    ManufacturingRelease,
}

/// What an operation needs: project fields and the fields of every item of a kind, each at a
/// minimum level. `all_fields` = every field of the record must be firm.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct OperationDef {
    pub operation: Operation,
    pub label: &'static str,
    pub needs: &'static [(&'static str, Level)],
    pub item_needs: &'static [(ReqItemKind, &'static str, Level)],
    pub all_fields: bool,
}

const PROV: Level = Level::Provisional;
const FIRM: Level = Level::Firm;
const RAIL: ReqItemKind = ReqItemKind::Rail;
const IFC: ReqItemKind = ReqItemKind::Interface;

/// The operation dependency table, in workflow order (ADR-0057).
pub const OPERATIONS: &[OperationDef] = &[
    OperationDef {
        operation: Operation::Architecture,
        label: "Architecture and block plan",
        needs: &[
            ("function.description", PROV),
            ("function.performance", PROV),
            ("function.operating_modes", PROV),
            ("supply.sources", PROV),
            ("supply.input_voltage", PROV),
            ("supply.rails", PROV),
            ("environment.operating_temp", PROV),
            ("safety.max_working_voltage", PROV),
            ("safety.isolation", PROV),
            ("manufacturing.quantity", PROV),
        ],
        item_needs: &[(RAIL, "voltage", PROV)],
        all_fields: false,
    },
    OperationDef {
        operation: Operation::PartSelection,
        label: "Part selection and BOM",
        needs: &[
            ("function.description", PROV),
            ("supply.input_voltage", PROV),
            ("supply.rails", PROV),
            ("power_budget.peak_current", PROV),
            ("power_budget.average_current", PROV),
            ("environment.operating_temp", PROV),
            ("mechanical.max_height_top", PROV),
            ("mechanical.max_height_bottom", PROV),
            ("manufacturing.quantity", PROV),
            ("manufacturing.assembler", PROV),
            ("manufacturing.assembly_process", PROV),
            ("cost.bom_target", PROV),
            ("supply_chain.lifecycle", PROV),
            ("interfaces.list", PROV),
        ],
        item_needs: &[
            (RAIL, "voltage", PROV),
            (RAIL, "current_peak", PROV),
            (RAIL, "current_continuous", PROV),
            (IFC, "protocol", PROV),
            (IFC, "connector", PROV),
        ],
        all_fields: false,
    },
    OperationDef {
        operation: Operation::SchematicCapture,
        label: "Schematic capture",
        needs: &[
            ("supply.rails", PROV),
            ("supply.reverse_polarity", PROV),
            ("interfaces.list", PROV),
        ],
        item_needs: &[
            (RAIL, "voltage", PROV),
            (RAIL, "loads", PROV),
            (IFC, "protocol", PROV),
            (IFC, "voltage_level", PROV),
            (IFC, "esd_level", PROV),
            (IFC, "pinout", PROV),
        ],
        all_fields: false,
    },
    OperationDef {
        operation: Operation::PowerSizing,
        label: "Power, protection and trace sizing",
        needs: &[
            ("supply.input_voltage", FIRM),
            ("supply.rails", FIRM),
            ("power_budget.peak_current", FIRM),
            ("environment.operating_temp", FIRM),
        ],
        item_needs: &[
            (RAIL, "voltage", FIRM),
            (RAIL, "current_peak", FIRM),
            (RAIL, "current_continuous", FIRM),
        ],
        all_fields: false,
    },
    OperationDef {
        operation: Operation::BoardOutline,
        label: "Board outline and mechanical fit",
        needs: &[
            ("mechanical.max_length", FIRM),
            ("mechanical.max_width", FIRM),
            ("mechanical.board_thickness", FIRM),
            ("mechanical.mounting", FIRM),
            ("mechanical.enclosure", FIRM),
        ],
        item_needs: &[],
        all_fields: false,
    },
    OperationDef {
        operation: Operation::Placement,
        label: "Placement",
        needs: &[
            ("mechanical.max_length", FIRM),
            ("mechanical.max_width", FIRM),
            ("mechanical.max_height_top", FIRM),
            ("mechanical.max_height_bottom", FIRM),
            ("mechanical.connector_positions", PROV),
            ("interfaces.list", PROV),
        ],
        item_needs: &[(IFC, "connector", PROV)],
        all_fields: false,
    },
    OperationDef {
        operation: Operation::Routing,
        label: "Routing",
        needs: &[
            ("supply.rails", FIRM),
            ("safety.max_working_voltage", FIRM),
            ("safety.isolation", FIRM),
            ("interfaces.list", PROV),
        ],
        item_needs: &[(RAIL, "current_continuous", FIRM), (IFC, "data_rate", PROV)],
        all_fields: false,
    },
    OperationDef {
        operation: Operation::SafetySpacing,
        label: "Clearance, creepage and isolation",
        needs: &[
            ("safety.max_working_voltage", FIRM),
            ("safety.isolation", FIRM),
            ("safety.standards", FIRM),
            ("safety.sector", FIRM),
            ("environment.altitude", FIRM),
            ("environment.condensation", FIRM),
            ("environment.ingress", FIRM),
        ],
        item_needs: &[],
        all_fields: false,
    },
    OperationDef {
        operation: Operation::ManufacturingRelease,
        label: "Manufacturing release",
        needs: &[],
        item_needs: &[],
        all_fields: true,
    },
];

/// Counts over a set of fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Completeness {
    pub total: usize,
    pub unknown: usize,
    pub needs_confirmation: usize,
    pub confirmed: usize,
    pub derived: usize,
    pub not_applicable: usize,
    pub missing: usize,
    pub provisional: usize,
    pub firm: usize,
    /// Fields with a value or a NOT_APPLICABLE decision, in whole percent (rounded down).
    pub percent_filled: u32,
    /// Firm fields in whole percent (rounded down).
    pub percent_firm: u32,
}

impl Completeness {
    fn add(&mut self, status: ReqStatus, level: Level) {
        self.total += 1;
        match status {
            ReqStatus::Unknown => self.unknown += 1,
            ReqStatus::NeedsConfirmation => self.needs_confirmation += 1,
            ReqStatus::Confirmed => self.confirmed += 1,
            ReqStatus::Derived => self.derived += 1,
            ReqStatus::NotApplicable => self.not_applicable += 1,
        }
        match level {
            Level::Missing => self.missing += 1,
            Level::Provisional => self.provisional += 1,
            Level::Firm => self.firm += 1,
        }
    }

    fn finish(mut self) -> Self {
        let total = self.total;
        // Floored percentage; an empty total (no fields) is 0 %, never a division by zero.
        let pct = |n: usize| (n * 100).checked_div(total).map_or(0, |p| p as u32);
        self.percent_filled = pct(total - self.missing);
        self.percent_firm = pct(self.firm);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SectionReport {
    pub section: Section,
    pub label: &'static str,
    pub completeness: Completeness,
}

/// The evaluated state of one field.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FieldReport {
    pub path: String,
    pub label: String,
    pub section: Section,
    /// The recorded status; UNKNOWN when nothing is recorded. An item list with items reports
    /// DERIVED (from its items, listed in `derived_from`).
    pub status: ReqStatus,
    pub level: Level,
    /// Whether anything is recorded at this path.
    pub recorded: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<ReqValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<ReqSource>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub derived_from: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub risk_if_wrong: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at_ms: Option<u64>,
    pub material: bool,
    pub na_allowed: bool,
}

/// A field that keeps an operation from being ready.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Blocker {
    pub path: String,
    pub status: ReqStatus,
    pub level: Level,
    pub needs: Level,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OperationReport {
    pub operation: Operation,
    pub label: &'static str,
    pub ready: bool,
    pub blocked_by: Vec<Blocker>,
}

/// A targeted question about a missing value that changes architecture or safety.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Question {
    pub path: String,
    pub question: String,
    /// Operations the missing value blocks.
    pub blocks: Vec<Operation>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RequirementsReport {
    pub completeness: Completeness,
    pub sections: Vec<SectionReport>,
    pub fields: Vec<FieldReport>,
    pub operations: Vec<OperationReport>,
    pub questions: Vec<Question>,
    /// Consistency problems the record has but that do not refuse it (a DERIVED value from an
    /// UNKNOWN input, a loop of derived values).
    pub issues: Vec<String>,
    pub not_implemented: Vec<&'static str>,
    pub limitations: Vec<&'static str>,
}

pub const NOT_IMPLEMENTED: &[&str] = &[
    "cross-field consistency (rail currents against the power budget, rail voltages against the input range, battery life against capacity)",
    "enforcement: no engine refuses an operation on this report yet; operation readiness is advisory",
    "requirement-to-constraint compilation (design profile) and requirement-to-block-to-net traceability",
    "scoped gates (a gate that passes for one variant or scope, such as a low-voltage prototype, while another stays blocked)",
    "domain-required fields from the classification (for example, NOT_APPLICABLE refused for safety under a mains or high-voltage classification)",
    "a deterministic evaluator that writes DERIVED values; today a person records them with their inputs",
    "folding the harness design-flow spec (design-flow/spec.json) into this record",
];

pub const LIMITATIONS: &[&str] = &[
    "completeness counts presence and status only: a CONFIRMED value is as right as its source",
    "the operation table is EAK's own engineering-order dependency table (ADR-0057), not a standard",
    "an item list (rails, interfaces) is satisfied by one item; whether the list is complete is the owner's call",
];

fn status_level(status: ReqStatus) -> Level {
    match status {
        ReqStatus::Unknown => Level::Missing,
        ReqStatus::NeedsConfirmation => Level::Provisional,
        // DERIVED is resolved through its inputs in `Ctx::level_of`.
        ReqStatus::Confirmed | ReqStatus::NotApplicable | ReqStatus::Derived => Level::Firm,
    }
}

struct Ctx<'a> {
    set: &'a RequirementSet,
    issues: BTreeSet<String>,
}

impl<'a> Ctx<'a> {
    /// Level of a path; a DERIVED value is as firm as its weakest input.
    fn level_of(&mut self, path: &str, stack: &mut Vec<String>) -> Level {
        let set: &'a RequirementSet = self.set;
        if let Ok(ReqPath::Field(k)) = parse_path(path) {
            if let Some(ValueKind::Items(kind)) = field_def(k).map(|d| d.kind) {
                if set.items_of(kind).next().is_some() {
                    return Level::Firm;
                }
            }
        }
        let Some(e) = set.entry(path) else {
            return Level::Missing;
        };
        if e.status != ReqStatus::Derived {
            return status_level(e.status);
        }
        if stack.iter().any(|p| p == path) {
            self.issues.insert(format!(
                "{path}: derived values form a loop ({} -> {path})",
                stack.join(" -> ")
            ));
            return Level::Provisional;
        }
        stack.push(path.to_string());
        let mut level = Level::Firm;
        for input in &e.derived_from {
            let l = self.level_of(input, stack);
            if l == Level::Missing {
                self.issues
                    .insert(format!("{path} is DERIVED from {input}, which is UNKNOWN"));
            }
            if l < Level::Firm {
                level = Level::Provisional;
            }
            // Stale: an input was recorded after the derived value, so it must be derived again.
            if let Some(changed) = set
                .entry(input)
                .filter(|i| i.updated_revision > e.updated_revision)
            {
                self.issues.insert(format!(
                    "{path} is stale: {input} changed at revision {} after it was derived at revision {}; derive it again",
                    changed.updated_revision, e.updated_revision
                ));
                level = Level::Provisional;
            }
        }
        stack.pop();
        level
    }

    fn field(&mut self, path: String, label: String, d: &FieldDef) -> FieldReport {
        let set: &'a RequirementSet = self.set;
        let level = self.level_of(&path, &mut vec![]);
        let list_items: Vec<String> = match d.kind {
            ValueKind::Items(kind) => set.items_of(kind).map(|i| i.id.clone()).collect(),
            _ => vec![],
        };
        let e = set.entry(&path);
        let (status, derived_from) = if list_items.is_empty() {
            (
                e.map_or(ReqStatus::Unknown, |e| e.status),
                e.map(|e| e.derived_from.clone()).unwrap_or_default(),
            )
        } else {
            (ReqStatus::Derived, list_items)
        };
        FieldReport {
            path,
            label,
            section: d.section,
            status,
            level,
            recorded: e.is_some(),
            value: e.and_then(|e| e.value.clone()),
            source: e.and_then(|e| e.source.clone()),
            derived_from,
            owner: e.and_then(|e| e.owner.clone()),
            risk_if_wrong: e.and_then(|e| e.risk_if_wrong.clone()),
            note: e.and_then(|e| e.note.clone()),
            updated_revision: e.map(|e| e.updated_revision),
            updated_at_ms: e.map(|e| e.updated_at_ms),
            material: d.material,
            na_allowed: d.na_allowed,
        }
    }
}

/// Evaluate the record: completeness, operation readiness, questions. Pure and deterministic.
pub fn evaluate(set: &RequirementSet) -> RequirementsReport {
    let mut cx = Ctx {
        set,
        issues: BTreeSet::new(),
    };
    let mut fields = vec![];
    let mut defs: Vec<&'static FieldDef> = vec![];
    for d in FIELDS {
        fields.push(cx.field(d.key.to_string(), d.label.to_string(), d));
        defs.push(d);
    }
    for item in set.items.values() {
        for d in item.kind.fields() {
            let label = format!("{} {}: {}", item.kind.label(), item.name, d.label);
            fields.push(cx.field(format!("{}/{}", item.id, d.key), label, d));
            defs.push(d);
        }
    }

    let mut all = Completeness::default();
    let mut by_section: BTreeMap<Section, Completeness> = BTreeMap::new();
    for f in &fields {
        all.add(f.status, f.level);
        by_section
            .entry(f.section)
            .or_default()
            .add(f.status, f.level);
    }
    let sections = Section::ALL
        .iter()
        .map(|s| SectionReport {
            section: *s,
            label: s.label(),
            completeness: by_section.get(s).copied().unwrap_or_default().finish(),
        })
        .collect();

    let index: BTreeMap<&str, &FieldReport> = fields.iter().map(|f| (f.path.as_str(), f)).collect();
    let mut operations = vec![];
    for op in OPERATIONS {
        let mut needs: Vec<(String, Level)> = vec![];
        if op.all_fields {
            needs.extend(fields.iter().map(|f| (f.path.clone(), Level::Firm)));
        }
        needs.extend(op.needs.iter().map(|(k, l)| (k.to_string(), *l)));
        for (kind, key, l) in op.item_needs {
            for item in set.items_of(*kind) {
                needs.push((format!("{}/{key}", item.id), *l));
            }
        }
        let mut seen = BTreeSet::new();
        let mut blocked_by = vec![];
        for (path, need) in needs {
            if !seen.insert(path.clone()) {
                continue;
            }
            let (status, level) = index
                .get(path.as_str())
                .map_or((ReqStatus::Unknown, Level::Missing), |f| {
                    (f.status, f.level)
                });
            if level < need {
                blocked_by.push(Blocker {
                    path,
                    status,
                    level,
                    needs: need,
                });
            }
        }
        operations.push(OperationReport {
            operation: op.operation,
            label: op.label,
            ready: blocked_by.is_empty(),
            blocked_by,
        });
    }

    let mut questions = vec![];
    for (f, d) in fields.iter().zip(&defs) {
        if f.level != Level::Missing || !d.material {
            continue;
        }
        let blocks: Vec<Operation> = operations
            .iter()
            .filter(|o| o.blocked_by.iter().any(|b| b.path == f.path))
            .map(|o| o.operation)
            .collect();
        let question = match parse_path(&f.path) {
            Ok(ReqPath::Item { id, .. }) => {
                let name = set.items.get(id).map_or(id, |i| i.name.as_str());
                format!("{name}: {}", d.question)
            }
            _ => d.question.to_string(),
        };
        questions.push(Question {
            path: f.path.clone(),
            question,
            blocks,
        });
    }

    RequirementsReport {
        completeness: all.finish(),
        sections,
        fields,
        operations,
        questions,
        issues: cx.issues.into_iter().collect(),
        not_implemented: NOT_IMPLEMENTED.to_vec(),
        limitations: LIMITATIONS.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(status: ReqStatus, value: Option<ReqValue>) -> ReqEntry {
        let source =
            matches!(status, ReqStatus::NeedsConfirmation | ReqStatus::Confirmed).then(|| {
                ReqSource {
                    kind: SourceKind::Client,
                    reference: "kick-off call 2026-11-20".into(),
                }
            });
        ReqEntry {
            status,
            value,
            source,
            derived_from: vec![],
            owner: matches!(status, ReqStatus::NeedsConfirmation | ReqStatus::Confirmed)
                .then(|| "A. Engineer".to_string()),
            risk_if_wrong: (status == ReqStatus::NeedsConfirmation)
                .then(|| "the regulator is re-selected".to_string()),
            note: (status == ReqStatus::NotApplicable).then(|| "no battery".to_string()),
            updated_at_ms: 1,
            updated_revision: 1,
        }
    }

    fn volts(min: f64, nom: f64, max: f64) -> ReqValue {
        ReqValue::Range {
            min: Some(min),
            nom: Some(nom),
            max: Some(max),
            unit: ReqUnit::Volt,
        }
    }

    fn amps(value: f64) -> ReqValue {
        ReqValue::Number {
            value,
            unit: ReqUnit::Ampere,
            tolerance_pct: None,
        }
    }

    fn op(r: &RequirementsReport, which: Operation) -> &OperationReport {
        r.operations.iter().find(|o| o.operation == which).unwrap()
    }

    #[test]
    fn catalogue_is_consistent() {
        let mut keys = BTreeSet::new();
        for d in FIELDS {
            assert!(keys.insert(d.key), "duplicate field {}", d.key);
            assert!(!d.key.contains('/'), "{}", d.key);
            assert_eq!(
                d.measure.is_some(),
                matches!(
                    d.kind,
                    ValueKind::Number | ValueKind::Range | ValueKind::NumberOrRange
                ),
                "{}: a quantity has a measure and nothing else does",
                d.key
            );
            assert!(d.question.ends_with('?'), "{}", d.key);
        }
        for kind in [ReqItemKind::Rail, ReqItemKind::Interface] {
            let mut keys = BTreeSet::new();
            for d in kind.fields() {
                assert!(keys.insert(d.key), "duplicate {kind:?} field {}", d.key);
                assert!(!matches!(d.kind, ValueKind::Items(_)));
                assert!(d.question.ends_with('?'), "{}", d.key);
            }
            assert!(
                FIELDS.iter().any(|d| d.kind == ValueKind::Items(kind)),
                "{kind:?} has a list field"
            );
        }
        for s in Section::ALL {
            assert!(FIELDS.iter().any(|d| d.section == s), "{s:?} has no field");
        }
        // The sections are DESK's ten requirement dimensions (ADR-0057).
        let names: Vec<_> = Section::ALL
            .iter()
            .map(|s| serde_json::to_value(s).unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "function",
                "supply",
                "power_budget",
                "interfaces",
                "mechanical",
                "environmental",
                "safety_regulatory",
                "manufacturing_test",
                "cost_supply",
                "lifecycle_firmware"
            ]
        );
        for o in OPERATIONS {
            for (k, _) in o.needs {
                assert!(
                    field_def(k).is_some(),
                    "{:?} needs unknown {k}",
                    o.operation
                );
            }
            for (kind, k, _) in o.item_needs {
                assert!(item_field_def(*kind, k).is_some(), "{:?} {k}", o.operation);
            }
        }
    }

    #[test]
    fn unit_and_status_symbols_are_the_serde_names() {
        for u in ReqUnit::ALL {
            assert_eq!(serde_json::to_value(u).unwrap(), u.symbol());
            let back: ReqUnit = serde_json::from_value(u.symbol().into()).unwrap();
            assert_eq!(back, u);
        }
        for s in ReqStatus::ALL {
            assert_eq!(serde_json::to_value(s).unwrap(), s.as_str());
        }
    }

    #[test]
    fn an_empty_record_is_all_unknown_and_blocks_everything() {
        let r = evaluate(&RequirementSet::default());
        assert_eq!(r.completeness.total, FIELDS.len());
        assert_eq!(r.completeness.unknown, FIELDS.len());
        assert_eq!(r.completeness.missing, FIELDS.len());
        assert_eq!(r.completeness.percent_filled, 0);
        assert_eq!(r.completeness.percent_firm, 0);
        assert!(r.operations.iter().all(|o| !o.ready));
        // Missing is never zero: no field reports a value it was not given.
        assert!(r.fields.iter().all(|f| !f.recorded && f.value.is_none()));
        assert_eq!(
            op(&r, Operation::ManufacturingRelease).blocked_by.len(),
            FIELDS.len()
        );
        // Material fields are asked about, with the operations they block; others are not.
        assert!(r.questions.iter().any(
            |q| q.path == "supply.input_voltage" && q.blocks.contains(&Operation::PowerSizing)
        ));
        assert!(!r.questions.iter().any(|q| q.path == "lifecycle.variants"));
        assert_eq!(r.sections.len(), Section::ALL.len());
        assert!(!r.not_implemented.is_empty() && !r.limitations.is_empty());
    }

    #[test]
    fn status_rules_refuse_invented_values() {
        let mut s = RequirementSet::default();
        let path = "supply.input_voltage";
        let wrong_unit = ReqValue::Range {
            min: Some(1.0),
            nom: None,
            max: Some(2.0),
            unit: ReqUnit::Ampere,
        };
        for bad in [
            // UNKNOWN with a value, a value without a source, CONFIRMED without owner or value.
            entry(ReqStatus::Unknown, Some(volts(4.5, 5.0, 5.5))),
            ReqEntry {
                source: None,
                ..entry(ReqStatus::NeedsConfirmation, Some(volts(4.5, 5.0, 5.5)))
            },
            ReqEntry {
                owner: None,
                ..entry(ReqStatus::Confirmed, Some(volts(4.5, 5.0, 5.5)))
            },
            entry(ReqStatus::Confirmed, None),
            // A working value without the owner who confirms it or its risk if wrong; a risk on
            // a value that is not provisional.
            ReqEntry {
                owner: None,
                ..entry(ReqStatus::NeedsConfirmation, Some(volts(4.5, 5.0, 5.5)))
            },
            ReqEntry {
                risk_if_wrong: None,
                ..entry(ReqStatus::NeedsConfirmation, Some(volts(4.5, 5.0, 5.5)))
            },
            ReqEntry {
                risk_if_wrong: Some("none".into()),
                ..entry(ReqStatus::Confirmed, Some(volts(4.5, 5.0, 5.5)))
            },
            // Wrong unit, wrong shape, disordered range.
            entry(ReqStatus::NeedsConfirmation, Some(wrong_unit)),
            entry(
                ReqStatus::NeedsConfirmation,
                Some(ReqValue::Text { text: "5 V".into() }),
            ),
            entry(ReqStatus::NeedsConfirmation, Some(volts(5.5, 5.0, 4.5))),
            // The input voltage always applies.
            entry(ReqStatus::NotApplicable, None),
        ] {
            assert!(
                matches!(s.set(path, bad.clone()), Err(ReqEditError::Invalid(_))),
                "{bad:?} was accepted"
            );
        }
        assert!(s.is_empty());
        assert!(matches!(
            s.set("supply.no_such_field", entry(ReqStatus::Unknown, None)),
            Err(ReqEditError::NotFound(_))
        ));
        let good = entry(ReqStatus::Confirmed, Some(volts(4.5, 5.0, 5.5)));
        s.set(path, good.clone()).unwrap();
        assert_eq!(
            s.set(path, good),
            Err(ReqEditError::Unchanged(format!(
                "requirement {path} already has that content"
            )))
        );
        assert!(s.problems().is_empty());
    }

    #[test]
    fn rails_form_a_tree_and_gate_power_sizing() {
        let mut s = RequirementSet::default();
        let vin = s
            .upsert_item(None, ReqItemKind::Rail, "VIN", None)
            .unwrap()
            .unwrap();
        let v3 = s
            .upsert_item(None, ReqItemKind::Rail, "3V3", Some(vin.as_str()))
            .unwrap()
            .unwrap();
        // A loop, a parent for an interface and a change of kind are refused.
        assert!(s
            .upsert_item(
                Some(vin.as_str()),
                ReqItemKind::Rail,
                "VIN",
                Some(v3.as_str())
            )
            .is_err());
        let usb = s
            .upsert_item(None, ReqItemKind::Interface, "USB", None)
            .unwrap()
            .unwrap();
        assert!(s
            .upsert_item(
                Some(usb.as_str()),
                ReqItemKind::Interface,
                "USB",
                Some(vin.as_str())
            )
            .is_err());
        assert!(s
            .upsert_item(Some(usb.as_str()), ReqItemKind::Rail, "USB", None)
            .is_err());
        // The list fields are satisfied by their items.
        let r = evaluate(&s);
        let rails = r.fields.iter().find(|f| f.path == "supply.rails").unwrap();
        assert_eq!(
            (rails.status, rails.level),
            (ReqStatus::Derived, Level::Firm)
        );
        assert_eq!(rails.derived_from.len(), 2);
        // Power sizing waits for each rail's firm voltage and currents.
        assert!(op(&r, Operation::PowerSizing)
            .blocked_by
            .iter()
            .any(|b| b.path == format!("{v3}/current_peak") && b.level == Level::Missing));
        // A provisional value unblocks provisional work, not sizing.
        let p = format!("{v3}/voltage");
        s.set(
            &p,
            entry(ReqStatus::NeedsConfirmation, Some(volts(3.2, 3.3, 3.4))),
        )
        .unwrap();
        let r = evaluate(&s);
        assert!(!op(&r, Operation::Architecture)
            .blocked_by
            .iter()
            .any(|b| b.path == p));
        let b = op(&r, Operation::PowerSizing)
            .blocked_by
            .iter()
            .find(|b| b.path == p)
            .unwrap();
        assert_eq!((b.level, b.needs), (Level::Provisional, Level::Firm));
        // A rail other rails are generated from cannot be deleted; a leaf can.
        assert!(matches!(
            s.delete_item(&vin),
            Err(ReqEditError::Precondition(_))
        ));
        s.delete_item(&v3).unwrap();
        s.delete_item(&vin).unwrap();
        assert!(s.problems().is_empty());
    }

    #[test]
    fn derived_values_are_only_as_firm_as_their_inputs() {
        let mut s = RequirementSet::default();
        let derived = ReqEntry {
            derived_from: vec![
                "power_budget.peak_current".into(),
                "supply.input_voltage".into(),
            ],
            ..entry(
                ReqStatus::Derived,
                Some(ReqValue::Number {
                    value: 2.75,
                    unit: ReqUnit::Watt,
                    tolerance_pct: None,
                }),
            )
        };
        s.set("power_budget.total_power", derived).unwrap();
        let level = |s: &RequirementSet| {
            let r = evaluate(s);
            let f = r
                .fields
                .iter()
                .find(|f| f.path == "power_budget.total_power")
                .unwrap()
                .level;
            (f, r.issues)
        };
        let (l, issues) = level(&s);
        assert_eq!(l, Level::Provisional);
        assert_eq!(issues.len(), 2, "{issues:?}");
        s.set(
            "supply.input_voltage",
            entry(ReqStatus::Confirmed, Some(volts(4.5, 5.0, 5.5))),
        )
        .unwrap();
        s.set(
            "power_budget.peak_current",
            entry(ReqStatus::Confirmed, Some(amps(0.5))),
        )
        .unwrap();
        let (l, issues) = level(&s);
        assert_eq!(l, Level::Firm);
        assert!(issues.is_empty(), "{issues:?}");
        // An input recorded after the derived value makes it stale until it is derived again.
        let later = ReqEntry {
            updated_revision: 2,
            ..entry(ReqStatus::Confirmed, Some(amps(0.6)))
        };
        s.set("power_budget.peak_current", later).unwrap();
        let (l, issues) = level(&s);
        assert_eq!(l, Level::Provisional);
        assert!(issues.iter().any(|i| i.contains("is stale")), "{issues:?}");
        let again = ReqEntry {
            updated_revision: 3,
            ..s.entry("power_budget.total_power").unwrap().clone()
        };
        s.fields.insert("power_budget.total_power".into(), again);
        let (l, issues) = level(&s);
        assert_eq!(l, Level::Firm);
        assert!(issues.is_empty(), "{issues:?}");
        // An input still in use cannot be cleared; an input that does not exist is refused.
        assert!(matches!(
            s.clear("supply.input_voltage"),
            Err(ReqEditError::Precondition(_))
        ));
        let bad = ReqEntry {
            derived_from: vec!["rail_gone/voltage".into()],
            ..entry(ReqStatus::Derived, Some(amps(1.0)))
        };
        assert!(s.set("power_budget.average_current", bad).is_err());
    }

    #[test]
    fn not_applicable_needs_a_reason_and_counts_as_decided() {
        let mut s = RequirementSet::default();
        let no_reason = ReqEntry {
            note: None,
            ..entry(ReqStatus::NotApplicable, None)
        };
        assert!(s.set("power_budget.battery_life", no_reason).is_err());
        s.set(
            "power_budget.battery_life",
            entry(ReqStatus::NotApplicable, None),
        )
        .unwrap();
        s.set("interfaces.list", entry(ReqStatus::NotApplicable, None))
            .unwrap();
        // No interface can be added while the list is declared not applicable.
        assert!(matches!(
            s.upsert_item(None, ReqItemKind::Interface, "UART", None),
            Err(ReqEditError::Precondition(_))
        ));
        let r = evaluate(&s);
        assert_eq!(r.completeness.not_applicable, 2);
        assert_eq!(r.completeness.firm, 2);
        assert!(r.completeness.percent_filled > 0);
        // An item list only records UNKNOWN or NOT_APPLICABLE.
        let listed = entry(
            ReqStatus::NeedsConfirmation,
            Some(ReqValue::Text { text: "VIN".into() }),
        );
        assert!(s.set("supply.rails", listed).is_err());
    }

    #[test]
    fn evaluation_is_deterministic_and_the_record_round_trips() {
        let mut s = RequirementSet::default();
        let v = s
            .upsert_item(None, ReqItemKind::Rail, "5V", None)
            .unwrap()
            .unwrap();
        s.set(
            &format!("{v}/voltage"),
            entry(ReqStatus::Confirmed, Some(volts(4.75, 5.0, 5.25))),
        )
        .unwrap();
        let a = serde_json::to_string(&evaluate(&s)).unwrap();
        let b = serde_json::to_string(&evaluate(&s.clone())).unwrap();
        assert_eq!(a, b);
        assert!(a.contains("\"CONFIRMED\""));
        let json = serde_json::to_string(&s).unwrap();
        let back: RequirementSet = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }
}
