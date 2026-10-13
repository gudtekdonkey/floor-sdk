//! The mint metadata URI template, so a launch can be created before the metadata host exists.
//! Two pure functions and nothing else: the validator for the base a deployer sets,
//! and the join that turns `(base, mint)` into the one string written into the mint.
//!
//! ⛔ **The template is `<base>/<mint pubkey base58>.json`, frozen.** It is MINT-ADDRESSED, so the
//! id exists before any off-chain write and depends on nothing a launcher chooses.
//! ⛔ **No Solana crate.** The mint arrives ALREADY base58-encoded — `floor_launch` passes
//! `mint.key().to_string()`, the test passes `Pubkey::to_string()`. This crate never learns base58.
//! ⚠ The length bound is a const proof below, not a comment: the longest base (64 B) joined to the
//! longest base58 pubkey (44 chars) plus `/` and `.json` is 114 B, inside `MAX_URI_LEN` (200).

use crate::constants::{MAX_METADATA_BASE_LEN, MAX_URI_LEN, METADATA_URI_SUFFIX};

/// The longest a base58-encoded 32-byte pubkey can be.
pub const MAX_BASE58_PUBKEY_LEN: usize = 44;

/// The ONLY schemes a metadata base may carry.
///
/// ⛔ **An ALLOWLIST, never a `://` substring test.** A substring test accepts EVERY scheme, and the
/// sharpest member of that set is `javascript://evil.example` — 24 printable ASCII bytes, no `#`, no
/// `?`, no `\`, no userinfo, so every other rule in [`normalize_metadata_base`] passes it — which
/// the template renders as `javascript://evil.example/<mint>.json`, a standard XSS payload in any
/// consumer that puts the string in an `href` or hands it to `window.open`. The whole point of this
/// design is that the string is read by wallets and explorers FLOOR does not write, and it is baked
/// into every mint created or filled in that window, immutably. `file:///etc/passwd` and any `x://`
/// are refused the same way. Refusing `#` `?` `\` and userinfo but not the SCHEME would leave
/// the sharpest member of the class the function exists to catch.
/// ⛔ **`http://` is REFUSED**, and that is a decision, not an omission: the base is a permanent,
/// un-migratable pointer, so a plaintext one is MITM-able for the life of every token under it and
/// mixed-content-blocked in every browser. A base is chosen once; it may as well be chosen right.
/// ⚠ **The scheme is matched case-SENSITIVELY, so `HTTPS://h` is refused, never rewritten.** This
/// function returns a slice BORROWED from its input and the caller stores those bytes as they were
/// sent; lower-casing a prefix would mean allocating, and a base silently rewritten is a base nobody
/// read. A deployer who types the scheme in capitals is told so and types it again.
/// ⚠ `ipfs://` and `ar://` are here because a base is just a string: `ipfs://<dir cid>` is a
/// content-addressed directory that keeps working without any particular host.
pub const METADATA_BASE_SCHEMES: [&str; 3] = ["https://", "ipfs://", "ar://"];

/// The template can never outgrow `MAX_URI_LEN`, whatever the deployer sets.
const _: () = assert!(MAX_METADATA_BASE_LEN + 1 + MAX_BASE58_PUBKEY_LEN + METADATA_URI_SUFFIX.len() <= MAX_URI_LEN);

/// Normalise a metadata base for storage, or refuse it.
///
/// - `""` is "no host yet" and is always accepted — it is what makes a launch possible before the
///   host exists (the one-shot fill closes it later).
/// - EVERY trailing `/` is stripped, so the join below adds exactly one and `https://h/m` can never
///   come out as `https://h//m`. A base that is nothing but slashes is REFUSED, never silently
///   turned into "no host yet".
/// - ≤ [`MAX_METADATA_BASE_LEN`] bytes after stripping; every byte ASCII `0x21..=0x7E`, so no NUL,
///   no control byte, no space and no non-ASCII — a URL carrying any of those is a link nobody can
///   follow, written into a mint for ever.
/// - ABSOLUTE, and its scheme is one of [`METADATA_BASE_SCHEMES`], with at least one byte after it.
///   A relative base yields a relative uri, which resolves against nothing in a wallet; a base on
///   any other scheme is the `javascript://` class that const refuses. ⚠ This is the ONE
///   check a "path-only, resolved by the app" base would have to relax.
/// - ⛔ **A base is a PATH PREFIX, so `#`, `?` and `\` are refused outright, and `@` is refused in
///   the authority**. The ASCII-range test above is a CHARACTER CLASS, not a URL
///   parse, and it let through exactly the class of typo this function exists to catch: `…/x#`
///   builds `…/x#/<mint>.json`, whose fragment means the JSON is never fetched; `…/x?v=1` builds
///   `…/x?v=1/<mint>.json`, a query with the id inside it; `https://evil@h` is valid userinfo, so
///   the host is `h` and not the one that was read. Each would be baked into every mint made or
///   filled in that window, for ever. `%` is NOT refused — it is how a legal path spells a byte.
pub fn normalize_metadata_base(raw: &str) -> Option<&str> {
    if raw.is_empty() {
        return Some("");
    }
    let mut end = raw.len();
    while end > 0 && raw.as_bytes()[end - 1] == b'/' {
        end -= 1;
    }
    let base = &raw[..end];
    if base.is_empty() || base.len() > MAX_METADATA_BASE_LEN {
        return None;
    }
    if !base.as_bytes().iter().all(|c| (0x21..=0x7E).contains(c)) {
        return None;
    }
    if base.as_bytes().iter().any(|c| matches!(c, b'#' | b'?' | b'\\')) {
        return None;
    }
    // ⛔ The scheme is an ALLOWLIST, case-sensitive, and there must be a byte after it. See
    // `METADATA_BASE_SCHEMES`: a `://` substring test admitted `javascript://evil.example`.
    let scheme = METADATA_BASE_SCHEMES.iter().find(|s| base.starts_with(**s))?;
    if base.len() == scheme.len() {
        return None;
    }
    // Userinfo: everything from the scheme to the next `/` is the authority, and a `@` in it moves
    // the host somewhere the reader did not read.
    let after = &base[scheme.len()..];
    let authority = &after[..after.find('/').unwrap_or(after.len())];
    if authority.contains('@') {
        return None;
    }
    Some(base)
}

/// `<base>/<mint base58>.json`, or `""` when there is no host yet.
///
/// ⛔ `base` must already be the NORMALISED form (no trailing `/`); this function does not
/// re-validate it, because on-chain it reads a base `set_metadata_base` already normalised.
pub fn metadata_uri(base: &str, mint_base58: &str) -> String {
    if base.is_empty() {
        return String::new();
    }
    let mut s = String::with_capacity(base.len() + 1 + mint_base58.len() + METADATA_URI_SUFFIX.len());
    s.push_str(base);
    s.push('/');
    s.push_str(mint_base58);
    s.push_str(METADATA_URI_SUFFIX);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINT: &str = "8JUjWjAyXTMB4ZXcV7nk3p6aiaPtGGbJcUFTPKxGC2Wx";

    #[test]
    fn the_template_is_base_slash_mint_dot_json() {
        assert_eq!(
            metadata_uri("https://floor.supply/meta/solana", MINT),
            "https://floor.supply/meta/solana/8JUjWjAyXTMB4ZXcV7nk3p6aiaPtGGbJcUFTPKxGC2Wx.json"
        );
        assert_eq!(metadata_uri("", MINT), "", "no host yet is the empty uri, never a dead link");
    }

    #[test]
    fn a_trailing_slash_is_stripped_so_the_join_never_doubles_it() {
        for raw in ["https://h.example/m/", "https://h.example/m//", "https://h.example/m"] {
            assert_eq!(normalize_metadata_base(raw), Some("https://h.example/m"), "{raw}");
        }
        assert_eq!(metadata_uri(normalize_metadata_base("https://h.example/m/").unwrap(), MINT), format!("https://h.example/m/{MINT}.json"));
    }

    #[test]
    fn empty_is_no_host_yet_but_slashes_alone_are_refused() {
        assert_eq!(normalize_metadata_base(""), Some(""));
        assert_eq!(normalize_metadata_base("/"), None);
        assert_eq!(normalize_metadata_base("///"), None);
    }

    #[test]
    fn a_base_that_is_not_an_absolute_ascii_url_is_refused() {
        assert_eq!(normalize_metadata_base("floor.supply/meta"), None, "no scheme");
        assert_eq!(normalize_metadata_base("://h"), None, "no scheme name");
        assert_eq!(normalize_metadata_base("https://"), None, "scheme only");
        assert_eq!(normalize_metadata_base("https://h example"), None, "a space");
        assert_eq!(normalize_metadata_base("https://h\u{0}x"), None, "an interior NUL");
        assert_eq!(normalize_metadata_base("https://hé.example"), None, "not ASCII");
        assert_eq!(normalize_metadata_base("https://h\nx"), None, "a control byte");
        // ⛔ The class an ASCII-range test alone would let through. Each one is a link that
        // would be written into every mint of that window and never move again.
        assert_eq!(normalize_metadata_base("https://h/x#"), None, "a fragment: the JSON is never fetched");
        assert_eq!(normalize_metadata_base("https://h/x#frag"), None, "a fragment with a body");
        assert_eq!(normalize_metadata_base("https://h/x?v=1"), None, "a query");
        assert_eq!(normalize_metadata_base("https://h\\x"), None, "a backslash");
        assert_eq!(normalize_metadata_base("https://evil@h"), None, "userinfo moves the host");
        assert_eq!(normalize_metadata_base("https://evil@h/x"), None, "userinfo with a path");
        // ⛔ The SCHEME, which a `://` substring test does not read at all.
        // `javascript://evil.example` passes every other rule here — printable ASCII, 24 B, no
        // `#`/`?`/`\`, no userinfo — and renders as `javascript://evil.example/<mint>.json`.
        assert_eq!(normalize_metadata_base("javascript://evil.example"), None, "an XSS payload as a base");
        assert_eq!(normalize_metadata_base("javascript://x/a"), None, "the same with a path");
        assert_eq!(normalize_metadata_base("file:///etc/passwd"), None, "a local file");
        assert_eq!(normalize_metadata_base("data://x"), None, "any other scheme");
        assert_eq!(normalize_metadata_base("http://h"), None, "plaintext: a permanent pointer is not MITM-able");
        assert_eq!(normalize_metadata_base("HTTPS://h"), None, "the scheme is case-sensitive, never rewritten");
        assert_eq!(normalize_metadata_base("ipfs://"), None, "a scheme with nothing after it");
        assert_eq!(normalize_metadata_base("ar://"), None, "a scheme with nothing after it");
        // …and the two content-addressed schemes that ARE allowed, plus what is still legal in a
        // path: a `%` is how a path spells a byte, and an `@` past the authority is ordinary.
        assert_eq!(normalize_metadata_base("ipfs://bafyfakecid"), Some("ipfs://bafyfakecid"), "ipfs");
        assert_eq!(normalize_metadata_base("ar://txid/"), Some("ar://txid"), "arweave, trailing slash stripped");
        assert_eq!(normalize_metadata_base("https://h/a%20b"), Some("https://h/a%20b"), "percent-encoding");
        assert_eq!(normalize_metadata_base("https://h/a@b"), Some("https://h/a@b"), "`@` in the path");
        assert!(normalize_metadata_base(&format!("https://h.example/{}", "a".repeat(MAX_METADATA_BASE_LEN))).is_none());
        let longest = format!("https://h/{}", "a".repeat(MAX_METADATA_BASE_LEN - 10));
        assert_eq!(longest.len(), MAX_METADATA_BASE_LEN);
        assert_eq!(normalize_metadata_base(&longest), Some(longest.as_str()));
    }

    #[test]
    fn the_longest_template_fits_the_uri_bound() {
        let base = format!("https://h/{}", "a".repeat(MAX_METADATA_BASE_LEN - 10));
        let uri = metadata_uri(&base, &"1".repeat(MAX_BASE58_PUBKEY_LEN));
        assert_eq!(uri.len(), MAX_METADATA_BASE_LEN + 1 + MAX_BASE58_PUBKEY_LEN + 5);
        assert!(uri.len() <= MAX_URI_LEN, "{} <= {MAX_URI_LEN}", uri.len());
    }
}
