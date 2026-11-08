//! Checks the version resource of the built executable: its original file name is the name the
//! release zip for this processor gives it, so Properties > Details matches the file a user unpacks.

/// Gets the value of the version-resource string `key` from the raw bytes of an executable.
///
/// Each entry of a `StringFileInfo` table is a `String` structure: three `u16` fields, the key in
/// UTF-16LE with a NUL, padding to a 32-bit boundary, then the value with a NUL. Returns `None` if
/// the key is not there.
fn version_string(image: &[u8], key: &str) -> Option<String> {
    let mut needle: Vec<u16> = key.encode_utf16().collect();
    needle.push(0);
    // The search decodes the bytes as UTF-16 from an even and from an odd offset, so it does not
    // depend on where the resource section sits in the file.
    for bytes in [image, image.get(1..)?] {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_le_bytes(pair))
            .collect();
        if let Some(at) = units
            .windows(needle.len())
            .position(|window| window == needle)
        {
            let value: Vec<u16> = units[at + needle.len()..]
                .iter()
                .copied()
                .skip_while(|&unit| unit == 0)
                .take_while(|&unit| unit != 0)
                .collect();
            return String::from_utf16(&value).ok();
        }
    }
    None
}

#[test]
fn original_filename_is_the_release_zip_name_for_this_processor() {
    let image = std::fs::read(env!("CARGO_BIN_EXE_overlay-lock")).expect("the executable reads");
    let cpu = if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        unreachable!("OverlayLock ships only x64, x86 and arm64")
    };
    assert_eq!(
        version_string(&image, "OriginalFilename").as_deref(),
        Some(format!("overlay-lock-{cpu}.exe").as_str())
    );
}

#[test]
fn version_string_reads_a_padded_entry() {
    let mut image = vec![0u8; 3];
    for unit in "OriginalFilename\0\0name.exe\0".encode_utf16() {
        image.extend_from_slice(&unit.to_le_bytes());
    }
    assert_eq!(
        version_string(&image, "OriginalFilename").as_deref(),
        Some("name.exe")
    );
    assert_eq!(version_string(&image, "Missing"), None);
}
