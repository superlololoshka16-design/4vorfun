use compact_str::CompactString;
use compact_str::ToCompactString as _;
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProfileError {
    #[error("ua/{field}: {detail}")]
    Ua {
        field: &'static str,
        detail: CompactString,
    },
    #[error("platform/{field}: {detail}")]
    Platform {
        field: &'static str,
        detail: CompactString,
    },
    #[error("screen/{field}: {detail}")]
    Screen {
        field: &'static str,
        detail: CompactString,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Platform {
    Windows,
    MacOS,
    Linux,
    Android,
    IOS,
}

impl Platform {
    pub const fn as_str(self) -> &'static str {
        match self {
            Platform::Windows => "Windows",
            Platform::MacOS => "macOS",
            Platform::Linux => "Linux",
            Platform::Android => "Android",
            Platform::IOS => "iOS",
        }
    }

    pub const fn is_mobile(self) -> bool {
        matches!(self, Platform::Android | Platform::IOS)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NetKind {
    Datacenter,
    Residential,
    Mobile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Family {
    Chrome { major: u16 },
    Edge { major: u16 },
    Firefox { major: u16 },
    Safari { mobile: bool },
}

#[repr(align(64))]
#[derive(Debug, Clone)]
pub struct Profile {
    pub ua: Arc<str>,
    pub sec_ch_ua: Arc<str>,
    pub accept_language: Arc<str>,
    pub platform: Platform,
    pub locale: CompactString,
    pub tz: CompactString,
    pub screen_w: u32,
    pub screen_h: u32,
    pub canvas_seed: u64,
    pub webgl_vendor: CompactString,
    pub webgl_renderer: CompactString,
    pub asn: u32,
    pub net: NetKind,
    pub family: Family,
}

fn ua_has(ua: &str, token: &str) -> bool {
    memchr::memmem::find(ua.as_bytes(), token.as_bytes()).is_some()
}

fn ua_has_b(ua: &[u8], token: &[u8]) -> bool {
    memchr::memmem::find(ua, token).is_some()
}

fn ua_major_after(ua: &str, marker: &str) -> Option<u16> {
    let bytes = ua.as_bytes();
    let pos = memchr::memmem::find(bytes, marker.as_bytes())? + marker.len();
    let mut end = pos;
    while end < bytes.len() && bytes[end].is_ascii_digit() {
        end += 1;
    }
    if end == pos {
        return None;
    }
    let mut v: u32 = 0;
    for b in &bytes[pos..end] {
        v = v * 10 + u32::from(b - b'0');
        if v > 999 {
            return None;
        }
    }
    u16::try_from(v).ok()
}

impl Profile {
    pub fn validate(&self) -> Result<(), ProfileError> {
        let ua = self.ua.as_ref();
        let sec = self.sec_ch_ua.as_ref();
        let plat_token = match self.platform {
            Platform::Windows => "Windows NT",
            Platform::MacOS => "Macintosh",
            Platform::Linux => "X11",
            Platform::Android => "Android",
            Platform::IOS => "iPhone",
        };
        if !ua_has(ua, plat_token) {
            return Err(ProfileError::Ua {
                field: "platform-token",
                detail: CompactString::new(plat_token),
            });
        }
        if ua_has(ua, "HeadlessChrome") || ua_has(ua, "PhantomJS") || ua_has(ua, "python-requests")
        {
            return Err(ProfileError::Ua {
                field: "automation-mark",
                detail: CompactString::new("ua leaks automation"),
            });
        }
        let (name, major, uam) = match self.family {
            Family::Chrome { major } => ("Chrome", major, ua_major_after(ua, "Chrome/")),
            Family::Edge { major } => ("Edg", major, ua_major_after(ua, "Edg/")),
            Family::Firefox { major } => ("Firefox", major, ua_major_after(ua, "Firefox/")),
            Family::Safari { mobile } => {
                let ok = ua_has(ua, "Safari/") && (mobile == ua_has(ua, "iPhone"));
                if !ok {
                    return Err(ProfileError::Ua {
                        field: "safari-mobile",
                        detail: CompactString::new("family/mobile mismatch"),
                    });
                }
                ("Version", 0, ua_major_after(ua, "Version/"))
            }
        };
        let safari = matches!(self.family, Family::Safari { .. });
        if !safari {
            if let Some(expect) = uam
                && expect != major
            {
                return Err(ProfileError::Ua {
                    field: "ua-major",
                    detail: format_args!("{name}: {expect} != {major}").to_compact_string(),
                });
            }
            let mut needle = CompactString::const_new(";v=\"");
            needle.push_str(&major.to_string());
            needle.push('"');
            if !ua_has_b(sec.as_bytes(), needle.as_bytes()) && name != "Firefox" {
                let mut d = CompactString::new("");
                use core::fmt::Write;
                let _ = write!(d, "{name} {major} missing in sec-ch-ua");
                return Err(ProfileError::Ua {
                    field: "sec-ch-ua-major",
                    detail: d,
                });
            }
        }
        if self.platform.is_mobile() != ua_has(ua, "Mobi") && self.platform != Platform::IOS {
            return Err(ProfileError::Ua {
                field: "mobile-flag",
                detail: CompactString::new("platform mobile != ua Mobi"),
            });
        }
        if self.screen_w == 0 || self.screen_h == 0 {
            return Err(ProfileError::Screen {
                field: "dims",
                detail: CompactString::new("zero screen"),
            });
        }
        if !self.locale.contains('-') || self.locale.len() < 4 {
            return Err(ProfileError::Platform {
                field: "locale",
                detail: self.locale.clone(),
            });
        }
        Ok(())
    }
}
