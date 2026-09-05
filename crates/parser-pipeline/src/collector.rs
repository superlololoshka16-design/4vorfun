use crate::dom::{
    DomTree, OpenStack, TagInterner, AttrNameInterner, TAG_UNKNOWN, tags,
};
use crate::nextdata::parse_next_data;
use crate::probe::is_challenge;
use crate::scratch::{AttrEv, Ev};
use crate::types::{FieldData, FieldKind, Form, FormData, NextData, PageData};
use bytes::Bytes;
use compact_str::CompactString;
use smallvec::SmallVec;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub byte_brake: u64,
    pub script_cap: usize,
    pub next_data_cap: usize,
    pub max_forms: usize,
    pub max_fields: usize,
    pub max_tokens: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            byte_brake: 4 * 1024 * 1024,
            script_cap: 256 * 1024,
            next_data_cap: 2 * 1024 * 1024,
            max_forms: 16,
            max_fields: 32,
            max_tokens: 8,
        }
    }
}

enum ScriptKind {
    Inline,
    NextData,
}

struct ScriptCapture {
    kind: ScriptKind,
    buf: SmallVec<[u8; 2048]>,
    over: bool,
}

struct FormBuilder {
    action: Option<CompactString>,
    method: CompactString,
    fields: SmallVec<[FormData; 8]>,
}

const TAG_HTML: u16 = 49;
const TAG_HEAD: u16 = 45;
const TAG_BODY: u16 = 13;
const TAG_A: u16 = 1;
const TAG_LABEL: u16 = 56;
const TAG_IFRAME: u16 = 51;
const TAG_NOSCRIPT: u16 = 67;
const TAG_TEMPLATE: u16 = 100;
const ATTR_ID_ID: u16 = 17;
const ATTR_CLASS_ID: u16 = 7;

#[inline]
fn is_target_tag(t: u16) -> bool {
    matches!(
        t,
        TAG_HTML | TAG_HEAD | TAG_BODY | tags::TITLE | tags::SCRIPT | tags::STYLE
            | tags::LINK | tags::META | TAG_NOSCRIPT | tags::FORM | tags::INPUT
            | tags::SELECT | tags::OPTION | tags::TEXTAREA | tags::BUTTON | TAG_LABEL
            | TAG_A | TAG_IFRAME | TAG_TEMPLATE
    )
}

pub struct Collector {
    limits: Limits,
    pub title: Option<CompactString>,
    pub meta_description: Option<CompactString>,
    title_open: bool,
    forms: SmallVec<[FormBuilder; 4]>,
    open_form: Option<usize>,
    pub fields: SmallVec<[FieldData; 32]>,
    tokens: SmallVec<[CompactString; 8]>,
    script_srcs: SmallVec<[CompactString; 12]>,
    inline_count: usize,
    challenge: Option<Bytes>,
    pub challenge_markers: SmallVec<[(CompactString, crate::types::ChallengeType); 4]>,
    pub challenge_script_url: Option<CompactString>,
    extract_keys: Vec<CompactString>,
    extracted_ids: std::collections::BTreeMap<u32, CompactString>,
    next_data: Option<NextData>,
    cur_script: Option<ScriptCapture>,
    parse_errors: u32,
    dom: DomTree,
    open: OpenStack,
    skip: Vec<bool>,
    tag_intern: TagInterner,
    attr_intern: AttrNameInterner,
}

impl Collector {
    pub fn new(limits: Limits, extract_keys: Vec<CompactString>) -> Self {
        Self {
            limits,
            title: None,
            meta_description: None,
            title_open: false,
            forms: SmallVec::new(),
            open_form: None,
            fields: SmallVec::new(),
            tokens: SmallVec::new(),
            script_srcs: SmallVec::new(),
            inline_count: 0,
            challenge: None,
            challenge_markers: SmallVec::new(),
            challenge_script_url: None,
            extract_keys,
            extracted_ids: std::collections::BTreeMap::new(),
            next_data: None,
            cur_script: None,
            parse_errors: 0,
            dom: DomTree::new(crate::dom::DomLimits::default()),
            open: OpenStack::new(),
            skip: Vec::with_capacity(64),
            tag_intern: TagInterner::new(),
            attr_intern: AttrNameInterner::new(),
        }
    }

    fn finalize_form(&mut self) {
        self.open_form = None;
    }

    fn finalize_script(&mut self) {
        let Some(cap) = self.cur_script.take() else { return };
        match cap.kind {
            ScriptKind::NextData => {
                if cap.over || cap.buf.is_empty() {
                    self.parse_errors += 1;
                } else {
                    match parse_next_data(&cap.buf) {
                        Ok(nd) => self.next_data = Some(nd),
                        Err(_) => self.parse_errors += 1,
                    }
                }
            }
            ScriptKind::Inline => {
                if !cap.over && is_challenge(&cap.buf) {
                    self.challenge = Some(Bytes::from(cap.buf.into_vec()));
                } else {
                    self.inline_count += 1;
                }
            }
        }
    }

    fn finalize_pending(&mut self) {
        self.title_open = false;
        self.finalize_script();
        self.finalize_form();
    }

    #[inline]
    fn dom_parent(&self) -> u32 {
        self.open.top().unwrap_or(u32::MAX)
    }

    #[inline]
    fn dom_has_marker_attr(&self, node: u32) -> bool {
        self.dom.attr(node, ATTR_ID_ID).is_some() || self.dom.attr(node, ATTR_CLASS_ID).is_some()
    }

    pub fn apply(&mut self, pool: &[u8], attrs: &[AttrEv], ev: Ev) {
        match ev {
            Ev::DomOpen { tag, tag_dyn, attr_start, attr_count } => {
                let tag_id = if tag == TAG_UNKNOWN {
                    match tag_dyn {
                        Some(sp) => self.tag_intern.intern(sp.get(pool)),
                        None => TAG_UNKNOWN,
                    }
                } else {
                    tag
                };
                let a0 = attr_start as usize;
                let a1 = (a0 + attr_count as usize).min(attrs.len());
                let void = crate::dom::is_void_tag(tag_id);
                let mut marker = is_target_tag(tag_id);
                let mut refs: SmallVec<[(u16, &[u8]); 6]> = SmallVec::new();
                for a in &attrs[a0..a1] {
                    let name_id = if a.name == crate::dom::ATTR_UNKNOWN {
                        match a.name_dyn {
                            Some(sp) => self.attr_intern.intern(sp.get(pool)),
                            None => crate::dom::ATTR_UNKNOWN,
                        }
                    } else {
                        a.name
                    };
                    if !marker
                        && (name_id == ATTR_ID_ID
                            || name_id == ATTR_CLASS_ID
                            || a.name_dyn
                                .map(|sp| sp.get(pool).as_bytes().starts_with(b"data-"))
                                .unwrap_or(false))
                    {
                        marker = true;
                    }
                    refs.push((name_id, a.value.get(pool).as_bytes()));
                }
                if !marker {
                    if !void {
                        self.skip.push(true);
                    }
                    return;
                }
                self.open.imply_close(&self.dom, tag_id);
                let parent = self.dom_parent();
                if let Some(node) = self.dom.open_element(parent, tag_id, &refs) {
                    if !void {
                        self.open.push(node);
                    }
                }
                if !void {
                    self.skip.push(false);
                }
            }
            Ev::DomText { span } => {
                let Some(top) = self.open.top() else { return };
                let t = self.dom.tag_id(top);
                if t == tags::SCRIPT {
                    return;
                }
                let text = span.get(pool);
                let always = matches!(t, tags::STYLE | tags::BUTTON | TAG_LABEL | TAG_A);
                if always {
                    let _ = self.dom.push_text(top, text.as_bytes());
                } else if self.dom_has_marker_attr(top) {
                    let cut = floor_char_boundary(text, 128);
                    let _ = self.dom.push_text(top, text[..cut].as_bytes());
                }
            }
            Ev::DomClose { tag, tag_dyn } => {
                let tag_id = if tag == TAG_UNKNOWN {
                    match tag_dyn {
                        Some(sp) => self.tag_intern.intern(sp.get(pool)),
                        None => return,
                    }
                } else {
                    tag
                };
                if self.skip.pop() == Some(true) {
                    return;
                }
                let _ = self.open.close(&self.dom, tag_id);
            }
            Ev::Script { src, is_next_data } => {
                self.finalize_script();
                if let Some(src) = src {
                    if self.script_srcs.len() < 24 {
                        self.script_srcs.push(CompactString::from(src.get(pool)));
                    }
                    self.cur_script = None;
                } else {
                    self.cur_script = Some(ScriptCapture {
                        kind: if is_next_data { ScriptKind::NextData } else { ScriptKind::Inline },
                        buf: SmallVec::new(),
                        over: false,
                    });
                }
            }
            Ev::ScriptText { span, last } => {
                if let Some(cap) = self.cur_script.as_mut() {
                    let limit = match cap.kind {
                        ScriptKind::NextData => self.limits.next_data_cap,
                        ScriptKind::Inline => self.limits.script_cap,
                    };
                    let text = span.get(pool);
                    if cap.buf.len() + text.len() > limit {
                        cap.over = true;
                    } else {
                        cap.buf.extend_from_slice(text.as_bytes());
                    }
                    if last {
                        self.finalize_script();
                    }
                }
            }
            Ev::TitleOpen => {
                self.title_open = true;
            }
            Ev::TitleText { span, last } => {
                if self.title_open {
                    let text = span.get(pool);
                    if !text.is_empty() {
                        let cap = 256usize;
                        match &mut self.title {
                            Some(t) => {
                                if t.len() < cap {
                                    t.push_str(&text[..text.len().min(cap - t.len())]);
                                }
                            }
                            None => {
                                self.title = Some(CompactString::from(&text[..text.len().min(cap)]));
                            }
                        }
                    }
                    if last {
                        self.title_open = false;
                    }
                }
            }
            Ev::MetaDescription { span } => {
                if self.meta_description.is_none() {
                    let content = span.get(pool);
                    let cap = 512usize;
                    let cut = floor_char_boundary(content, cap);
                    self.meta_description = Some(CompactString::from(&content[..cut]));
                }
            }
            Ev::Form { action, method } => {
                self.finalize_form();
                if self.forms.len() >= self.limits.max_forms {
                    return;
                }
                let method = if method.len == 0 {
                    CompactString::const_new("get")
                } else {
                    CompactString::from(method.get(pool))
                };
                self.forms.push(FormBuilder {
                    action: action.map(|a| CompactString::from(a.get(pool))),
                    method,
                    fields: SmallVec::new(),
                });
                self.open_form = Some(self.forms.len() - 1);
            }
            Ev::Field { tag, name, value, kind, hidden, id } => {
                let kind_str = kind.get(pool);
                let kind_enum = FieldKind::from_type_attr(kind_str);
                let name_str = name.get(pool);
                if self.fields.len() < self.limits.max_fields {
                    self.fields.push(FieldData {
                        tag: CompactString::from(tag.get(pool)),
                        name: CompactString::from(name_str),
                        value: value.map(|v| CompactString::from(v.get(pool))),
                        kind: kind_enum,
                        hidden,
                        id: id.map(|v| CompactString::from(v.get(pool))),
                    });
                }
                if hidden && looks_like_token(name_str) {
                    if let Some(v) = value {
                        let v = v.get(pool);
                        if self.tokens.len() < self.limits.max_tokens && v.len() <= 512 {
                            self.tokens.push(CompactString::from(v));
                        }
                    }
                }
                if let Some(idx) = self.open_form {
                    let builder = &mut self.forms[idx];
                    if builder.fields.len() < self.limits.max_fields {
                        builder.fields.push(FormData {
                            name: CompactString::from(name_str),
                            value: value.map(|v| CompactString::from(v.get(pool))),
                            kind: kind_enum,
                        });
                    }
                }
            }
            Ev::Extract { key, span } => {
                self.push_extract(key, span.get(pool), 512);
            }
            Ev::ChallengeDetected { marker, ct } => {
                let url = marker.get(pool);
                if self.challenge_markers.len() < 4 {
                    self.challenge_markers.push((
                        CompactString::from(url),
                        crate::types::ChallengeType::from_pattern_index(ct as usize),
                    ));
                }
                if self.challenge_script_url.is_none() {
                    self.challenge_script_url = Some(CompactString::from(url));
                }
            }
        }
    }

    pub fn push_extract(&mut self, key: u32, text: &str, cap: usize) {
        let e = self.extracted_ids.entry(key).or_default();
        if e.len() >= cap {
            return;
        }
        let cut = floor_char_boundary(text, cap - e.len());
        e.push_str(&text[..cut]);
    }

    pub fn into_page(mut self, bytes_fed: u64, utf8_bad_chunks: u32, truncated: bool) -> PageData {
        self.finalize_pending();
        let tag_names = self.tag_intern.name_table();
        let attr_names = self.attr_intern.name_table();
        self.dom.attach_name_tables(tag_names, attr_names);
        let parse_errors = self.parse_errors;
        let extracted = self
            .extracted_ids
            .into_iter()
            .filter_map(|(k, v)| self.extract_keys.get(k as usize).map(|key| (key.clone(), v)))
            .collect();
        PageData {
            title: self.title,
            meta_description: self.meta_description,
            forms: self
                .forms
                .into_iter()
                .map(|b| Form {
                    action: b.action,
                    method: b.method,
                    fields: b.fields,
                })
                .collect(),
            fields: self.fields,
            tokens: self.tokens,
            script_srcs: self.script_srcs,
            inline_count: self.inline_count,
            challenge: self.challenge,
            challenge_markers: self.challenge_markers,
            challenge_script_url: self.challenge_script_url,
            extracted,
            next_data: self.next_data,
            dom: self.dom,
            bytes_fed,
            utf8_bad_chunks,
            truncated,
            html_head: None,
            parse_errors,
        }
    }
}

pub fn floor_char_boundary(s: &str, limit: usize) -> usize {
    if s.len() <= limit {
        return s.len();
    }
    let mut i = limit;
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

const TOKEN_MARKERS: &[&str] = &[
    "csrf", "_token", "token", "xsrf", "authenticity", "x-csrf",
    "anticsrf", "anti-csrf", "requesttoken", "request-token",
    "__requestverificationtoken", "csrfmiddlewaretoken",
];

#[inline]
fn ascii_ci_contains(hay: &str, needle: &str) -> bool {
    let h = hay.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || h.len() < n.len() {
        return false;
    }
    'outer: for i in 0..=h.len() - n.len() {
        for j in 0..n.len() {
            if h[i + j].to_ascii_lowercase() != n[j] {
                continue 'outer;
            }
        }
        return true;
    }
    false
}

pub fn looks_like_token(name: &str) -> bool {
    TOKEN_MARKERS.iter().any(|needle| ascii_ci_contains(name, needle))
}
