//! Checks the resources of the built executable: the version resource that Properties > Details
//! shows, and the manifest's assembly identity. Each value is read from the raw bytes of the file.

/// Gets the raw bytes of the built executable.
fn image() -> Vec<u8> {
    std::fs::read(env!("CARGO_BIN_EXE_overlay-lock")).expect("the executable reads")
}

/// Gets the short CPU name of the target the tests were built for.
fn cpu() -> &'static str {
    if cfg!(target_arch = "x86_64") {
        "x64"
    } else if cfg!(target_arch = "x86") {
        "x86"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        unreachable!("OverlayLock ships only x64, x86 and arm64")
    }
}

/// Decodes `image` as UTF-16LE from an even and from an odd offset, so a search does not depend on
/// where the resource section sits in the file.
fn utf16_views(image: &[u8]) -> impl Iterator<Item = Vec<u16>> + '_ {
    [Some(image), image.get(1..)]
        .into_iter()
        .flatten()
        .map(|bytes| {
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| u16::from_le_bytes(pair))
                .collect()
        })
}

/// Gets the value of the version-resource string `key` from the raw bytes of an executable.
///
/// Each entry of a `StringFileInfo` table is a `String` structure: three `u16` fields, the key in
/// UTF-16LE with a NUL, padding to a 32-bit boundary, then the value with a NUL. Returns `None` if
/// the key is not there.
fn version_string(image: &[u8], key: &str) -> Option<String> {
    let mut needle: Vec<u16> = key.encode_utf16().collect();
    needle.push(0);
    utf16_views(image).find_map(|units| {
        let at = units
            .windows(needle.len())
            .position(|window| window == needle)?;
        let value: Vec<u16> = units[at + needle.len()..]
            .iter()
            .copied()
            .skip_while(|&unit| unit == 0)
            .take_while(|&unit| unit != 0)
            .collect();
        String::from_utf16(&value).ok()
    })
}

/// Gets the `VS_FIXEDFILEINFO` fields after its signature: the structure version, the file version
/// (high and low), the product version (high and low), the flags mask and the flags.
fn fixed_file_info(image: &[u8]) -> [u32; 7] {
    let signature = 0xFEEF_04BD_u32.to_le_bytes();
    let at = image
        .windows(4)
        .position(|window| window == signature)
        .expect("the executable has a VS_FIXEDFILEINFO");
    let mut fields = [0; 7];
    for (index, field) in fields.iter_mut().enumerate() {
        let start = at + 4 + index * 4;
        *field = u32::from_le_bytes(
            image[start..start + 4]
                .try_into()
                .expect("four bytes make a u32"),
        );
    }
    fields
}

/// Gets a `Cargo.toml` version component as a number.
fn version_part(text: &str) -> u32 {
    text.parse().expect("a version component is a number")
}

#[test]
fn original_filename_is_the_release_zip_name_for_this_processor() {
    assert_eq!(
        version_string(&image(), "OriginalFilename").as_deref(),
        Some(format!("overlay-lock_{}.exe", cpu()).as_str())
    );
}

#[test]
fn details_name_the_product_company_version_and_copyright() {
    let image = image();
    let get = |key: &str| version_string(&image, key).unwrap_or_else(|| panic!("{key} is set"));
    assert_eq!(get("CompanyName"), "Joe Cox");
    assert_eq!(get("ProductName"), "OverlayLock");
    assert_eq!(get("FileDescription"), "OverlayLock");
    assert_eq!(get("InternalName"), "overlay-lock");
    assert_eq!(get("FileVersion"), env!("CARGO_PKG_VERSION"));
    assert_eq!(get("ProductVersion"), env!("CARGO_PKG_VERSION"));
    let copyright = get("LegalCopyright");
    assert!(
        copyright.starts_with("Copyright \u{a9} 2026") && copyright.ends_with(" Joe Cox"),
        "{copyright:?}"
    );
    let comments = get("Comments");
    for part in [
        env!("CARGO_PKG_DESCRIPTION"),
        &format!("Built for {} (", cpu()),
        "MIT licence",
        "https://github.com/BootBlock/OverlayLock",
    ] {
        assert!(comments.contains(part), "{comments:?} lacks {part:?}");
    }
}

/// The strings are in English (United Kingdom), Unicode: the table key `080904b0`.
#[test]
fn details_are_in_british_english() {
    let key: Vec<u16> = "080904b0\0".encode_utf16().collect();
    assert!(
        utf16_views(&image()).any(|units| units.windows(key.len()).any(|window| window == key)),
        "the string table is not 080904b0"
    );
}

#[test]
fn fixed_file_info_has_the_package_version_and_flags() {
    let [
        _,
        file_high,
        file_low,
        product_high,
        product_low,
        mask,
        flags,
    ] = fixed_file_info(&image());
    let high = version_part(env!("CARGO_PKG_VERSION_MAJOR")) << 16
        | version_part(env!("CARGO_PKG_VERSION_MINOR"));
    let low = version_part(env!("CARGO_PKG_VERSION_PATCH")) << 16;
    assert_eq!((file_high, file_low), (high, low));
    assert_eq!((product_high, product_low), (high, low));
    assert_eq!(mask, 0x3F);
    let debug = u32::from(cfg!(debug_assertions));
    let prerelease = u32::from(!env!("CARGO_PKG_VERSION_PRE").is_empty()) << 1;
    assert_eq!(
        flags,
        debug | prerelease,
        "VS_FF_DEBUG and VS_FF_PRERELEASE"
    );
}

#[test]
fn manifest_identity_has_the_package_version_and_architecture() {
    let architecture = match cpu() {
        "x64" => "amd64",
        other => other,
    };
    let identity = format!(
        r#"name="BootBlock.OverlayLock" version="{}.{}.{}.0" processorArchitecture="{architecture}""#,
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
    );
    let image = image();
    assert!(
        image
            .windows(identity.len())
            .any(|window| window == identity.as_bytes()),
        "the manifest lacks {identity}"
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
