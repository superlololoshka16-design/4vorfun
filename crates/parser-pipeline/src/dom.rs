
use compact_str::CompactString;
use smallvec::SmallVec;
use std::collections::HashMap;

pub mod tags {
    pub const AREA: u16 = 4;
    pub const BASE: u16 = 9;
    pub const BR: u16 = 14;
    pub const BUTTON: u16 = 15;
    pub const COL: u16 = 20;
    pub const DD: u16 = 24;
    pub const DT: u16 = 31;
    pub const EMBED: u16 = 33;
    pub const FORM: u16 = 38;
    pub const HR: u16 = 48;
    pub const IMG: u16 = 52;
    pub const INPUT: u16 = 53;
    pub const LI: u16 = 58;
    pub const LINK: u16 = 59;
    pub const META: u16 = 64;
    pub const OPTION: u16 = 71;
    pub const P: u16 = 73;
    pub const PARAM: u16 = 74;
    pub const SCRIPT: u16 = 84;
    pub const SELECT: u16 = 87;
    pub const SOURCE: u16 = 90;
    pub const STYLE: u16 = 93;
    pub const TBODY: u16 = 98;
    pub const TD: u16 = 99;
    pub const TEXTAREA: u16 = 101;
    pub const TFOOT: u16 = 102;
    pub const TH: u16 = 103;
    pub const THEAD: u16 = 104;
    pub const TITLE: u16 = 106;
    pub const TRACK: u16 = 108;
    pub const TR: u16 = 107;
    pub const WBR: u16 = 113;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId {
    pub index: u32,
    pub generation: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StrSpan {
    pub off: u32,
    pub len: u16,
}

pub const EMPTY_SPAN: StrSpan = StrSpan { off: 0, len: 0 };

pub mod node_flags {
    pub const ELEMENT: u8 = 1 << 0;
    pub const TEXT: u8 = 1 << 1;
    pub const SCRIPT: u8 = 1 << 2;
    pub const FORM: u8 = 1 << 3;
    pub const INPUT: u8 = 1 << 4;
    pub const HIDDEN: u8 = 1 << 5;
    pub const VOID: u8 = 1 << 6;
    pub const TRUNCATED: u8 = 1 << 7;
}

pub static TAGS: phf::Map<&'static str, u16> = phf::phf_map! {
    "a" => 1, "abbr" => 2, "address" => 3, "area" => tags::AREA, "article" => 5,
    "aside" => 6, "audio" => 7, "b" => 8, "base" => tags::BASE, "bdi" => 10, "bdo" => 11,
    "blockquote" => 12, "body" => 13, "br" => tags::BR, "button" => tags::BUTTON,
    "canvas" => 16, "caption" => 17, "cite" => 18, "code" => 19, "col" => tags::COL,
    "colgroup" => 21, "data" => 22, "datalist" => 23, "dd" => tags::DD, "del" => 25,
    "details" => 26, "dfn" => 27, "dialog" => 28, "div" => 29, "dl" => 30,
    "dt" => tags::DT, "em" => 32, "embed" => tags::EMBED, "fieldset" => 34,
    "figcaption" => 35, "figure" => 36, "footer" => 37, "form" => tags::FORM,
    "h1" => 39, "h2" => 40, "h3" => 41, "h4" => 42, "h5" => 43, "h6" => 44,
    "head" => 45, "header" => 46, "hgroup" => 47, "hr" => tags::HR, "html" => 49,
    "i" => 50, "iframe" => 51, "img" => tags::IMG, "input" => tags::INPUT, "ins" => 54,
    "kbd" => 55, "label" => 56, "legend" => 57, "li" => tags::LI, "link" => tags::LINK,
    "main" => 60, "map" => 61, "mark" => 62, "menu" => 63, "meta" => tags::META,
    "meter" => 65, "nav" => 66, "noscript" => 67, "object" => 68, "ol" => 69,
    "optgroup" => 70, "option" => tags::OPTION, "output" => 72, "p" => tags::P,
    "param" => tags::PARAM, "picture" => 75, "pre" => 76, "progress" => 77, "q" => 78,
    "rp" => 79, "rt" => 80, "ruby" => 81, "s" => 82, "samp" => 83, "script" => tags::SCRIPT,
    "search" => 85, "section" => 86, "select" => tags::SELECT, "slot" => 88,
    "small" => 89, "source" => tags::SOURCE, "span" => 91, "strong" => 92, "style" => 93,
    "sub" => 94, "summary" => 95, "sup" => 96, "table" => 97, "tbody" => tags::TBODY,
    "td" => tags::TD, "template" => 100, "textarea" => tags::TEXTAREA,
    "tfoot" => tags::TFOOT, "th" => tags::TH, "thead" => tags::THEAD, "time" => 105,
    "title" => tags::TITLE, "tr" => tags::TR, "track" => tags::TRACK, "u" => 109,
    "ul" => 110, "var" => 111, "video" => 112, "wbr" => tags::WBR,
};

pub const TAG_UNKNOWN: u16 = u16::MAX;
const DYNAMIC_TAG_BASE: u16 = 200;

static TAG_NAME_BY_ID: std::sync::LazyLock<Vec<Option<&'static str>>> =
    std::sync::LazyLock::new(|| {
        let mut v: Vec<Option<&'static str>> = vec![None; DYNAMIC_TAG_BASE as usize];
        for (name, id) in TAGS.entries() {
            v[*id as usize] = Some(*name);
        }
        v
    });

static ATTR_NAME_BY_ID: std::sync::LazyLock<Vec<Option<&'static str>>> =
    std::sync::LazyLock::new(|| {
        let mut v: Vec<Option<&'static str>> = vec![None; DYNAMIC_ATTR_BASE as usize];
        for (name, id) in ATTR_NAMES.entries() {
            v[*id as usize] = Some(*name);
        }
        v
    });

#[inline]
pub fn is_void_tag(tag: u16) -> bool {
    matches!(
        tag,
        tags::AREA | tags::BASE | tags::BR | tags::COL | tags::EMBED | tags::HR | tags::IMG
            | tags::INPUT | tags::LINK | tags::META | tags::PARAM | tags::SOURCE | tags::TRACK
            | tags::WBR
    )
}

pub const ATTR_UNKNOWN: u16 = u16::MAX;

pub static ATTR_NAMES: phf::Map<&'static str, u16> = phf::phf_map! {
    "accept" => 1, "action" => 2, "alt" => 3, "async" => 4, "charset" => 5, "checked" => 6,
    "class" => 7, "cols" => 8, "content" => 9, "defer" => 10, "dir" => 11, "disabled" => 12,
    "for" => 13, "headers" => 14, "height" => 15, "href" => 16, "id" => 17, "lang" => 18,
    "loading" => 19, "max" => 20, "maxlength" => 21, "media" => 22, "method" => 23,
    "min" => 24, "multiple" => 25, "name" => 26, "pattern" => 27, "placeholder" => 28,
    "property" => 29, "rel" => 30, "required" => 31, "rows" => 32, "sandbox" => 33,
    "scope" => 34, "selected" => 35, "shape" => 36, "size" => 37, "sizes" => 38, "span" => 39,
    "src" => 40, "srcdoc" => 41, "srclang" => 42, "srcset" => 43, "start" => 44, "step" => 45,
    "style" => 46, "tabindex" => 47, "target" => 48, "title" => 49, "type" => 50,
    "usemap" => 51, "value" => 52, "width" => 53, "wrap" => 54, "role" => 55,
    "aria-hidden" => 56, "aria-label" => 57, "crossorigin" => 58, "integrity" => 59,
    "referrerpolicy" => 60, "nomodule" => 61, "kind" => 62, "label" => 63, "open" => 64,
    "datetime" => 65, "download" => 66, "hidden" => 67,
};

const DYNAMIC_ATTR_BASE: u16 = 100;
const ATTR_NAME_ID: u16 = 26;
const ATTR_TYPE_ID: u16 = 50;

#[derive(Debug, Clone, Copy)]
pub struct DomLimits {
    pub max_nodes: usize,
    pub max_attrs: usize,
    pub pool_bytes: usize,
    pub text_node_bytes: usize,
}

impl Default for DomLimits {
    fn default() -> Self {
        Self {
            max_nodes: 20_000,
            max_attrs: 16_384,
            pool_bytes: 256 * 1024,
            text_node_bytes: 4 * 1024,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Attr {
    pub name: u16,
    pub value: StrSpan,
}

pub struct DomTree {
    parents: Vec<u32>,
    first_child: Vec<u32>,
    last_child: Vec<u32>,
    next_sibling: Vec<u32>,
    prev_sibling: Vec<u32>,
    tag_ids: Vec<u16>,
    flags: Vec<u8>,
    generations: Vec<u32>,
    spans: Vec<StrSpan>,
    attr_start: Vec<u32>,
    attr_count: Vec<u8>,
    attrs: Vec<Attr>,
    pool: Vec<u8>,
    tag_names: Vec<CompactString>,
    attr_name_table: Vec<CompactString>,
    free: Vec<u32>,
    roots_head: u32,
    roots_tail: u32,
    limits: DomLimits,
    truncated: bool,
    pub scripts: Vec<u32>,
    pub forms: Vec<u32>,
    pub inputs: Vec<u32>,
    pub title_node: Option<u32>,
    pub meta_desc_node: Option<u32>,
}

impl DomTree {
    pub fn new(limits: DomLimits) -> Self {
        let cap = 1024.min(limits.max_nodes);
        Self {
            parents: Vec::with_capacity(cap),
            first_child: Vec::with_capacity(cap),
            last_child: Vec::with_capacity(cap),
            next_sibling: Vec::with_capacity(cap),
            prev_sibling: Vec::with_capacity(cap),
            tag_ids: Vec::with_capacity(cap),
            flags: Vec::with_capacity(cap),
            generations: Vec::with_capacity(cap),
            spans: Vec::with_capacity(256),
            attr_start: Vec::with_capacity(cap),
            attr_count: Vec::with_capacity(cap),
            attrs: Vec::with_capacity(256),
            pool: Vec::with_capacity(8 * 1024),
            tag_names: Vec::new(),
            attr_name_table: Vec::new(),
            free: Vec::new(),
            roots_head: u32::MAX,
            roots_tail: u32::MAX,
            limits,
            truncated: false,
            scripts: Vec::new(),
            forms: Vec::new(),
            inputs: Vec::new(),
            title_node: None,
            meta_desc_node: None,
        }
    }

    #[inline]
    fn index_swap_remove(v: &mut Vec<u32>, i: u32) {
        if let Some(pos) = v.iter().position(|&x| x == i) {
            v.swap_remove(pos);
        }
    }

    fn alloc_slot(&mut self) -> Option<u32> {
        if let Some(index) = self.free.pop() {
            Some(index)
        } else if self.parents.len() < self.limits.max_nodes {
            Some(self.parents.len() as u32)
        } else {
            self.truncated = true;
            None
        }
    }

    fn push_slot(&mut self, parent: u32, tag: u16, flags: u8, span: StrSpan) -> Option<u32> {
        let index = self.alloc_slot()?;
        if (index as usize) == self.parents.len() {
            self.parents.push(parent);
            self.first_child.push(u32::MAX);
            self.last_child.push(u32::MAX);
            self.next_sibling.push(u32::MAX);
            self.prev_sibling.push(u32::MAX);
            self.tag_ids.push(tag);
            self.flags.push(flags);
            self.generations.push(0);
            self.spans.push(span);
            self.attr_start.push(0);
            self.attr_count.push(0);
        } else {
            let i = index as usize;
            self.parents[i] = parent;
            self.first_child[i] = u32::MAX;
            self.last_child[i] = u32::MAX;
            self.next_sibling[i] = u32::MAX;
            self.prev_sibling[i] = u32::MAX;
            self.tag_ids[i] = tag;
            self.flags[i] = flags;
            self.spans[i] = span;
            self.attr_start[i] = 0;
            self.attr_count[i] = 0;
        }
        self.link_child(parent, index);
        Some(index)
    }

    fn link_child(&mut self, parent: u32, child: u32) {
        let (prev, p) = if parent == u32::MAX {
            (self.roots_tail, None)
        } else {
            let p = parent as usize;
            (self.last_child[p], Some(p))
        };
        self.parents[child as usize] = parent;
        if prev == u32::MAX {
            if parent == u32::MAX {
                self.roots_head = child;
            } else {
                self.first_child[p.expect("parent")] = child;
            }
        } else {
            self.next_sibling[prev as usize] = child;
            self.prev_sibling[child as usize] = prev;
        }
        if parent == u32::MAX {
            self.roots_tail = child;
        } else {
            self.last_child[p.expect("parent")] = child;
        }
    }

    fn pool_put(&mut self, bytes: &[u8]) -> StrSpan {
        if bytes.len() > u16::MAX as usize {
            let cut = floor_char_boundary(bytes, u16::MAX as usize);
            return self.pool_put(&bytes[..cut]);
        }
        if self.pool.len() + bytes.len() > self.limits.pool_bytes {
            self.truncated = true;
            return EMPTY_SPAN;
        }
        let off = self.pool.len() as u32;
        self.pool.extend_from_slice(bytes);
        StrSpan { off, len: bytes.len() as u16 }
    }

    pub(crate) fn open_element(
        &mut self,
        parent: u32,
        tag: u16,
        attrs: &[(u16, &[u8])],
    ) -> Option<u32> {
        let mut flags = node_flags::ELEMENT;
        if tag == tags::SCRIPT {
            flags |= node_flags::SCRIPT;
        } else if tag == tags::FORM {
            flags |= node_flags::FORM;
        } else if matches!(tag, tags::INPUT | tags::SELECT | tags::TEXTAREA | tags::BUTTON) {
            flags |= node_flags::INPUT;
        }
        if is_void_tag(tag) {
            flags |= node_flags::VOID;
        }
        let node = self.push_slot(parent, tag, flags, EMPTY_SPAN)?;
        let start = self.attrs.len() as u32;
        let mut count: u8 = 0;
        let mut hidden = false;
        for (name, value) in attrs {
            if self.attrs.len() >= self.limits.max_attrs {
                self.truncated = true;
                break;
            }
            let span = self.pool_put(value);
            if *name == ATTR_TYPE_ID {
                hidden = value.eq_ignore_ascii_case(b"hidden");
            }
            self.attrs.push(Attr { name: *name, value: span });
            count += 1;
        }
        let i = node as usize;
        self.attr_start[i] = start;
        self.attr_count[i] = count;
        if hidden {
            self.flags[i] |= node_flags::HIDDEN;
        }
        if tag == tags::SCRIPT {
            self.scripts.push(node);
        } else if tag == tags::FORM {
            self.forms.push(node);
        } else if self.flags[i] & node_flags::INPUT != 0 {
            self.inputs.push(node);
        } else if tag == tags::TITLE && self.title_node.is_none() {
            self.title_node = Some(node);
        } else if tag == tags::META && self.meta_desc_node.is_none() {
            let named = attrs.iter().any(
                |(n, v)| *n == ATTR_NAME_ID && v.eq_ignore_ascii_case(b"description"),
            );
            if named {
                self.meta_desc_node = Some(node);
            }
        }
        Some(node)
    }

    pub(crate) fn push_text(&mut self, parent: u32, text: &[u8]) -> Option<u32> {
        if parent == u32::MAX {
            return None;
        }
        let cut = text.len().min(self.limits.text_node_bytes);
        let cut = floor_char_boundary(text, cut);
        let truncated = cut < text.len();
        let span = self.pool_put(&text[..cut]);
        let last = self.last_child[parent as usize];
        if last != u32::MAX {
            let il = last as usize;
            let adjacent = self.flags[il] & node_flags::TEXT != 0
                && self.parents[il] == parent
                && self.spans[il].off as usize + self.spans[il].len as usize
                    == span.off as usize;
            if adjacent {
                let merged = self.spans[il].len as usize + span.len as usize;
                if merged <= u16::MAX as usize {
                    self.spans[il].len = merged as u16;
                    if truncated {
                        self.flags[il] |= node_flags::TRUNCATED;
                    }
                    return Some(last);
                }
            }
        }
        let flags = if truncated {
            node_flags::TEXT | node_flags::TRUNCATED
        } else {
            node_flags::TEXT
        };
        self.push_slot(parent, TAG_UNKNOWN, flags, span)
    }

    fn unlink(&mut self, node: u32) {
        let i = node as usize;
        let prev = self.prev_sibling[i];
        let next = self.next_sibling[i];
        let is_root = self.parents[i] == u32::MAX;
        if prev != u32::MAX {
            self.next_sibling[prev as usize] = next;
        } else if is_root {
            self.roots_head = next;
        } else if let Some(p) = self.parent_index(i) {
            self.first_child[p] = next;
        }
        if next != u32::MAX {
            self.prev_sibling[next as usize] = prev;
        } else if is_root {
            self.roots_tail = prev;
        } else if let Some(p) = self.parent_index(i) {
            self.last_child[p] = prev;
        }
        self.prev_sibling[i] = u32::MAX;
        self.next_sibling[i] = u32::MAX;
    }

    fn free_node(&mut self, node: u32) {
        let i = node as usize;
        self.generations[i] = self.generations[i].wrapping_add(1);
        self.flags[i] = 0;
        self.tag_ids[i] = TAG_UNKNOWN;
        self.spans[i] = EMPTY_SPAN;
        self.attr_start[i] = 0;
        self.attr_count[i] = 0;
        self.parents[i] = u32::MAX;
        self.free.push(node);
    }

    pub fn remove_node(&mut self, id: NodeId) -> bool {
        let i = id.index as usize;
        if i >= self.parents.len() || self.generations[i] != id.generation {
            return false;
        }
        let mut child = self.first_child[i];
        while child != u32::MAX {
            let next = self.next_sibling[child as usize];
            self.unlink(child);
            self.free_node(child);
            child = next;
        }
        self.unlink(id.index);
        self.free_node(id.index);
        Self::index_swap_remove(&mut self.scripts, id.index);
        Self::index_swap_remove(&mut self.forms, id.index);
        Self::index_swap_remove(&mut self.inputs, id.index);
        if self.title_node == Some(id.index) {
            self.title_node = None;
        }
        if self.meta_desc_node == Some(id.index) {
            self.meta_desc_node = None;
        }
        true
    }

    #[inline]
    fn parent_index(&self, i: usize) -> Option<usize> {
        let p = self.parents[i];
        (p != u32::MAX).then_some(p as usize)
    }

    #[inline]
    pub fn is_valid(&self, id: NodeId) -> bool {
        let i = id.index as usize;
        i < self.parents.len() && self.generations[i] == id.generation
    }

    pub fn node_id(&self, index: u32) -> NodeId {
        NodeId { index, generation: self.generations[index as usize] }
    }

    #[inline]
    pub fn tag_id(&self, index: u32) -> u16 {
        self.tag_ids[index as usize]
    }

    #[inline]
    pub fn flags(&self, index: u32) -> u8 {
        self.flags[index as usize]
    }

    pub fn parent(&self, index: u32) -> Option<u32> {
        let p = self.parents[index as usize];
        (p != u32::MAX).then_some(p)
    }

    pub fn len(&self) -> usize {
        self.parents.len() - self.free.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn tag_name(&self, tag: u16) -> Option<&str> {
        if tag == TAG_UNKNOWN {
            return None;
        }
        if tag < DYNAMIC_TAG_BASE {
            TAG_NAME_BY_ID[tag as usize]
        } else {
            self.tag_names.get((tag - DYNAMIC_TAG_BASE) as usize).map(|s| s.as_str())
        }
    }

    pub fn text(&self, index: u32) -> Option<&str> {
        let i = index as usize;
        if self.flags[i] & node_flags::TEXT == 0 {
            return None;
        }
        self.span_str(self.spans[i])
    }

    pub fn attr(&self, index: u32, name: u16) -> Option<&str> {
        let i = index as usize;
        let start = self.attr_start[i] as usize;
        let count = self.attr_count[i] as usize;
        for a in &self.attrs[start..start + count] {
            if a.name == name {
                return self.span_str(a.value);
            }
        }
        None
    }

    pub fn attrs_of(&self, index: u32) -> impl Iterator<Item = (&str, &str)> {
        let i = index as usize;
        let start = self.attr_start[i] as usize;
        let count = self.attr_count[i] as usize;
        let pool = &self.pool;
        let attr_table = &self.attr_name_table;
        self.attrs[start..start + count].iter().map(move |a| {
            let name = reverse_attr_name(a.name, attr_table).unwrap_or("?");
            let value = span_str_in(pool, a.value).unwrap_or("");
            (name, value)
        })
    }

    pub fn children(&self, index: u32) -> ChildIter<'_> {
        if index == u32::MAX {
            return ChildIter { tree: self, next: self.roots_head };
        }
        ChildIter { tree: self, next: self.first_child[index as usize] }
    }

    pub fn roots_head(&self) -> u32 {
        self.roots_head
    }

    pub fn span_str(&self, span: StrSpan) -> Option<&str> {
        span_str_in(&self.pool, span)
    }

    pub fn depth(&self, mut index: u32) -> u32 {
        let mut d = 0;
        while let Some(p) = self.parent(index) {
            index = p;
            d += 1;
        }
        d
    }

    pub fn script_attr(&self, i: usize, name: &str) -> Option<&str> {
        let node = *self.scripts.get(i)?;
        let name_id = ATTR_NAMES.get(name).copied()?;
        self.attr(node, name_id)
    }

    pub fn pool_len(&self) -> usize {
        self.pool.len()
    }

    pub fn attr_count_total(&self) -> usize {
        self.attrs.len()
    }

    pub(crate) fn attach_name_tables(
        &mut self,
        tag_names: Vec<CompactString>,
        attr_name_table: Vec<CompactString>,
    ) {
        self.tag_names = tag_names;
        self.attr_name_table = attr_name_table;
    }
}

pub struct ChildIter<'a> {
    tree: &'a DomTree,
    next: u32,
}

impl Iterator for ChildIter<'_> {
    type Item = u32;
    #[inline]
    fn next(&mut self) -> Option<u32> {
        if self.next == u32::MAX {
            return None;
        }
        let cur = self.next;
        self.next = self.tree.next_sibling[cur as usize];
        Some(cur)
    }
}

#[inline]
fn span_str_in(pool: &[u8], span: StrSpan) -> Option<&str> {
    if span.len == 0 {
        return Some("");
    }
    let start = span.off as usize;
    let end = start + span.len as usize;
    pool.get(start..end).and_then(|b| std::str::from_utf8(b).ok())
}

fn reverse_attr_name(name: u16, dynamic: &[CompactString]) -> Option<&str> {
    if name >= DYNAMIC_ATTR_BASE {
        if name == ATTR_UNKNOWN {
            return None;
        }
        return dynamic.get((name - DYNAMIC_ATTR_BASE) as usize).map(|s| s.as_str());
    }
    ATTR_NAME_BY_ID[name as usize]
}

fn floor_char_boundary(s: &[u8], limit: usize) -> usize {
    if s.len() <= limit {
        return s.len();
    }
    let mut i = limit;
    while i > 0 && (s[i] & 0xC0) == 0x80 {
        i -= 1;
    }
    i
}

pub struct TagInterner {
    dynamic: HashMap<CompactString, u16>,
    next_tag: u16,
}

impl Default for TagInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl TagInterner {
    pub fn new() -> Self {
        Self { dynamic: HashMap::new(), next_tag: DYNAMIC_TAG_BASE }
    }

    #[inline]
    pub fn intern(&mut self, tag: &str) -> u16 {
        if let Some(id) = TAGS.get(tag) {
            return *id;
        }
        if let Some(id) = self.dynamic.get(tag) {
            return *id;
        }
        if self.next_tag < DYNAMIC_TAG_BASE + 500 {
            let id = self.next_tag;
            self.next_tag += 1;
            self.dynamic.insert(CompactString::new(tag), id);
            id
        } else {
            TAG_UNKNOWN
        }
    }

    pub fn name_table(&self) -> Vec<CompactString> {
        let mut v: Vec<(u16, CompactString)> =
            self.dynamic.iter().map(|(k, id)| (*id, k.clone())).collect();
        v.sort_unstable_by_key(|(id, _)| *id);
        v.into_iter().map(|(_, k)| k).collect()
    }
}

pub struct AttrNameInterner {
    dynamic: HashMap<CompactString, u16>,
    next: u16,
}

impl Default for AttrNameInterner {
    fn default() -> Self {
        Self::new()
    }
}

impl AttrNameInterner {
    pub fn new() -> Self {
        Self { dynamic: HashMap::new(), next: DYNAMIC_ATTR_BASE }
    }

    #[inline]
    pub fn intern(&mut self, name: &str) -> u16 {
        if let Some(id) = ATTR_NAMES.get(name) {
            return *id;
        }
        if let Some(id) = self.dynamic.get(name) {
            return *id;
        }
        if self.next < DYNAMIC_ATTR_BASE + 200 {
            let id = self.next;
            self.next += 1;
            self.dynamic.insert(CompactString::new(name), id);
            id
        } else {
            ATTR_UNKNOWN
        }
    }

    pub fn name_table(&self) -> Vec<CompactString> {
        let mut v: Vec<(u16, CompactString)> =
            self.dynamic.iter().map(|(k, id)| (*id, k.clone())).collect();
        v.sort_unstable_by_key(|(id, _)| *id);
        v.into_iter().map(|(_, k)| k).collect()
    }
}

pub struct OpenStack {
    stack: SmallVec<[u32; 64]>,
}

impl Default for OpenStack {
    fn default() -> Self {
        Self::new()
    }
}

impl OpenStack {
    pub fn new() -> Self {
        Self { stack: SmallVec::new() }
    }

    #[inline]
    pub fn top(&self) -> Option<u32> {
        self.stack.last().copied()
    }

    #[inline]
    pub fn push(&mut self, node: u32) {
        if self.stack.len() < 512 {
            self.stack.push(node);
        }
    }

    pub fn close(&mut self, tree: &DomTree, tag: u16) -> Option<u32> {
        let mut matched = None;
        for (from_top, &node) in self.stack.iter().rev().enumerate() {
            if tree.tag_id(node) == tag {
                matched = Some(from_top);
                break;
            }
        }
        let Some(matched) = matched else { return None };
        for _ in 0..=matched {
            self.stack.pop();
        }
        self.stack.last().copied()
    }

    pub fn imply_close(&mut self, tree: &DomTree, opening: u16) {
        while let Some(&top) = self.stack.last() {
            if sibling_closes(opening, tree.tag_id(top)) {
                self.stack.pop();
            } else {
                break;
            }
        }
    }
}

#[inline]
fn sibling_closes(opening: u16, top: u16) -> bool {
    match (opening, top) {
        (tags::P, tags::P) => true,
        (tags::LI, tags::LI) => true,
        (a, b) if (a == tags::DT || a == tags::DD) && (b == tags::DT || b == tags::DD) => true,
        (a, b) if (a == tags::TH || a == tags::TD) && (b == tags::TH || b == tags::TD) => true,
        (tags::TR, b) if b == tags::TD || b == tags::TH || b == tags::TR => true,
        (a, b) if a == tags::THEAD || a == tags::TBODY || a == tags::TFOOT => {
            b == tags::THEAD || b == tags::TBODY || b == tags::TFOOT
                || b == tags::TD || b == tags::TH || b == tags::TR
        }
        (tags::OPTION, tags::OPTION) => true,
        _ => false,
    }
}
