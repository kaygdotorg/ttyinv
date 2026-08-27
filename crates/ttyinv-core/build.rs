use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Component, Path, PathBuf},
};
use ttf_parser::Face;

const MAX_FONT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    version: u32,
    fonts: Vec<Font>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Font {
    id: String,
    label: String,
    license: String,
    regular: Slot,
    semibold: Slot,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Slot {
    slot: String,
    file: String,
    sha256: String,
    source: String,
    weight: u16,
    family: String,
    postscript: String,
    name: String,
}

fn rust_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

fn main() {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest.join("assets/render-fonts");
    let root = fs::canonicalize(&root).expect("assets/render-fonts is required");
    let registry_path = root.join("registry.json");
    println!("cargo:rerun-if-changed={}", registry_path.display());
    let text =
        fs::read_to_string(&registry_path).expect("assets/render-fonts/registry.json is required");
    let registry: Registry =
        serde_json::from_str(&text).expect("invalid render font registry JSON");
    assert_eq!(
        registry.version, 1,
        "unsupported render font registry version"
    );
    assert!(
        !registry.fonts.is_empty(),
        "font registry must not be empty"
    );
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR"));
    let mut generated = String::from(
        "pub struct FontAsset { pub id: &'static str, pub label: &'static str, pub regular: &'static [u8], pub semibold: &'static [u8], pub regular_weight: u16, pub semibold_weight: u16 }\n",
    );
    for font in &registry.fonts {
        assert!(!font.label.is_empty(), "font {} label is required", font.id);
        assert!(
            !font.license.is_empty(),
            "font {} license is required",
            font.id
        );
        assert!(
            (300..=800).contains(&font.regular.weight),
            "font {} regular slot has invalid weight",
            font.id
        );
        assert!(
            (300..=800).contains(&font.semibold.weight)
                && font.semibold.weight > font.regular.weight,
            "font {} semibold slot must be heavier than regular and within 300..=800",
            font.id
        );
        for slot in [&font.regular, &font.semibold] {
            assert!(!slot.slot.is_empty(), "font {} slot is required", font.id);
            assert!(
                !slot.family.is_empty(),
                "font {} family is required",
                font.id
            );
            assert!(
                !slot.postscript.is_empty(),
                "font {} postscript is required",
                font.id
            );
            assert!(!slot.name.is_empty(), "font {} name is required", font.id);
        }
    }
    generated.push_str("pub const FONT_ASSETS: &[FontAsset] = &[\n");
    for font in &registry.fonts {
        let regular = copy_asset(&root, &out, &font.id, "regular", &font.regular);
        let semibold = copy_asset(&root, &out, &font.id, "semibold", &font.semibold);
        generated.push_str(&format!("FontAsset {{ id: {}, label: {}, regular: include_bytes!({}), semibold: include_bytes!({}), regular_weight: {}, semibold_weight: {} }},\n", rust_str(&font.id), rust_str(&font.label), rust_str(&regular), rust_str(&semibold), font.regular.weight, font.semibold.weight));
    }
    generated.push_str("];\n");
    fs::write(out.join("render_fonts.rs"), generated).expect("write generated font registry");
}

fn copy_asset(root: &Path, out: &Path, id: &str, slot: &str, spec: &Slot) -> String {
    let relative = Path::new(&spec.file);
    assert!(!relative.is_absolute(), "font {id} {slot} must be relative");
    assert!(
        relative.components().all(|c| !matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )),
        "font {id} {slot} escapes asset root"
    );
    let source = root.join(relative);
    let canonical = fs::canonicalize(&source)
        .unwrap_or_else(|e| panic!("font {id} {slot} unavailable at {}: {e}", source.display()));
    assert!(
        canonical.starts_with(root),
        "font {id} {slot} escapes asset root"
    );
    let bytes = fs::read(&canonical).expect("read render font");
    println!("cargo:rerun-if-changed={}", canonical.display());
    assert!(
        !bytes.is_empty() && bytes.len() <= MAX_FONT_BYTES,
        "font {id} {slot} has invalid size"
    );
    let mut hash = Sha256::new();
    hash.update(&bytes);
    let actual = format!("{:x}", hash.finalize());
    assert_eq!(
        actual,
        spec.sha256.to_ascii_lowercase(),
        "font {id} {slot} sha256 mismatch"
    );
    let face = Face::parse(&bytes, 0)
        .unwrap_or_else(|_| panic!("font {id} {slot} is not a valid sfnt face"));
    assert!(
        face.units_per_em() > 0,
        "font {id} {slot} has invalid units-per-em"
    );
    for c in 0x20u8..=0x7eu8 {
        let glyph = face
            .glyph_index(c as char)
            .unwrap_or_else(|| panic!("font {id} {slot} lacks printable ASCII"));
        assert!(
            face.glyph_hor_advance(glyph).is_some(),
            "font {id} {slot} lacks glyph advance"
        );
    }
    assert!(
        (300..=800).contains(&spec.weight),
        "font {id} {slot} has invalid weight"
    );
    assert!(
        !spec.source.is_empty(),
        "font {id} {slot} source is required"
    );
    let dest = out.join(format!("font-{id}-{slot}.bin"));
    fs::write(&dest, &bytes).expect("copy render font");
    dest.display().to_string()
}
