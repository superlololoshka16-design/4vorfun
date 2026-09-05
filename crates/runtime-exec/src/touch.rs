use std::cell::Cell;
use std::fmt;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::LazyLock;

/// Интернированные имена API. Индекс = бит в маске. Имена никогда
/// не появляются в JS-контексте: строка живёт только в дампе.
pub struct ApiKey;

impl ApiKey {
    pub const USER_AGENT: u32 = 0;
    pub const LANGUAGE: u32 = 1;
    pub const LANGUAGES: u32 = 2;
    pub const PLATFORM: u32 = 3;
    pub const HARDWARE_CONCURRENCY: u32 = 4;
    pub const DEVICE_MEMORY: u32 = 5;
    pub const VENDOR: u32 = 6;
    pub const APP_VERSION: u32 = 7;
    pub const PRODUCT: u32 = 8;
    pub const PRODUCT_SUB: u32 = 9;
    pub const OSCPU: u32 = 10;
    pub const BUILD_ID: u32 = 11;
    pub const GET_PLATFORM: u32 = 12;
    pub const JAVA_ENABLED: u32 = 13;
    pub const COOKIE_ENABLED: u32 = 14;
    pub const DO_NOT_TRACK: u32 = 15;
    pub const GLOBAL_PRIVACY_CONTROL: u32 = 16;
    pub const PDF_VIEWER: u32 = 17;
    pub const CONNECTION: u32 = 18;
    pub const USER_AGENT_DATA: u32 = 19;
    pub const WIDTH: u32 = 20;
    pub const HEIGHT: u32 = 21;
    pub const AVAIL_WIDTH: u32 = 22;
    pub const AVAIL_HEIGHT: u32 = 23;
    pub const COLOR_DEPTH: u32 = 24;
    pub const PIXEL_DEPTH: u32 = 25;
    pub const DEVICE_PIXEL_RATIO: u32 = 26;
    pub const SCREEN: u32 = 27;
    pub const SCREEN_X: u32 = 28;
    pub const SCREEN_Y: u32 = 29;
    pub const PAGE_X_OFFSET: u32 = 30;
    pub const PAGE_Y_OFFSET: u32 = 31;
    pub const INNER_WIDTH: u32 = 32;
    pub const INNER_HEIGHT: u32 = 33;
    pub const OUTER_WIDTH: u32 = 34;
    pub const OUTER_HEIGHT: u32 = 35;
    pub const TITLE: u32 = 36;
    pub const REFERRER: u32 = 37;
    pub const URL: u32 = 38;
    pub const DOMAIN: u32 = 39;
    pub const ORIGIN: u32 = 40;
    pub const READY_STATE: u32 = 41;
    pub const CHARACTER_SET: u32 = 42;
    pub const CONTENT_TYPE: u32 = 43;
    pub const COOKIE: u32 = 44;
    pub const HEAD: u32 = 45;
    pub const BODY: u32 = 46;
    pub const SCRIPTS: u32 = 47;
    pub const FORMS: u32 = 48;
    pub const IMAGES: u32 = 49;
    pub const LINKS: u32 = 50;
    pub const EMBEDS: u32 = 51;
    pub const PLUGINS: u32 = 52;
    pub const MIME_TYPES: u32 = 53;
    pub const QUERY_SELECTOR: u32 = 54;
    pub const QUERY_SELECTOR_ALL: u32 = 55;
    pub const GET_ELEMENT_BY_ID: u32 = 56;
    pub const GET_ELEMENTS_BY_TAG_NAME: u32 = 57;
    pub const GET_ELEMENTS_BY_CLASS_NAME: u32 = 58;
    pub const GET_ELEMENTS_BY_NAME: u32 = 59;
    pub const CREATE_ELEMENT: u32 = 60;
    pub const CANVAS_CONTEXT: u32 = 61;
    pub const CANVAS_TO_DATA_URL: u32 = 62;
    pub const WEBGL_CONTEXT: u32 = 63;
    pub const WEBGL2_CONTEXT: u32 = 64;
    pub const GET_SUPPORTED_EXTENSIONS: u32 = 65;
    pub const GET_PARAMETER: u32 = 66;
    pub const GET_CONTEXT_ATTRIBUTES: u32 = 67;
    pub const UNMASKED_VENDOR: u32 = 68;
    pub const UNMASKED_RENDERER: u32 = 69;
    pub const STORAGE_GET_ITEM: u32 = 70;
    pub const STORAGE_SET_ITEM: u32 = 71;
    pub const STORAGE_REMOVE_ITEM: u32 = 72;
    pub const STORAGE_KEY: u32 = 73;
    pub const STORAGE_LENGTH: u32 = 74;
    pub const CIPHERS: u32 = 75;
    pub const NOW: u32 = 76;
    pub const RANDOM: u32 = 77;
    pub const GET_BOUNDING_CLIENT_RECT: u32 = 78;
    pub const PLUGINS_LENGTH: u32 = 79;
    pub const WEBDRIVER: u32 = 80;
    pub const MAX_TOUCH_POINTS: u32 = 81;
    pub const LOCATION_HREF: u32 = 82;
    pub const LOCATION_ORIGIN: u32 = 83;
    pub const LOCATION_HOST: u32 = 84;
    pub const LOCATION_PATHNAME: u32 = 85;
    pub const LOCATION_PROTOCOL: u32 = 86;
    pub const VISIBILITY: u32 = 87;
    pub const TOTAL: u32 = 88;
}

/// Имена для холодного дампа: индекс → строка. Ноль строк в горячем пути.
static KEY_NAMES: LazyLock<[&'static str; ApiKey::TOTAL as usize]> = LazyLock::new(|| {
    [
        "navigator.userAgent",
        "navigator.language",
        "navigator.languages",
        "navigator.platform",
        "navigator.hardwareConcurrency",
        "navigator.deviceMemory",
        "navigator.vendor",
        "navigator.appVersion",
        "navigator.product",
        "navigator.productSub",
        "navigator.oscpu",
        "navigator.buildID",
        "navigator.getPlatform",
        "navigator.javaEnabled",
        "navigator.cookieEnabled",
        "navigator.doNotTrack",
        "navigator.globalPrivacyControl",
        "navigator.pdfViewerEnabled",
        "navigator.connection",
        "navigator.userAgentData",
        "screen.width",
        "screen.height",
        "screen.availWidth",
        "screen.availHeight",
        "screen.colorDepth",
        "screen.pixelDepth",
        "window.devicePixelRatio",
        "window.screen",
        "window.screenX",
        "window.screenY",
        "window.pageXOffset",
        "window.pageYOffset",
        "window.innerWidth",
        "window.innerHeight",
        "window.outerWidth",
        "window.outerHeight",
        "document.title",
        "document.referrer",
        "document.URL",
        "document.domain",
        "document.origin",
        "document.readyState",
        "document.characterSet",
        "document.contentType",
        "document.cookie",
        "document.head",
        "document.body",
        "document.scripts",
        "document.forms",
        "document.images",
        "document.links",
        "document.embeds",
        "navigator.plugins",
        "navigator.mimeTypes",
        "document.querySelector",
        "document.querySelectorAll",
        "document.getElementById",
        "document.getElementsByTagName",
        "document.getElementsByClassName",
        "document.getElementsByName",
        "document.createElement",
        "HTMLCanvasElement.getContext",
        "HTMLCanvasElement.toDataURL",
        "HTMLCanvasElement.getContext(webgl)",
        "HTMLCanvasElement.getContext(webgl2)",
        "WebGLRenderingContext.getSupportedExtensions",
        "WebGLRenderingContext.getParameter",
        "WebGLRenderingContext.getContextAttributes",
        "WebGLRenderingContext.UNMASKED_VENDOR_WEBGL",
        "WebGLRenderingContext.UNMASKED_RENDERER_WEBGL",
        "Storage.getItem",
        "Storage.setItem",
        "Storage.removeItem",
        "Storage.key",
        "Storage.length",
        "crypto.getRandomValues",
        "performance.now",
        "Math.random",
        "Element.getBoundingClientRect",
        "navigator.plugins.length",
        "navigator.webdriver",
        "navigator.maxTouchPoints",
        "location.href",
        "location.origin",
        "location.host",
        "location.pathname",
        "location.protocol",
        "document.visibilityState",
    ]
});

/// Дампа-имена: O(1) по индексу, без линейных поисков.
#[inline]
pub fn key_name(idx: u32) -> &'static str {
    KEY_NAMES[idx as usize]
}

/// Битовая маска касаний: один u64 на каждые 64 API-ключа. Запись —
/// OR одного бита, lock-free. Итог за задачу — ровно `words()` слов
/// вместо вектора записей на каждое обращение.
#[repr(align(64))]
pub struct TouchLog {
    words: [Cell<u64>; TouchLog::words()],
}

impl TouchLog {
    pub const fn words() -> usize {
        (ApiKey::TOTAL as usize + 63) / 64
    }

    #[inline(always)]
    pub fn record(&self, key: u32) {
        let idx = key as usize;
        debug_assert!(idx < ApiKey::TOTAL as usize);
        let w = &self.words[idx >> 6];
        w.set(w.get() | (1u64 << (idx & 63)));
    }

    pub fn clear(&self) {
        for w in &self.words {
            w.set(0);
        }
    }

    /// Число затронутых API (popcount по маске) — итог в ExecOutcome.
    pub fn count(&self) -> u32 {        let mut n = 0u32;
        for w in &self.words {
            n += w.get().count_ones();
        }
        n
    }

    /// Какие ключи тронуты: возвращает индексы установленных битов
    /// в холодный дамп. Порядок — по возрастанию индекса.
    pub fn touched_keys(&self) -> impl Iterator<Item = u32> + '_ {
        self.words
            .iter()
            .enumerate()
            .flat_map(|(wi, w)| {
                let mut x = w.get();
                (0..64).filter_map(move |_| {
                    if x == 0 {
                        return None;
                    }
                    let bit = x.trailing_zeros();
                    x &= x - 1;
                    Some((wi as u32) * 64 + bit)
                })
            })
            .take(ApiKey::TOTAL as usize)
    }
}

/// Счётчик номеров задач — просто чтобы дамп снабдить контекстом.
static TASK_SEQ: AtomicU32 = AtomicU32::new(0);

pub fn next_task_seq() -> u32 {
    TASK_SEQ.fetch_add(1, Ordering::Relaxed)
}

pub fn task_seq() -> u32 {
    TASK_SEQ.load(Ordering::Relaxed)
}

thread_local! {
    static LOG: TouchLog = const { TouchLog { words: [const { Cell::new(0) }; TouchLog::words()] } };
}

/// Запись касания из нативного Rust-геттера: в JS-окружении не существует
/// ни одной видимой функции трассировки — `__silo_touch` в window нет.
#[inline(always)]
pub fn touch_log_record(key: u32) {
    LOG.with(|l| l.record(key));
}

/// Сброс перед новой задачей (вызывается на нити воркера в начале run()).
/// Каждый сброс = новая задача: seq инкрементится, дамп получает контекст.
pub fn touch_log_reset() -> u32 {
    let seq = next_task_seq();
    LOG.with(|l| l.clear());
    seq
}

/// Итог в ExecOutcome: сколько уникальных API тронул скрипт.
pub fn touch_log_count() -> u64 {
    LOG.with(|l| l.count() as u64)
}

/// Холодный дамп: имена затронутых API. Строки материализуются только здесь.
pub fn touch_log_dump() -> TouchDump {
    let seq = task_seq();
    let keys: Vec<u32> = LOG.with(|l| l.touched_keys().collect());
    TouchDump { seq, keys }
}

#[derive(Clone)]
pub struct TouchDump {
    pub seq: u32,
    pub keys: Vec<u32>,
}

impl fmt::Display for TouchDump {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.keys.is_empty() {
            return write!(f, "task#{}: none", self.seq);
        }
        write!(f, "task#{}: {} api touched:", self.seq, self.keys.len())?;
        let mut first = true;
        for k in &self.keys {
            if *k < ApiKey::TOTAL {
                if !first {
                    f.write_str(", ")?;
                }
                f.write_str(KEY_NAMES[*k as usize])?;
                first = false;
            }
        }
        Ok(())
    }
}

