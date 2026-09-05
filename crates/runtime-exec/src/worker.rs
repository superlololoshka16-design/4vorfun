use crate::cache::NormCache;
use crate::events::{Event, EventTx};
use crate::normalize::{Lit, normalize};
use crate::polyfill::Bundle;
use crate::task::{ExecError, ExecOutcome, ExecPath, ExecReq, ProfileSnap};
use crate::touch::{self, ApiKey};
use crate::wasm::run_wasm;
use compact_str::CompactString;
use rquickjs::class::Trace;
use rquickjs::function::{Func, Function};
use rquickjs::object::{Accessor, Property};
use rquickjs::{Class, Context, Ctx, IntoJs, JsLifetime, Object, Persistent, Runtime, Type, Value};
use rand::rngs::SmallRng;
use rand::{RngCore, SeedableRng};
use smallvec::SmallVec;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

const MEM_LIMIT: usize = 8 * 1024 * 1024;
const GC_THRESHOLD: usize = 1024 * 1024;
const STACK_LIMIT: usize = 1024 * 1024;
const FN_CACHE_CAP: usize = 256;
const WASM_FUEL: u64 = 8_000_000;

struct ProfVals {
    ua: String,
    app_version: String,
    platform: String,
    locale: String,
    locale_base: String,
    screen_w: f64,
    screen_h: f64,
    mobile: bool,
    href: String,
    origin: String,
    host: String,
    cookie: String,
    webgl_vendor: CompactString,
    webgl_renderer: CompactString,
    canvas_hash_hex: CompactString,
}

impl ProfVals {
    fn defaults() -> Self {
        Self {
            ua: String::new(),
            app_version: String::new(),
            platform: String::new(),
            locale: String::new(),
            locale_base: String::new(),
            screen_w: 0.0,
            screen_h: 0.0,
            mobile: false,
            href: String::new(),
            origin: String::new(),
            host: String::new(),
            cookie: String::new(),
            webgl_vendor: CompactString::new(""),
            webgl_renderer: CompactString::new(""),
            canvas_hash_hex: CompactString::new(""),
        }
    }
}

thread_local! {
    static PROF: RefCell<ProfVals> = RefCell::new(ProfVals::defaults());
}

thread_local! {
    static DOC: RefCell<Option<Arc<parser_pipeline::PageData>>> = const { RefCell::new(None) };
}

thread_local! {
    static HANDLES: RefCell<HashMap<u32, Persistent<Object<'static>>>> = RefCell::new(HashMap::new());
}

thread_local! {
    static SCRIPTS_COL: RefCell<Option<Persistent<Object<'static>>>> = const { RefCell::new(None) };
}

thread_local! {
    static FAST_RNG: RefCell<SmallRng> = RefCell::new(SmallRng::from_os_rng());
}

#[inline]
fn with_doc<R>(f: impl FnOnce(Option<&parser_pipeline::PageData>) -> R) -> R {
    DOC.with(|c| f(c.borrow().as_deref()))
}

fn store_doc(doc: Option<Arc<parser_pipeline::PageData>>) {
    DOC.with(|c| *c.borrow_mut() = doc);
}

fn handles_clear() {
    HANDLES.with(|h| h.borrow_mut().clear());
}

fn drain_env_locals() {
    handles_clear();
    store_doc(None);
    SCRIPTS_COL.with(|c| *c.borrow_mut() = None);
}

#[inline]
fn handle_value<'js>(ctx: &Ctx<'js>, node: u32) -> rquickjs::Result<Value<'js>> {
    let cached = HANDLES
        .with(|h| h.borrow().get(&node).map(|p| p.clone().restore(ctx)))
        .transpose()?;
    if let Some(obj) = cached {
        return Ok(obj.into_value());
    }
    let class: Class<NodeHandle> = Class::instance(ctx.clone(), NodeHandle { node })?;
    let obj: Object = class.into_inner();
    HANDLES.with(|h| {
        h.borrow_mut().insert(node, Persistent::save(ctx, obj.clone()));
    });
    Ok(obj.into_value())
}

fn find_node_by_id(id: &str) -> Option<u32> {
    with_doc(|doc| {
        let dom = &doc?.dom;
        let attr_id = parser_pipeline::ATTR_NAMES.get("id").copied()?;
        let mut stack: SmallVec<[u32; 64]> = SmallVec::new();
        let mut cur = dom.children(u32::MAX).next()?;
        loop {
            if dom.attr(cur, attr_id).is_some_and(|v| v == id) {
                return Some(cur);
            }
            let mut kids = dom.children(cur);
            if let Some(first) = kids.next() {
                for k in kids {
                    stack.push(k);
                }
                cur = first;
                continue;
            }
            cur = stack.pop()?;
        }
    })
}

#[inline]
fn with_prof<R>(f: impl FnOnce(&ProfVals) -> R) -> R {
    PROF.with(|p| f(&p.borrow()))
}

fn store_prof(snap: &ProfileSnap) {
    let ua = snap.ua.to_string();
    let app_version = ua.strip_prefix("Mozilla/").unwrap_or(&ua).to_string();
    let locale_base = snap.locale.split('-').next().unwrap_or("").to_string();
    let (origin, host) = split_origin(&snap.href);
    PROF.with(|p| {
        *p.borrow_mut() = ProfVals {
            ua,
            app_version,
            platform: snap.platform.to_string(),
            locale: snap.locale.to_string(),
            locale_base,
            screen_w: snap.screen_w as f64,
            screen_h: snap.screen_h as f64,
            mobile: snap.mobile,
            href: snap.href.to_string(),
            origin,
            host,
            cookie: snap.cookie.to_string(),
            webgl_vendor: snap.webgl_vendor.clone(),
            webgl_renderer: snap.webgl_renderer.clone(),
            canvas_hash_hex: snap.canvas_hash_hex.clone(),
        }
    });
}

fn split_origin(href: &str) -> (String, String) {
    let Some(scheme_end) = href.find("://") else {
        return (String::new(), String::new());
    };
    let rest = &href[scheme_end + 3..];
    let host_len = rest
        .find(['/', '?', '#'])
        .unwrap_or(rest.len());
    let host = rest[..host_len].to_string();
    let origin = format!("{}://{}", &href[..scheme_end], host);
    (origin, host)
}

#[derive(Trace, JsLifetime, Clone)]
#[rquickjs::class(rename_all = "camelCase")]
struct NodeHandle {
    node: u32,
}

impl NodeHandle {
    fn attr(&self, name: &str) -> Option<String> {
        let name_id = parser_pipeline::ATTR_NAMES.get(name).copied()?;
        with_doc(|doc| doc.and_then(|p| p.dom.attr(self.node, name_id).map(str::to_string)))
    }
}

#[rquickjs::methods]
impl NodeHandle {
    #[qjs(get, rename = "tagName")]
    fn tag_name(&self) -> Option<String> {
        with_doc(|doc| {
            doc.and_then(|p| {
                let dom = &p.dom;
                dom.tag_name(dom.tag_id(self.node)).map(str::to_string)
            })
        })
    }

    #[qjs(get, rename = "id")]
    fn id(&self) -> Option<String> {
        self.attr("id")
    }

    #[qjs(get, rename = "className")]
    fn class_name(&self) -> Option<String> {
        self.attr("class")
    }

    #[qjs(rename = "getAttribute")]
    fn get_attribute(&self, name: String) -> Option<String> {
        self.attr(&name)
    }

    #[qjs(rename = "hasAttribute")]
    fn has_attribute(&self, name: String) -> bool {
        self.attr(&name).is_some()
    }
}

fn add_get_element_by_id<'js>(doc: &Object<'js>) -> rquickjs::Result<()> {
    let gebi = Function::new(
        doc.ctx().clone(),
        |c: Ctx<'js>, id: String| -> rquickjs::Result<Value<'js>> {
            touch::touch_log_record(ApiKey::GET_ELEMENT_BY_ID);
            match find_node_by_id(&id) {
                Some(n) => handle_value(&c, n),
                None => Ok(Value::new_null(c)),
            }
        },
    )?;
    doc.prop(
        "getElementById",
        Property::from(gebi).writable().configurable(),
    )
}

fn add_doc_scripts<'js>(ctx: &Ctx<'js>, doc: &Object<'js>) -> rquickjs::Result<()> {
    let scripts_col = build_scripts_collection(ctx)?;
    SCRIPTS_COL.with(|c| {
        *c.borrow_mut() = Some(Persistent::save(ctx, scripts_col));
    });
    doc.prop(
        "scripts",
        Accessor::from(move |c: Ctx<'js>| -> rquickjs::Result<Value<'js>> {
            touch::touch_log_record(ApiKey::SCRIPTS);
            SCRIPTS_COL.with(|cell| match cell.borrow().as_ref() {
                Some(p) => Ok(p.clone().restore(&c)?.into_value()),
                None => Ok(Value::new_null(c)),
            })
        })
        .enumerable()
        .configurable(),
    )
}

fn build_crypto<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    let crypto = Object::new(ctx.clone())?;
    let grv = Function::new(
        ctx.clone(),
        |c: Ctx<'js>, val: Value<'js>| -> rquickjs::Result<Value<'js>> {
            touch::touch_log_record(ApiKey::CIPHERS);
            let ta = val
                .as_object()
                .and_then(|o| o.as_typed_array::<u8>())
                .and_then(|t| t.as_raw().map(|raw| (t.len(), raw)));
            let Some((len, raw)) = ta else {
                return Err(rquickjs::Exception::throw_message(
                    &c,
                    "TypeMismatchError: Argument 1 of Crypto.getRandomValues is not an ArrayBufferView",
                ));
            };
            if len > 65536 {
                return Err(rquickjs::Exception::throw_message(
                    &c,
                    "QuotaExceededError: The requested length exceeds 65,536 bytes",
                ));
            }
            let view = unsafe { std::slice::from_raw_parts_mut(raw.ptr.as_ptr(), raw.len) };
            FAST_RNG.with(|rng| rng.borrow_mut().fill_bytes(view));
            Ok(val)
        },
    )?;
    crypto.prop(
        "getRandomValues",
        Property::from(grv).writable().configurable(),
    )?;
    let uuid = Function::new(ctx.clone(), |c: Ctx<'js>| -> rquickjs::Result<Value<'js>> {
        touch::touch_log_record(ApiKey::CIPHERS);
        let mut b = [0u8; 16];
        FAST_RNG.with(|rng| rng.borrow_mut().fill_bytes(&mut b));
        b[6] = (b[6] & 0x0f) | 0x40;
        b[8] = (b[8] & 0x3f) | 0x80;
        let hex = b"0123456789abcdef";
        let mut s = String::with_capacity(36);
        for (i, byte) in b.iter().enumerate() {
            if i == 4 || i == 6 || i == 8 || i == 10 {
                s.push('-');
            }
            s.push(hex[(*byte >> 4) as usize] as char);
            s.push(hex[(*byte & 0xf) as usize] as char);
        }
        s.into_js(&c)
    })?;
    crypto.prop(
        "randomUUID",
        Property::from(uuid).writable().configurable(),
    )?;
    Ok(crypto)
}

fn build_scripts_collection<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Object<'js>> {
    let col = Object::new(ctx.clone())?;
    col.prop(
        "length",
        Accessor::from(|| -> u32 {
            with_doc(|doc| doc.map_or(0, |p| p.dom.scripts.len() as u32))
        })
        .enumerable()
        .configurable(),
    )?;
    let item = Function::new(
        ctx.clone(),
        |c: Ctx<'js>, i: u32| -> rquickjs::Result<Value<'js>> {
            touch::touch_log_record(ApiKey::SCRIPTS);
            let node = with_doc(|doc| doc.and_then(|p| p.dom.scripts.get(i as usize).copied()));
            match node {
                Some(n) => handle_value(&c, n),
                None => Ok(Value::new_null(c)),
            }
        },
    )?;
    col.prop("item", Property::from(item).writable().configurable())?;
    Ok(col)
}

fn add_acc<'js, R, F>(
    obj: &Object<'js>,
    key: &str,
    api: u32,
    f: F,
) -> rquickjs::Result<()>
where
    R: IntoJs<'js> + 'js,
    F: Fn(&ProfVals) -> R + 'js,
{
    obj.prop(
        key,
        Accessor::from(move || {
            touch::touch_log_record(api);
            with_prof(|p| f(p))
        })
        .enumerable()
        .configurable(),
    )
}

fn add_acc_null<'js>(obj: &Object<'js>, key: &str, api: u32) -> rquickjs::Result<()> {
    obj.prop(
        key,
        Accessor::from(move |c: Ctx<'js>| -> Value<'js> {
            touch::touch_log_record(api);
            Value::new_null(c)
        })
        .enumerable()
        .configurable(),
    )
}

fn add_acc_undef<'js>(
    obj: &Object<'js>,
    key: &str,
    api: u32,
) -> rquickjs::Result<()> {
    obj.prop(
        key,
        Accessor::from(move |c: Ctx<'js>| -> Value<'js> {
            touch::touch_log_record(api);
            Value::new_undefined(c)
        })
        .enumerable()
        .configurable(),
    )
}

fn add_acc_connection<'js>(
    obj: &Object<'js>,
    key: &str,
    api: u32,
) -> rquickjs::Result<()> {
    obj.prop(
        key,
        Accessor::from(move |c: Ctx<'js>| -> rquickjs::Result<Value<'js>> {
            touch::touch_log_record(api);
            let o = Object::new(c.clone())?;
            o.set("effectiveType", "4g")?;
            o.set("rtt", 50i32)?;
            o.set("downlink", 10f64)?;
            o.set("saveData", false)?;
            Ok(o.into_value())
        })
        .enumerable()
        .configurable(),
    )
}

fn add_method_null<'js>(
    obj: &Object<'js>,
    key: &str,
    api: u32,
) -> rquickjs::Result<()> {
    obj.prop(
        key,
        Property::from(Func::from(move |c: Ctx<'js>| -> Value<'js> {
            touch::touch_log_record(api);
            Value::new_null(c)
        }))
        .writable()
        .configurable(),
    )
}

fn add_method_empty<'js>(
    obj: &Object<'js>,
    key: &str,
    api: u32,
) -> rquickjs::Result<()> {
    obj.prop(
        key,
        Property::from(Func::from(move || -> Vec<Value<'js>> {
            touch::touch_log_record(api);
            Vec::new()
        }))
        .writable()
        .configurable(),
    )
}

struct JsEnv {
    sha256: Persistent<Function<'static>>,
    md5: Persistent<Function<'static>>,
    fns: RefCell<HashMap<(u64, u64), Persistent<Function<'static>>>>,
    deadline_ms: Arc<AtomicU64>,
    seed: Arc<AtomicU64>,
    epoch: Instant,
    context: Context,
}

fn native_sha256<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Function<'js>> {
    Function::new(ctx.clone(), move |s: String| -> String {
        let mut out = [0u8; 64];
        payload_gen::sha256_hex_into(s.as_bytes(), &mut out);
        unsafe { String::from_utf8_unchecked(out.to_vec()) }
    })
}

fn native_md5<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Function<'js>> {
    Function::new(ctx.clone(), move |s: String| -> String {
        let mut out = [0u8; 32];
        payload_gen::md5_hex_into(s.as_bytes(), &mut out);
        unsafe { String::from_utf8_unchecked(out.to_vec()) }
    })
}

fn native_profile<'js>(ctx: &Ctx<'js>) -> rquickjs::Result<Function<'js>> {
    Function::new(ctx.clone(), move |c: Ctx<'js>| -> rquickjs::Result<Value<'js>> {
        let o = Object::new(c)?;
        with_prof(|p| -> rquickjs::Result<()> {
            o.set("ua", p.ua.as_str())?;
            o.set("platform", p.platform.as_str())?;
            o.set("locale", p.locale.as_str())?;
            o.set("screenW", p.screen_w)?;
            o.set("screenH", p.screen_h)?;
            o.set("mobile", p.mobile)?;
            o.set("webglVendor", p.webgl_vendor.as_str())?;
            o.set("webglRenderer", p.webgl_renderer.as_str())?;
            o.set("canvasHash", p.canvas_hash_hex.as_str())?;
            Ok(())
        })?;
        Ok(o.into_value())
    })
}

impl JsEnv {
    fn new(
        bundle: &Bundle,
        deadline_ms: Arc<AtomicU64>,
        seed: Arc<AtomicU64>,
    ) -> Result<Self, rquickjs::Error> {
        let runtime = Runtime::new()?;
        runtime.set_memory_limit(MEM_LIMIT);
        runtime.set_gc_threshold(GC_THRESHOLD);
        runtime.set_max_stack_size(STACK_LIMIT);
        let epoch = Instant::now();
        let d = deadline_ms.clone();
        runtime.set_interrupt_handler(Some(Box::new(move || {
            epoch.elapsed().as_millis() as u64 >= d.load(Ordering::Relaxed)
        })));
        let context = Context::full(&runtime)?;
        let (sha256, md5) = context.with(|ctx| -> rquickjs::Result<(
            Persistent<Function<'static>>,
            Persistent<Function<'static>>,
        )> {
            let sha = native_sha256(&ctx)?;
            let md5 = native_md5(&ctx)?;
            Ok((Persistent::save(&ctx, sha), Persistent::save(&ctx, md5)))
        })?;
        context.with(|ctx| {
            let globals = ctx.globals();
            let perf = Object::new(ctx.clone())?;
            let j = seed.clone();
            let e = epoch;
            perf.set("timeOrigin", 0f64)?;
            perf.prop(
                "now",
                Property::from(Func::from(move || -> f64 {
                    touch::touch_log_record(ApiKey::NOW);
                    let mut s = j.load(Ordering::Relaxed);
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    j.store(s, Ordering::Relaxed);
                    let jit = ((s >> 40) & 0x3f) as f64 * 0.012;
                    e.elapsed().as_secs_f64() * 1000.0 + jit
                }))
                .writable()
                .configurable(),
            )?;
            globals.prop("performance", Property::from(perf).configurable())?;

            let nav = Object::new(ctx.clone())?;
            add_acc(&nav, "userAgent", ApiKey::USER_AGENT, |p| p.ua.clone())?;
            add_acc(&nav, "appVersion", ApiKey::APP_VERSION, |p| {
                p.app_version.clone()
            })?;
            add_acc(&nav, "platform", ApiKey::PLATFORM, |p| p.platform.clone())?;
            add_acc(&nav, "language", ApiKey::LANGUAGE, |p| p.locale.clone())?;
            nav.prop(
                "languages",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::LANGUAGES);
                    with_prof(|p| vec![p.locale.clone(), p.locale_base.clone()])
                })
                .enumerable()
                .configurable(),
            )?;
            add_acc(
                &nav,
                "hardwareConcurrency",
                ApiKey::HARDWARE_CONCURRENCY,
                |_| 8u32,
            )?;
            add_acc(&nav, "deviceMemory", ApiKey::DEVICE_MEMORY, |_| 8u32)?;
            add_acc(&nav, "cookieEnabled", ApiKey::COOKIE_ENABLED, |_| true)?;
            add_acc(&nav, "webdriver", ApiKey::WEBDRIVER, |_| false)?;
            add_acc(
                &nav,
                "maxTouchPoints",
                ApiKey::MAX_TOUCH_POINTS,
                |p| if p.mobile { 5u32 } else { 0u32 },
            )?;
            add_acc(&nav, "productSub", ApiKey::PRODUCT_SUB, |_| {
                "20030107".to_string()
            })?;
            add_acc(&nav, "product", ApiKey::PRODUCT, |_| "Gecko".to_string())?;
            add_acc(&nav, "vendor", ApiKey::VENDOR, |_| {
                "Google Inc.".to_string()
            })?;
            add_acc(&nav, "oscpu", ApiKey::OSCPU, |_| {
                "Windows NT 10.0".to_string()
            })?;
            add_acc(&nav, "buildID", ApiKey::BUILD_ID, |_| {
                "20181001000000".to_string()
            })?;
            add_acc_null(&nav, "doNotTrack", ApiKey::DO_NOT_TRACK)?;
            add_acc_null(
                &nav,
                "globalPrivacyControl",
                ApiKey::GLOBAL_PRIVACY_CONTROL,
            )?;
            add_acc(&nav, "pdfViewerEnabled", ApiKey::PDF_VIEWER, |_| true)?;
            add_acc_connection(&nav, "connection", ApiKey::CONNECTION)?;
            add_acc_undef(&nav, "userAgentData", ApiKey::USER_AGENT_DATA)?;
            globals.prop("navigator", Property::from(nav).configurable())?;

            let scr = Object::new(ctx.clone())?;
            add_acc(&scr, "width", ApiKey::WIDTH, |p| p.screen_w)?;
            add_acc(&scr, "height", ApiKey::HEIGHT, |p| p.screen_h)?;
            add_acc(&scr, "availWidth", ApiKey::AVAIL_WIDTH, |p| p.screen_w)?;
            add_acc(
                &scr,
                "availHeight",
                ApiKey::AVAIL_HEIGHT,
                |p| p.screen_h - 40.0,
            )?;
            add_acc(&scr, "colorDepth", ApiKey::COLOR_DEPTH, |_| 24u32)?;
            add_acc(&scr, "pixelDepth", ApiKey::PIXEL_DEPTH, |_| 24u32)?;
            globals.prop("screen", Property::from(scr).configurable())?;

            let loc = Object::new(ctx.clone())?;
            add_acc(&loc, "href", ApiKey::LOCATION_HREF, |p| p.href.clone())?;
            add_acc(&loc, "origin", ApiKey::LOCATION_ORIGIN, |p| {
                p.origin.clone()
            })?;
            add_acc(&loc, "host", ApiKey::LOCATION_HOST, |p| p.host.clone())?;
            add_acc(&loc, "hostname", ApiKey::LOCATION_HOST, |p| p.host.clone())?;
            add_acc(&loc, "pathname", ApiKey::LOCATION_PATHNAME, |_| {
                "/".to_string()
            })?;
            add_acc(&loc, "protocol", ApiKey::LOCATION_PROTOCOL, |_| {
                "https:".to_string()
            })?;
            add_acc(&loc, "search", ApiKey::LOCATION_PATHNAME, |_| {
                String::new()
            })?;
            add_acc(&loc, "hash", ApiKey::LOCATION_PATHNAME, |_| String::new())?;
            globals.prop("location", Property::from(loc).configurable())?;

            let doc = Object::new(ctx.clone())?;
            add_acc(&doc, "cookie", ApiKey::COOKIE, |p| p.cookie.clone())?;
            add_acc(&doc, "referrer", ApiKey::REFERRER, |_| String::new())?;
            add_acc(&doc, "title", ApiKey::TITLE, |_| String::new())?;
            add_acc(&doc, "URL", ApiKey::URL, |p| p.href.clone())?;
            add_acc(&doc, "origin", ApiKey::ORIGIN, |p| p.origin.clone())?;
            add_acc(&doc, "domain", ApiKey::DOMAIN, |p| p.host.clone())?;
            add_acc(&doc, "readyState", ApiKey::READY_STATE, |_| {
                "complete".to_string()
            })?;
            add_acc(&doc, "visibilityState", ApiKey::VISIBILITY, |_| {
                "visible".to_string()
            })?;
            add_acc(&doc, "hidden", ApiKey::VISIBILITY, |_| false)?;
            add_acc(&doc, "characterSet", ApiKey::CHARACTER_SET, |_| {
                "UTF-8".to_string()
            })?;
            add_acc(&doc, "contentType", ApiKey::CONTENT_TYPE, |_| {
                "text/html".to_string()
            })?;
            add_method_null(&doc, "querySelector", ApiKey::QUERY_SELECTOR)?;
            add_get_element_by_id(&doc)?;
            add_method_empty(&doc, "querySelectorAll", ApiKey::QUERY_SELECTOR_ALL)?;
            add_method_empty(
                &doc,
                "getElementsByTagName",
                ApiKey::GET_ELEMENTS_BY_TAG_NAME,
            )?;
            add_doc_scripts(&ctx, &doc)?;
            globals.prop("document", Property::from(doc).configurable())?;
            let crypto = build_crypto(&ctx)?;
            globals.prop("crypto", Property::from(crypto).configurable())?;

            globals.prop(
                "innerWidth",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::INNER_WIDTH);
                    with_prof(|p| p.screen_w)
                })
                .enumerable()
                .configurable(),
            )?;
            globals.prop(
                "innerHeight",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::INNER_HEIGHT);
                    with_prof(|p| p.screen_h - 80.0)
                })
                .enumerable()
                .configurable(),
            )?;
            globals.prop(
                "outerWidth",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::OUTER_WIDTH);
                    with_prof(|p| p.screen_w)
                })
                .enumerable()
                .configurable(),
            )?;
            globals.prop(
                "outerHeight",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::OUTER_HEIGHT);
                    with_prof(|p| p.screen_h)
                })
                .enumerable()
                .configurable(),
            )?;
            globals.prop(
                "devicePixelRatio",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::DEVICE_PIXEL_RATIO);
                    1f64
                })
                .enumerable()
                .configurable(),
            )?;
            globals.prop(
                "screenX",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::SCREEN_X);
                    0i32
                })
                .enumerable()
                .configurable(),
            )?;
            globals.prop(
                "screenY",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::SCREEN_Y);
                    0i32
                })
                .enumerable()
                .configurable(),
            )?;
            globals.prop(
                "pageXOffset",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::PAGE_X_OFFSET);
                    0f64
                })
                .enumerable()
                .configurable(),
            )?;
            globals.prop(
                "pageYOffset",
                Accessor::from(|| {
                    touch::touch_log_record(ApiKey::PAGE_Y_OFFSET);
                    0f64
                })
                .enumerable()
                .configurable(),
            )?;

            let math: Object = ctx.eval("Math")?;
            let m = seed.clone();
            math.prop(
                "random",
                Property::from(Func::from(move || -> f64 {
                    touch::touch_log_record(ApiKey::RANDOM);
                    let mut s = m.load(Ordering::Relaxed);
                    s ^= s << 13;
                    s ^= s >> 7;
                    s ^= s << 17;
                    m.store(s, Ordering::Relaxed);
                    (s >> 11) as f64 / 9007199254740992.0
                }))
                .writable()
                .configurable(),
            )?;
            let p_getter = native_profile(&ctx)?;
            let bundle_fn: Function = ctx.eval(bundle.as_str())?;
            let _: Value = bundle_fn.call((p_getter,))?;
            Ok::<(), rquickjs::Error>(())
        })?;
        Ok(Self {
            context,
            deadline_ms,
            seed,
            epoch,
            sha256,
            md5,
            fns: RefCell::new(HashMap::new()),
        })
    }

    fn since_epoch_ms(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }

    fn exec(
        &mut self,
        domain: u64,
        skel: u64,
        src: &Arc<str>,
        args: &[Lit],
        snap: &ProfileSnap,
        timeout: Duration,
    ) -> (Result<Option<CompactString>, ExecError>, bool) {
        self.deadline_ms.store(
            self.since_epoch_ms() + timeout.as_millis() as u64,
            Ordering::Relaxed,
        );
        self.seed.store(snap.seed, Ordering::Relaxed);
        store_prof(snap);
        let mut local_hit = false;
        let result: Result<Option<CompactString>, rquickjs::Error> = {
            let JsEnv { context, fns, sha256, md5, .. } = self;
            context.with(|ctx| {
                let sha = sha256.clone().restore(&ctx)?;
                let md = md5.clone().restore(&ctx)?;
                let mut cache = fns.borrow_mut();
                let func: Function = if let Some(f) = cache.get(&(domain, skel)) {
                    local_hit = true;
                    f.clone().restore(&ctx)?
                } else {
                    if cache.len() >= FN_CACHE_CAP {
                        cache.clear();
                    }
                    let val: Value = ctx.eval(src.as_ref())?;
                    if !matches!(val.type_of(), Type::Function | Type::Constructor) {
                        return Err(rquickjs::Error::new_from_js_message(
                            "Value",
                            "Function",
                            "normalized source did not yield function",
                        ));
                    }
                    let f: Function = val.get()?;
                    cache.insert((domain, skel), Persistent::save(&ctx, f.clone()));
                    f
                };
                drop(cache);
                let arr = rquickjs::Array::new(ctx.clone())?;
                for (i, lit) in args.iter().enumerate() {
                    match lit {
                        Lit::Num(f) => arr.set(i, *f)?,
                        Lit::Str(s) => arr.set(i, s.as_str())?,
                    }
                }
                let out: Value = func.call((arr, sha, md))?;
                Ok(coerce(out))
            })
        };
        self.deadline_ms.store(u64::MAX, Ordering::Relaxed);
        let token = match result {
            Ok(Some(t)) => Ok(Some(t)),
            Ok(None) => Err(ExecError::NoResult),
            Err(e) => Err(classify(&e, &self.context)),
        };
        (token, local_hit)
    }
}

#[inline(always)]
fn format_worker_error<E: core::fmt::Display>(err: E) -> CompactString {
    use core::fmt::Write;
    let mut s = compact_str::CompactString::new("");
    let _ = write!(s, "{err}");
    s
}

fn num_token(n: impl std::fmt::Display) -> CompactString {
    compact_str::ToCompactString::to_compact_string(&n)
}

fn coerce(v: Value<'_>) -> Option<CompactString> {
    match v.type_of() {
        Type::String => v.get::<String>().ok().map(CompactString::from),
        Type::Int => v.get::<i32>().ok().map(num_token),
        Type::Float => v.get::<f64>().ok().map(num_token),
        Type::Bool => v
            .get::<bool>()
            .ok()
            .map(|b| CompactString::new(if b { "true" } else { "false" })),
        _ => None,
    }
}

fn classify(err: &rquickjs::Error, context: &Context) -> ExecError {
    if !matches!(err, rquickjs::Error::Exception) {
        return ExecError::Js(format_worker_error(err).into());
    }
    context.with(|ctx| {
        let caught = ctx.catch();
        if caught.is_uncatchable_error() {
            return ExecError::Timeout;
        }
        let exc: Option<String> = if let Some(e) = caught.as_exception() {
            e.message()
        } else if let Some(obj) = caught.as_object().cloned() {
            rquickjs::Exception::from_object(obj).and_then(|e| e.message())
        } else {
            caught.get::<String>().ok()
        };
        if let Some(msg) = exc {
            if msg.contains("out of memory") {
                return ExecError::Oom;
            }
            return ExecError::Js(msg);
        }
        ExecError::Oom
    })
}

struct Worker {
    env: JsEnv,
    bundle: Arc<Bundle>,
    cache: Arc<NormCache>,
    events: EventTx,
}

impl Worker {
    fn new(bundle: Arc<Bundle>, cache: Arc<NormCache>, events: EventTx) -> Option<Self> {
        let deadline = Arc::new(AtomicU64::new(u64::MAX));
        let seed = Arc::new(AtomicU64::new(0));
        let env = JsEnv::new(&bundle, deadline, seed).ok()?;
        Some(Self {
            env,
            bundle,
            cache,
            events,
        })
    }

    fn rebuild_env(&mut self) {
        let deadline = self.env.deadline_ms.clone();
        let seed = self.env.seed.clone();
        if let Ok(env) = JsEnv::new(&self.bundle, deadline, seed) {
            self.env = env;
            let _ = self.events.try_send(Event::Oom);
        }
    }

    fn run(&mut self, req: ExecReq) -> ExecOutcome {
        let start = Instant::now();
        touch::touch_log_reset();
        handles_clear();
        store_doc(req.doc.clone());
        if req.script.len() >= 4 && req.script[..4] == [0x00, 0x61, 0x73, 0x6d] {
            let (token, err) = match run_wasm(&req.script, WASM_FUEL) {
                Ok(tok) => {
                    let _ = self.events.try_send(Event::WasmRun);
                    (Some(tok), None)
                }
                Err(e) => {
                    let _ = self.events.try_send(Event::WasmFail);
                    let err =
                        if e.downcast_ref::<wasmtime::Trap>() == Some(&wasmtime::Trap::OutOfFuel) {
                            ExecError::WasmFuel
                        } else if e.to_string().contains("imports unsupported") {
                            ExecError::WasmImports
                        } else {
                            ExecError::WasmCompile
                        };
                    (None, Some(err))
                }
            };
            return ExecOutcome {
                token,
                path: ExecPath::Wasm,
                cache_hit: false,
                elapsed: start.elapsed(),
                err,
                touches: touch::touch_log_count(),
            };
        }
        let domain = req.domain;
        let raw = NormCache::raw_hash(&req.script);
        let (skel, args, raw_hit) = match self.cache.lookup_raw(domain, raw) {
            Some((skel, args)) => {
                let _ = self.events.try_send(Event::CacheHit);
                (skel, args, true)
            }
            None => {
                let _ = self.events.try_send(Event::CacheMiss);
                match normalize(&req.script) {
                    Ok(n) => {
                        let skel = n.skel;
                        let src = n.src;
                        let args = Arc::new(n.args);
                        self.cache.put_raw(domain, raw, skel, args.as_ref().clone());
                        self.cache.put_src(domain, skel, src);
                        (skel, args, false)
                    }
                    Err(()) => {
                        let _ = self.events.try_send(Event::ParseFail);
                        return ExecOutcome {
                            token: None,
                            path: ExecPath::Compile,
                            cache_hit: false,
                            elapsed: start.elapsed(),
                            err: Some(ExecError::Parse),
                            touches: touch::touch_log_count(),
                        };
                    }
                }
            }
        };
        let src = match self.cache.lookup_src(domain, skel) {
            Some(src) => src,
            None => match normalize(&req.script) {
                Ok(n) => {
                    self.cache.put_src(domain, skel, n.src.clone());
                    n.src
                }
                Err(()) => {
                    let _ = self.events.try_send(Event::ParseFail);
                    return ExecOutcome {
                        token: None,
                        path: ExecPath::Compile,
                        cache_hit: false,
                        elapsed: start.elapsed(),
                        err: Some(ExecError::Parse),
                        touches: touch::touch_log_count(),
                    };
                }
            },
        };
        let (token, local_hit) = self
            .env
            .exec(domain, skel, &src, &args, &req.snap, req.timeout);
        if token.is_ok() && !local_hit {
            let _ = self.events.try_send(Event::Compile);
        }
        let path = if raw_hit {
            ExecPath::RawHit
        } else if local_hit {
            ExecPath::NormHit
        } else {
            ExecPath::Compile
        };
        let (token_opt, err) = match token {
            Ok(t) => (t, None),
            Err(e) => (None, Some(e)),
        };
        if let Some(e) = &err {
            let ev = match e {
                ExecError::Timeout => Event::Timeout,
                ExecError::Oom => Event::Oom,
                ExecError::NoResult => Event::NoResult,
                _ => Event::ExecFail,
            };
            let _ = self.events.try_send(ev);
            if matches!(e, ExecError::Oom) {
                self.rebuild_env();
            }
        }
        let _ = self
            .events
            .try_send(Event::ExecDone(start.elapsed().as_millis() as u64));
        ExecOutcome {
            token: token_opt,
            path,
            cache_hit: raw_hit || local_hit,
            elapsed: start.elapsed(),
            err,
            touches: touch::touch_log_count(),
        }
    }
}

pub(crate) fn worker_main(
    idx: usize,
    rx: crossbeam_channel::Receiver<crate::task::ExecTask>,
    bundle: Arc<Bundle>,
    events: EventTx,
    cache: Arc<NormCache>,
) {
    if let Some(cores) = core_affinity::get_core_ids()
        && !cores.is_empty()
    {
        let pick = cores[(idx + 1) % cores.len()];
        let _ = core_affinity::set_for_current(pick);
    }
    let mut worker = match Worker::new(bundle, cache, events) {
        Some(w) => w,
        None => {
            tracing::error!(worker = idx, "quickjs env init failed");
            return;
        }
    };
    for task in rx {
        let req = task.req;
        let outcome =
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| worker.run(req))) {
                Ok(o) => o,
                Err(_) => ExecOutcome {
                    token: None,
                    path: ExecPath::Compile,
                    cache_hit: false,
                    elapsed: Duration::ZERO,
                    err: Some(ExecError::Panic),
                    touches: 0,
                },
            };
        let _ = task.reply.send(outcome);
    }
    drain_env_locals();
}
