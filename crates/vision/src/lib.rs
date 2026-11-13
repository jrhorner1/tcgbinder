//! Reading a photo of a card with Apple's Vision framework: find the card's outline,
//! straighten it, and read its text (`native/read.m`). Other platforms have no reader.

use anyhow::Result;
use tcgbinder_core::scan::Reading;

/// Whether [`read_image`] works on this platform.
pub fn available() -> bool {
    cfg!(any(target_os = "macos", target_os = "ios"))
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
mod apple {
    use std::ffi::{CStr, c_char, c_int, c_void};

    use anyhow::{Result, bail};
    use tcgbinder_core::scan::Reading;

    unsafe extern "C" {
        fn tcgb_read_image(data: *const c_void, length: c_int, err: *mut *mut c_char) -> *mut c_char;
        fn free(p: *mut c_void);
    }

    /// Takes a malloc'd C string from the reader.
    unsafe fn take(p: *mut c_char) -> String {
        // SAFETY: the reader returns NUL-terminated strings from malloc, owned by us.
        let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
        unsafe { free(p.cast()) };
        s
    }

    pub fn read_image(data: &[u8]) -> Result<Reading> {
        if data.is_empty() {
            bail!("empty image");
        }
        let Ok(len) = c_int::try_from(data.len()) else { bail!("the image is too large") };
        let mut err: *mut c_char = std::ptr::null_mut();
        // SAFETY: data is valid for len bytes; the reader copies it before returning.
        let out = unsafe { tcgb_read_image(data.as_ptr().cast(), len, &mut err) };
        if out.is_null() {
            let msg = if err.is_null() { "the photo couldn't be read".to_string() } else { unsafe { take(err) } };
            bail!(msg);
        }
        let json = unsafe { take(out) };
        Ok(serde_json::from_str(&json)?)
    }
}

/// Reads a photo of a card (JPEG, PNG, HEIC, ...): finds the card, straightens it and
/// reads its text. It takes a second or so; don't call it on a UI thread.
pub fn read_image(data: &[u8]) -> Result<Reading> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    return apple::read_image(data);
    #[cfg(not(any(target_os = "macos", target_os = "ios")))]
    {
        let _ = data;
        anyhow::bail!("card scanning needs Apple's Vision framework (macOS or iOS)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_images() {
        assert!(read_image(b"").is_err());
        if available() {
            assert!(read_image(b"not an image").is_err());
        }
    }

    /// Reads every photo in MTG_SCAN_DIR with Vision and identifies it against the live
    /// Scryfall API. A file named "<set>-<number>-*.jpg" must come out as that printing
    /// (or near the top of the choices). Skipped unless MTG_SCAN_DIR is set; with
    /// MTG_SCAN_TEXT set, the text read is printed too.
    #[test]
    fn live_read_and_identify() {
        use tcgbinder_core::game::Game;
        let Ok(dir) = std::env::var("MTG_SCAN_DIR") else { return };
        let g = tcgbinder_mtg::Magic::default();
        let scanner = g.scanner().unwrap();
        let mut failures = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| !e.eq_ignore_ascii_case("jpg")) {
                continue;
            }
            let base = path.file_name().unwrap().to_string_lossy().into_owned();
            let parts: Vec<&str> = base.splitn(3, '-').collect();
            let start = std::time::Instant::now();
            let r = match read_image(&std::fs::read(&path).unwrap()) {
                Ok(r) => r,
                Err(e) => {
                    failures.push(format!("{base}: {e}"));
                    continue;
                }
            };
            let took = start.elapsed();
            let res = scanner.identify(&r).unwrap();
            let key = |c: &tcgbinder_core::Card| format!("{}-{}", c.set.to_lowercase(), c.collector_number);
            let got = res.candidates.first().map_or("no match".to_string(), |c| format!("{} {}", key(c), c.name));
            if std::env::var_os("MTG_SCAN_TEXT").is_some() {
                eprintln!("{base} text:\n{}", r.text());
            }
            eprintln!(
                "{base:<22} card={:<5} read {:<28?} in {:>4}ms → {got} (exact={}, {} candidates) {}",
                r.card_found,
                res.read,
                took.as_millis(),
                res.exact,
                res.candidates.len(),
                res.note
            );
            if parts.len() == 3 {
                let want = format!("{}-{}", parts[0], parts[1]);
                let near_top = res.candidates.iter().take(if res.exact { 1 } else { 5 }).any(|c| key(c) == want);
                if !near_top {
                    failures.push(format!("{base}: identified as {got}; text read:\n{}", r.text()));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }
}
