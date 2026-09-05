use crate::dom::DomTree;
use bytes::Bytes;
use compact_str::CompactString;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Hidden,
    Password,
    Other,
}

impl FieldKind {
    #[inline(always)]
    pub fn from_type_attr(t: &str) -> Self {
        match t {
            "hidden" => Self::Hidden,
            "password" => Self::Password,
            "text" => Self::Text,
            _ => Self::Other,
        }
    }
}

pub struct FormData {
    pub name: CompactString,
    pub value: Option<CompactString>,
    pub kind: FieldKind,
}

pub struct FieldData {
    pub tag: CompactString,
    pub name: CompactString,
    pub value: Option<CompactString>,
    pub kind: FieldKind,
    pub hidden: bool,
    pub id: Option<CompactString>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeType {
    Turnstile,
    DataDome,
    CfManaged,
    HCaptcha,
    ReCaptcha,
    Unknown,
}

impl ChallengeType {
    pub fn from_pattern_index(idx: usize) -> Self {
        match idx {
            0 | 3 => Self::Turnstile,
            1 => Self::DataDome,
            2 => Self::CfManaged,
            4 => Self::HCaptcha,
            5 => Self::ReCaptcha,
            _ => Self::Unknown,
        }
    }
}

pub struct Form {
    pub action: Option<CompactString>,
    pub method: CompactString,
    pub fields: SmallVec<[FormData; 8]>,
}

pub struct NextData {
    pub page: CompactString,
    pub build_id: CompactString,
    pub query: Option<CompactString>,
}

pub struct PageData {
    pub title: Option<CompactString>,
    pub meta_description: Option<CompactString>,
    pub forms: SmallVec<[Form; 4]>,
    pub fields: SmallVec<[FieldData; 32]>,
    pub tokens: SmallVec<[CompactString; 8]>,
    pub script_srcs: SmallVec<[CompactString; 12]>,
    pub inline_count: usize,
    pub challenge: Option<Bytes>,
    pub challenge_markers: SmallVec<[(CompactString, ChallengeType); 4]>,
    pub challenge_script_url: Option<CompactString>,
    pub telemetry_route: Option<crate::telemetry::TelemetryRoute>,
    pub dom: DomTree,
    pub extracted: std::collections::BTreeMap<CompactString, CompactString>,
    pub next_data: Option<NextData>,
    pub bytes_fed: u64,
    pub utf8_bad_chunks: u32,
    pub truncated: bool,
    pub html_head: Option<SmallVec<[u8; 2048]>>,
    pub parse_errors: u32,
}

impl PageData {
    pub fn first_token(&self) -> Option<&str> {
        self.tokens.first().map(|t| t.as_str())
    }

    pub fn first_form(&self) -> Option<&Form> {
        self.forms.first()
    }
}