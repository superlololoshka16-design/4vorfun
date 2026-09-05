use crate::collector::Collector;
pub use crate::collector::Limits;
use crate::scratch::{self, Ev};
use lol_html::MemorySettings;
use lol_html::html_content::TextChunk;
use lol_html::send::Element;
use std::sync::LazyLock;

static CHALLENGE_AC: LazyLock<aho_corasick::AhoCorasick> = LazyLock::new(|| {
    aho_corasick::AhoCorasickBuilder::new()
        .match_kind(aho_corasick::MatchKind::LeftmostFirst)
        .build([
            b"challenges.cloudflare.com/turnstile" as &[u8],
            b"captcha-delivery.com",
            b"cdn-cgi/challenge-platform",
            b"challenges.cloudflare.com/managed",
            b"hcaptcha.com",
            b"recaptcha/api.js",
        ])
        .expect("aho-corasick build")
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Stop,
}

#[derive(Debug, thiserror::Error)]
pub enum PipeError {
    #[error("memory brake")]
    MemoryBrake,
    #[error("handler error")]
    Handler,
    #[error("ambiguous input")]
    Ambiguous,
    #[error("rewriter misuse")]
    Misuse,
}

type Sink = Box<dyn FnMut(&[u8]) + Send>;
type Rewriter = lol_html::send::HtmlRewriter<'static, Sink>;

/// Стрим-пайплайн: lol_html гонит поток, хендлеры кладут строки в арену
/// скретча и эмитят спан-события, дренч — синхронно в конце каждого push().
/// Каналов нет, клонов строк нет, аллокаций на событие нет.
pub struct StreamPipeline {
    rewriter: Option<Rewriter>,
    collector: Collector,
    limits: Limits,
    bytes_fed: u64,
    utf8_bad_chunks: u32,
}

impl StreamPipeline {
    pub fn new(limits: Limits) -> Self {
        Self::build(limits, &[])
    }

    /// Полная сборка: базовые хендлеры + text-extraction по CSS-селекторам.
    pub fn with_selectors(limits: Limits, selectors: &[(String, String)]) -> Self {
        Self::build(limits, selectors)
    }

    fn build(limits: Limits, selectors: &[(String, String)]) -> Self {
        let extract_keys: Vec<compact_str::CompactString> =
            selectors.iter().map(|(k, _)| compact_str::CompactString::new(k.as_str())).collect();

        let mut settings = lol_html::send::Settings::new_send()
            .with_memory_settings(
                MemorySettings::new()
                    .with_max_allowed_memory_usage(4 * 1024 * 1024)
                    .with_graceful_bail_out_on_memory_limit_exceeded(true),
            )
            .with_graceful_bail_out_on_content_handler_error(true)
            // --- SoA-строитель: универсальный селектор, каждый элемент ---
            .append_element_content_handler(lol_html::element!("*", move |el: &mut Element<'_, '_>| {
                let tag_name = el.tag_name();
                let (tag, tag_dyn) = match crate::dom::TAGS.get(tag_name.as_str()) {
                    Some(id) => (*id, None),
                    None => (crate::dom::TAG_UNKNOWN, Some(scratch::push_str(&tag_name))),
                };
                let attr_start = scratch::attr_mark();
                let mut count: u8 = 0;
                for a in el.attributes() {
                    let name = a.name();
                    let value = a.value();
                    let (aid, adyn) = match crate::dom::ATTR_NAMES.get(name.as_str()) {
                        Some(id) => (*id, None),
                        None => (crate::dom::ATTR_UNKNOWN, Some(scratch::push_str(&name))),
                    };
                    scratch::push_attr(scratch::AttrEv {
                        name: aid,
                        name_dyn: adyn,
                        value: scratch::push_str(&value),
                    });
                    count += 1;
                    if count == u8::MAX {
                        break;
                    }
                }
                scratch::emit(Ev::DomOpen { tag, tag_dyn, attr_start, attr_count: count });
                if !crate::dom::is_void_tag(tag) {
                    let _ = el.on_end_tag(lol_html::end_tag!(move |end| {
                        let name = end.name();
                        let (t, tdyn) = match crate::dom::TAGS.get(name.as_str()) {
                            Some(id) => (*id, None),
                            None => (crate::dom::TAG_UNKNOWN, Some(scratch::push_str(&name))),
                        };
                        scratch::emit(Ev::DomClose { tag: t, tag_dyn: tdyn });
                        Ok(())
                    }));
                }
                Ok(())
            }))
            .append_element_content_handler(lol_html::element!("script", move |el: &mut Element<'_, '_>| {
                let is_next_data = el.get_attribute("id").as_deref() == Some("__NEXT_DATA__");
                let src_raw = el.get_attribute("src");
                if let Some(raw) = &src_raw {
                    if let Some(mat) = CHALLENGE_AC.find(raw.as_bytes()) {
                        let marker = scratch::push_str(raw);
                        scratch::emit(Ev::ChallengeDetected {
                            marker,
                            ct: mat.pattern().as_usize() as u8,
                        });
                    }
                }
                let src = src_raw.map(|s| scratch::push_str(&s));
                scratch::emit(Ev::Script { src, is_next_data });
                Ok(())
            }))
            .append_element_content_handler(lol_html::element!("form", move |el: &mut Element<'_, '_>| {
                let action = el.get_attribute("action").map(|a| scratch::push_str(&a));
                let method = el.get_attribute("method")
                    .map(|mut m| {
                        m.make_ascii_lowercase();
                        scratch::push_str(&m)
                    })
                    .unwrap_or(scratch::NIL_SPAN);
                scratch::emit(Ev::Form { action, method });
                Ok(())
            }))
            .append_element_content_handler(lol_html::element!("input, select, textarea, button", move |el: &mut Element<'_, '_>| {
                let tag = el.tag_name();
                let name = el.get_attribute("name").unwrap_or_default();
                if name.is_empty() {
                    return Ok(());
                }
                let kind = el.get_attribute("type").unwrap_or_default();
                let hidden = tag == "input" && kind.eq_ignore_ascii_case("hidden");
                scratch::emit(Ev::Field {
                    tag: scratch::push_str(&tag),
                    name: scratch::push_str(&name),
                    value: el.get_attribute("value").map(|v| scratch::push_str(&v)),
                    kind: scratch::push_str(&kind),
                    hidden,
                    id: el.get_attribute("id").map(|v| scratch::push_str(&v)),
                });
                Ok(())
            }))
            .append_element_content_handler(lol_html::element!("title", move |_el: &mut Element<'_, '_>| {
                scratch::emit(Ev::TitleOpen);
                Ok(())
            }))
            .append_element_content_handler(lol_html::element!("meta[name='description']", move |el: &mut Element<'_, '_>| {
                if let Some(d) = el.get_attribute("content") {
                    scratch::emit(Ev::MetaDescription { span: scratch::push_str(&d) });
                }
                Ok(())
            }))
            .append_element_content_handler(lol_html::text!("script", move |t: &mut TextChunk<'_>| {
                let last = t.last_in_text_node();
                let span = scratch::push_str(t.as_str());
                scratch::emit(Ev::ScriptText { span, last });
                Ok(())
            }))
            .append_element_content_handler(lol_html::text!("title", move |t: &mut TextChunk<'_>| {
                let last = t.last_in_text_node();
                let span = scratch::push_str(t.as_str());
                scratch::emit(Ev::TitleText { span, last });
                Ok(())
            }))
            // --- Текст вне script/style: TEXT-узлы SoA-дерева ---
            .append_element_content_handler(lol_html::text!("*", move |t: &mut TextChunk<'_>| {
                let span = scratch::push_str(t.as_str());
                scratch::emit(Ev::DomText { span });
                Ok(())
            }));

        for (i, (_, sel)) in selectors.iter().enumerate() {
            let key = i as u32;
            let sel = sel.clone();
            settings = settings.append_element_content_handler(lol_html::text!(
                sel,
                move |t: &mut TextChunk<'_>| {
                    let span = scratch::push_str(t.as_str());
                    scratch::emit(Ev::Extract { key, span });
                    Ok(())
                }
            ));
        }

        let sink: Sink = Box::new(|_: &[u8]| {});
        let rewriter = lol_html::send::HtmlRewriter::new(settings, sink);

        Self {
            rewriter: Some(rewriter),
            collector: Collector::new(limits, extract_keys),
            limits,
            bytes_fed: 0,
            utf8_bad_chunks: 0,
        }
    }

    #[inline(always)]
    fn rewriter_mut(&mut self) -> &mut Rewriter {
        self.rewriter.as_mut().expect("rewriter alive until finish()")
    }
    #[inline(always)]
    fn drain(&mut self) {
        scratch::drain_into(&mut self.collector);
    }

    #[inline(always)]
    pub fn push(&mut self, chunk: &[u8]) -> Result<Flow, PipeError> {
        if self.bytes_fed >= self.limits.byte_brake {
            return Ok(Flow::Stop);
        }
        self.bytes_fed += chunk.len() as u64;
        let outcome: Result<(), PipeError> = if chunk.contains(&b'\r') {
            crate::run_task_scoped(|bump| {
                let cleaned = crate::normalize_stream(chunk, bump);
                self.rewriter_mut().write(cleaned).map_err(|_| PipeError::MemoryBrake)
            })
        } else if core_utils::utf8::basic::from_utf8(chunk).is_ok() {
            self.rewriter_mut().write(chunk).map_err(|_| PipeError::MemoryBrake)
        } else {
            self.utf8_bad_chunks += 1;
            let mut end = chunk.len();
            while end > 0 && core_utils::utf8::basic::from_utf8(&chunk[..end]).is_err() {
                end -= 1;
            }
            if end == 0 {
                Ok(())
            } else {
                self.rewriter_mut().write(&chunk[..end]).map_err(|_| PipeError::MemoryBrake)
            }
        };
        self.drain();
        outcome?;
        if self.bytes_fed >= self.limits.byte_brake {
            return Ok(Flow::Stop);
        }
        Ok(Flow::Continue)
    }

    pub fn finish(mut self) -> Result<crate::types::PageData, PipeError> {
        if let Some(r) = self.rewriter.take() {
            let _ = r.end();
        }
        self.drain();
        let truncated = self.bytes_fed >= self.limits.byte_brake;
        Ok(self.collector.into_page(self.bytes_fed, self.utf8_bad_chunks, truncated))
    }
}
