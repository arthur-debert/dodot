//! Plist binary↔XML conversion for git clean/smudge filters.
//!
//! Two operations:
//!
//! - [`clean`] reads any plist (binary or XML) and emits canonical XML —
//!   dictionary keys sorted recursively, deterministic formatting. This
//!   is what git stores in the index.
//! - [`smudge`] reads XML and emits binary. This is what the working
//!   tree holds and what apps read at `~/Library/Preferences/...`.
//!
//! See `docs/proposals/plists.lex` for the architectural rationale.

use std::io::Cursor;

use plist::{Value, XmlWriteOptions};

use crate::{DodotError, Result};

/// Clean filter: parse any plist representation, canonicalise key order,
/// emit XML.
///
/// Determinism is the contract: the same logical plist must produce
/// byte-identical XML across runs, regardless of whether the source was
/// binary or XML and regardless of the encoder's internal key order.
pub fn clean(input: &[u8]) -> Result<Vec<u8>> {
    let mut value = Value::from_reader(Cursor::new(input)).map_err(plist_err)?;
    sort_keys_recursive(&mut value);

    let mut out = Vec::with_capacity(input.len());
    value
        .to_writer_xml_with_options(&mut out, &XmlWriteOptions::default())
        .map_err(plist_err)?;

    // The crate emits trailing CRLFs in places; we want LF-only canonical
    // output. quick-xml's writer already uses LF, but we make the contract
    // explicit by ending the file with a single newline.
    if !out.ends_with(b"\n") {
        out.push(b'\n');
    }
    Ok(out)
}

/// Smudge filter: parse XML, emit binary.
///
/// Accepts XML input (the index form). Output is the binary plist that
/// macOS apps read.
pub fn smudge(input: &[u8]) -> Result<Vec<u8>> {
    let value = Value::from_reader_xml(Cursor::new(input)).map_err(plist_err)?;
    let mut out = Vec::new();
    value.to_writer_binary(&mut out).map_err(plist_err)?;
    Ok(out)
}

/// Recursively sort dictionary keys at every level of a plist value tree.
///
/// Arrays are walked into (their elements may contain dicts) but their
/// own ordering is preserved — array order is semantically meaningful in
/// plists (e.g. `LSHandlers`, recent-files lists, ordered toolbar items).
fn sort_keys_recursive(value: &mut Value) {
    match value {
        Value::Dictionary(dict) => {
            dict.sort_keys();
            for (_, v) in dict.iter_mut() {
                sort_keys_recursive(v);
            }
        }
        Value::Array(arr) => {
            for v in arr.iter_mut() {
                sort_keys_recursive(v);
            }
        }
        _ => {}
    }
}

fn plist_err(e: plist::Error) -> DodotError {
    DodotError::Other(format!("plist conversion failed: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal XML plist with two keys in non-alphabetical order.
    const UNSORTED_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>zebra</key>
	<string>last</string>
	<key>apple</key>
	<string>first</string>
</dict>
</plist>"#;

    #[test]
    fn clean_sorts_top_level_keys() {
        let xml = clean(UNSORTED_XML.as_bytes()).expect("clean");
        let xml_str = std::str::from_utf8(&xml).expect("utf8");
        let apple_pos = xml_str.find("apple").expect("apple key present");
        let zebra_pos = xml_str.find("zebra").expect("zebra key present");
        assert!(
            apple_pos < zebra_pos,
            "expected `apple` to appear before `zebra` after canonicalisation, got:\n{xml_str}"
        );
    }

    #[test]
    fn clean_sorts_nested_dict_keys() {
        let nested = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>outer</key>
	<dict>
		<key>z_inner</key>
		<string>last</string>
		<key>a_inner</key>
		<string>first</string>
	</dict>
</dict>
</plist>"#;
        let xml = clean(nested.as_bytes()).expect("clean");
        let xml_str = std::str::from_utf8(&xml).expect("utf8");
        let a_pos = xml_str.find("a_inner").expect("a_inner present");
        let z_pos = xml_str.find("z_inner").expect("z_inner present");
        assert!(
            a_pos < z_pos,
            "nested dict keys should be sorted, got:\n{xml_str}"
        );
    }

    #[test]
    fn clean_preserves_array_order() {
        let arr_xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<array>
	<string>third</string>
	<string>first</string>
	<string>second</string>
</array>
</plist>"#;
        let xml = clean(arr_xml.as_bytes()).expect("clean");
        let xml_str = std::str::from_utf8(&xml).expect("utf8");
        let third = xml_str.find("third").expect("third");
        let first = xml_str.find("first").expect("first");
        let second = xml_str.find("second").expect("second");
        assert!(
            third < first && first < second,
            "array order must be preserved, got:\n{xml_str}"
        );
    }

    #[test]
    fn smudge_then_clean_roundtrips_to_canonical_xml() {
        let canonical = clean(UNSORTED_XML.as_bytes()).expect("first clean");
        let binary = smudge(&canonical).expect("smudge");
        let back = clean(&binary).expect("second clean");
        assert_eq!(
            canonical, back,
            "binary→clean must reproduce the canonical XML"
        );
    }

    /// The contract test from §4.3 of the proposal: binary→clean→smudge→clean
    /// must produce identical XML across runs.
    #[test]
    fn determinism_property_test() {
        // Start from a plist with deliberately unstable encoder ordering.
        let canonical = clean(UNSORTED_XML.as_bytes()).expect("clean 1");

        // Round-trip several times. If anything is non-deterministic
        // (HashMap iteration, etc.), divergence shows up here.
        let mut current = canonical.clone();
        for i in 0..5 {
            let binary = smudge(&current).unwrap_or_else(|e| panic!("smudge iter {i}: {e}"));
            let xml = clean(&binary).unwrap_or_else(|e| panic!("clean iter {i}: {e}"));
            assert_eq!(
                canonical, xml,
                "round-trip {i} diverged from canonical form"
            );
            current = xml;
        }
    }

    #[test]
    fn clean_accepts_binary_input() {
        // Build canonical XML, convert to binary, ensure clean accepts it.
        let canonical = clean(UNSORTED_XML.as_bytes()).expect("clean");
        let binary = smudge(&canonical).expect("smudge");
        let from_binary = clean(&binary).expect("clean from binary");
        assert_eq!(canonical, from_binary);
    }

    #[test]
    fn clean_handles_mixed_value_types() {
        let mixed = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>string_key</key>
	<string>hello</string>
	<key>int_key</key>
	<integer>42</integer>
	<key>bool_key</key>
	<true/>
	<key>real_key</key>
	<real>3.14</real>
	<key>arr_key</key>
	<array>
		<string>a</string>
		<dict>
			<key>z</key>
			<string>z</string>
			<key>a</key>
			<string>a</string>
		</dict>
	</array>
</dict>
</plist>"#;
        let xml = clean(mixed.as_bytes()).expect("clean");
        let xml_str = std::str::from_utf8(&xml).expect("utf8");

        // Top-level keys sorted: arr_key, bool_key, int_key, real_key, string_key.
        let positions = ["arr_key", "bool_key", "int_key", "real_key", "string_key"]
            .iter()
            .map(|k| xml_str.find(k).unwrap_or_else(|| panic!("missing {k}")))
            .collect::<Vec<_>>();
        assert!(
            positions.windows(2).all(|w| w[0] < w[1]),
            "top-level keys not sorted, got:\n{xml_str}"
        );

        // Nested dict in array: a before z.
        let a_inner = xml_str.rfind(">a<").expect("inner a");
        let z_inner = xml_str.rfind(">z<").expect("inner z");
        assert!(
            a_inner < z_inner,
            "nested dict keys not sorted in array element"
        );
    }

    #[test]
    fn clean_emits_trailing_newline() {
        let xml = clean(UNSORTED_XML.as_bytes()).expect("clean");
        assert_eq!(xml.last().copied(), Some(b'\n'), "must end with LF");
    }

    #[test]
    fn smudge_rejects_garbage_input() {
        let bad = b"not actually XML at all";
        assert!(smudge(bad).is_err(), "smudge should reject non-XML input");
    }

    #[test]
    fn clean_rejects_garbage_input() {
        let bad = b"\x00\x01\x02neither binary nor XML";
        assert!(clean(bad).is_err(), "clean should reject non-plist garbage");
    }
}
