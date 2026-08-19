// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

use std::{
    collections::BTreeMap,
    fmt, fs,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

const BRAND_SCHEMA_VERSION: u32 = 1;
const TOKEN_SCHEMA_VERSION: u32 = 1;
const MAX_METADATA_BYTES: u64 = 16 * 1024;
const MAX_TOKEN_BYTES: u64 = 32 * 1024;
const MAX_LOGO_BYTES: u64 = 64 * 1024;
const FIXED_DARK_POSITIVE: &str = "#46f3b5";
const FIXED_DARK_WARNING: &str = "#fbbf24";
const FIXED_DARK_CRITICAL: &str = "#ff5c8a";
const FIXED_DARK_INFO: &str = "#38d5ff";
const FIXED_LIGHT_POSITIVE: &str = "#006b47";
const FIXED_LIGHT_WARNING: &str = "#7a4d00";
const FIXED_LIGHT_CRITICAL: &str = "#a10037";
const FIXED_LIGHT_INFO: &str = "#006b85";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrandMetadata {
    pub product_name: String,
    pub wordmark: String,
    pub tagline: String,
    pub bundle_identifier: String,
    pub publisher: String,
    pub show_vault_card: bool,
    pub logo: PathBuf,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BrandTokens {
    schema_version: u32,
    font_family: FontFamily,
    radius_personality: RadiusPersonality,
    dark: ColorScheme,
    light: ColorScheme,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum FontFamily {
    SystemSans,
    HumanistSans,
}

impl FontFamily {
    const fn css(self) -> &'static str {
        match self {
            Self::SystemSans => {
                "Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, \"Segoe UI\", sans-serif"
            }
            Self::HumanistSans => {
                "Avenir Next, Avenir, ui-sans-serif, system-ui, -apple-system, sans-serif"
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum RadiusPersonality {
    Square,
    Soft,
    Rounded,
}

impl RadiusPersonality {
    const fn card(self) -> &'static str {
        match self {
            Self::Square => "0.25rem",
            Self::Soft => "0.75rem",
            Self::Rounded => "1.25rem",
        }
    }

    const fn control(self) -> &'static str {
        match self {
            Self::Square => "0.125rem",
            Self::Soft => "0.5rem",
            Self::Rounded => "0.75rem",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ColorScheme {
    surface_0: String,
    surface_1: String,
    surface_2: String,
    surface_3: String,
    surface_4: String,
    text_strong: String,
    text: String,
    text_soft: String,
    text_muted: String,
    accent: String,
    accent_alt: String,
    family_vault: String,
    on_accent: String,
    overlay_white: String,
    shadow_ink: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrandPack {
    pub name: String,
    pub metadata: BrandMetadata,
    pub tokens: BrandTokens,
    pub logo_svg: String,
    directory: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedBrand {
    pub rust_path: PathBuf,
    pub css_path: PathBuf,
    pub logo_path: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BrandBuildError {
    message: String,
}

impl BrandBuildError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for BrandBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for BrandBuildError {}

pub fn load_brand_pack(directory: impl AsRef<Path>) -> Result<BrandPack, BrandBuildError> {
    let directory = checked_directory(directory.as_ref())?;
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| valid_slug(name))
        .ok_or_else(|| BrandBuildError::new("brand directory name must be a lowercase slug"))?
        .to_owned();
    let metadata_path =
        checked_regular_file(&directory, Path::new("brand.toml"), MAX_METADATA_BYTES)?;
    let token_path = checked_regular_file(&directory, Path::new("tokens.json"), MAX_TOKEN_BYTES)?;
    let metadata_text = read_utf8(&metadata_path, "brand metadata")?;
    let metadata = parse_brand_metadata(&metadata_text)?;
    let logo_path = checked_regular_file(&directory, &metadata.logo, MAX_LOGO_BYTES)?;
    let logo_svg = read_utf8(&logo_path, "brand logo")?;
    validate_svg(&logo_svg)?;
    let token_text = read_utf8(&token_path, "brand tokens")?;
    let tokens = serde_json::from_str::<BrandTokens>(&token_text)
        .map_err(|_| BrandBuildError::new("brand tokens must match the closed JSON schema"))?;
    validate_tokens(&tokens)?;

    Ok(BrandPack {
        name,
        metadata,
        tokens,
        logo_svg,
        directory,
    })
}

pub fn generate_brand(
    pack_directory: impl AsRef<Path>,
    output_directory: impl AsRef<Path>,
) -> Result<GeneratedBrand, BrandBuildError> {
    let brand = load_brand_pack(pack_directory)?;
    let output_directory = output_directory.as_ref();
    fs::create_dir_all(output_directory)
        .map_err(|_| BrandBuildError::new("brand output directory could not be created"))?;
    let css_path = output_directory.join("brand.css");
    let logo_path = output_directory.join("brand-logo.svg");
    let rust_path = output_directory.join("brand.rs");
    fs::write(&css_path, render_brand_css(&brand))
        .map_err(|_| BrandBuildError::new("generated brand CSS could not be written"))?;
    fs::write(&logo_path, &brand.logo_svg)
        .map_err(|_| BrandBuildError::new("generated brand logo could not be written"))?;
    fs::write(&rust_path, render_brand_rust(&brand))
        .map_err(|_| BrandBuildError::new("generated brand Rust could not be written"))?;
    Ok(GeneratedBrand {
        rust_path,
        css_path,
        logo_path,
    })
}

pub fn validate_app_manifest(
    brand: &BrandPack,
    manifest_path: impl AsRef<Path>,
) -> Result<(), BrandBuildError> {
    let manifest_path = manifest_path.as_ref();
    let metadata = fs::metadata(manifest_path)
        .map_err(|_| BrandBuildError::new("thin app Dioxus manifest is unavailable"))?;
    if !metadata.is_file() || metadata.len() > MAX_METADATA_BYTES {
        return Err(BrandBuildError::new("thin app Dioxus manifest is invalid"));
    }
    let manifest = read_utf8(manifest_path, "thin app Dioxus manifest")?;
    let identifier = manifest_string(&manifest, "identifier")?;
    let publisher = manifest_string(&manifest, "publisher")?;
    if identifier != brand.metadata.bundle_identifier {
        return Err(BrandBuildError::new(
            "thin app bundle identifier does not match its brand pack",
        ));
    }
    if publisher != brand.metadata.publisher {
        return Err(BrandBuildError::new(
            "thin app publisher does not match its brand pack",
        ));
    }
    for purpose in ["NSCameraUsageDescription", "NSFaceIDUsageDescription"] {
        let value = manifest_string(&manifest, purpose)?;
        let expected = match purpose {
            "NSCameraUsageDescription" => format!(
                "Scan credential offers and identity requests into your {} wallet.",
                brand.metadata.product_name
            ),
            "NSFaceIDUsageDescription" => format!(
                "Authorize access to your protected {} wallet.",
                brand.metadata.product_name
            ),
            _ => unreachable!("closed purpose-string set"),
        };
        if value != expected {
            return Err(BrandBuildError::new(format!(
                "{purpose} must match the code-owned purpose template"
            )));
        }
    }
    Ok(())
}

pub fn check_brand_path(path: impl AsRef<Path>) -> Result<Vec<String>, BrandBuildError> {
    let path = path.as_ref();
    if path.join("brand.toml").is_file() {
        return load_brand_pack(path).map(|brand| vec![brand.name]);
    }
    let root = checked_directory(path)?;
    let mut directories = fs::read_dir(&root)
        .map_err(|_| BrandBuildError::new("brands root could not be enumerated"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| BrandBuildError::new("brands root could not be enumerated"))?;
    directories.sort_by_key(fs::DirEntry::file_name);
    let mut names = Vec::new();
    for entry in directories {
        let file_type = entry
            .file_type()
            .map_err(|_| BrandBuildError::new("brand entry could not be inspected"))?;
        if file_type.is_symlink() || !file_type.is_dir() {
            return Err(BrandBuildError::new(
                "brands root may contain only real brand directories",
            ));
        }
        names.push(load_brand_pack(entry.path())?.name);
    }
    if names.is_empty() {
        return Err(BrandBuildError::new("brands root contains no brand packs"));
    }
    Ok(names)
}

pub fn render_brand_css(brand: &BrandPack) -> String {
    let mut css = String::from("/* Generated by oxid-brand-build; do not edit. */\n");
    css.push_str(":root {\n  /* OXID DESIGN TOKENS START */\n  color-scheme: dark;\n");
    css.push_str(&format!(
        "  --brand-font-family: {};\n  --radius-card: {};\n  --radius-control: {};\n  --radius-pill: 999px;\n",
        brand.tokens.font_family.css(),
        brand.tokens.radius_personality.card(),
        brand.tokens.radius_personality.control(),
    ));
    append_scheme_primitives(&mut css, "dark", &brand.tokens.dark);
    append_scheme_primitives(&mut css, "light", &brand.tokens.light);
    append_fixed_primitives(&mut css);
    append_semantic_scheme(&mut css, "dark", &brand.tokens.dark);
    css.push_str("  /* OXID DESIGN TOKENS END */\n}\n\n");
    css.push_str(":root[data-theme=\"light\"] {\n  color-scheme: light;\n");
    append_semantic_scheme(&mut css, "light", &brand.tokens.light);
    css.push_str("}\n");
    css
}

fn render_brand_rust(brand: &BrandPack) -> String {
    format!(
        "// Generated by oxid-brand-build; do not edit.\n\
         pub const BRAND_PROFILE: oxid_ui_dioxus::BrandProfile = oxid_ui_dioxus::BrandProfile::new(\n\
             {product_name:?},\n\
             {wordmark:?},\n\
             {tagline:?},\n\
             {bundle_identifier:?},\n\
             {publisher:?},\n\
             {show_vault_card},\n\
             include_str!(concat!(env!(\"OUT_DIR\"), \"/brand.css\")),\n\
             include_str!(concat!(env!(\"OUT_DIR\"), \"/brand-logo.svg\")),\n\
         );\n",
        product_name = brand.metadata.product_name,
        wordmark = brand.metadata.wordmark,
        tagline = brand.metadata.tagline,
        bundle_identifier = brand.metadata.bundle_identifier,
        publisher = brand.metadata.publisher,
        show_vault_card = brand.metadata.show_vault_card,
    )
}

fn parse_brand_metadata(input: &str) -> Result<BrandMetadata, BrandBuildError> {
    let mut values = BTreeMap::<String, MetadataValue>::new();
    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, raw_value) = line.split_once('=').ok_or_else(|| {
            BrandBuildError::new("brand metadata line must contain one equals sign")
        })?;
        let key = key.trim();
        if !key
            .chars()
            .all(|character| character.is_ascii_lowercase() || character == '_')
        {
            return Err(BrandBuildError::new("brand metadata key is invalid"));
        }
        let raw_value = raw_value.trim();
        let value = if raw_value.starts_with('"') {
            MetadataValue::Text(
                serde_json::from_str(raw_value)
                    .map_err(|_| BrandBuildError::new("brand metadata string is invalid"))?,
            )
        } else if let Ok(value) = raw_value.parse::<bool>() {
            MetadataValue::Boolean(value)
        } else if let Ok(value) = raw_value.parse::<u32>() {
            MetadataValue::Integer(value)
        } else {
            return Err(BrandBuildError::new("brand metadata value is invalid"));
        };
        if values.insert(key.to_owned(), value).is_some() {
            return Err(BrandBuildError::new(
                "brand metadata contains a duplicate key",
            ));
        }
    }

    let schema_version = take_integer(&mut values, "schema_version")?;
    if schema_version != BRAND_SCHEMA_VERSION {
        return Err(BrandBuildError::new(
            "unsupported brand metadata schema version",
        ));
    }
    let product_name = take_text(&mut values, "product_name")?;
    let wordmark = take_text(&mut values, "wordmark")?;
    let tagline = take_text(&mut values, "tagline")?;
    let bundle_identifier = take_text(&mut values, "bundle_identifier")?;
    let publisher = take_text(&mut values, "publisher")?;
    let logo = PathBuf::from(take_text(&mut values, "logo")?);
    let show_vault_card = take_boolean(&mut values, "show_vault_card")?;
    if !values.is_empty() {
        return Err(BrandBuildError::new(format!(
            "unknown brand metadata key: {}",
            values.keys().next().expect("non-empty map")
        )));
    }

    validate_text("product name", &product_name, 2, 48)?;
    validate_text("wordmark", &wordmark, 2, 24)?;
    validate_text("tagline", &tagline, 2, 80)?;
    validate_text("publisher", &publisher, 2, 64)?;
    if !valid_bundle_identifier(&bundle_identifier) {
        return Err(BrandBuildError::new("brand bundle identifier is invalid"));
    }
    if logo.is_absolute()
        || logo.extension().and_then(|extension| extension.to_str()) != Some("svg")
        || !matches!(logo.components().next(), Some(Component::Normal(part)) if part == "assets")
        || logo
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(BrandBuildError::new(
            "brand logo must be a relative SVG under assets",
        ));
    }

    Ok(BrandMetadata {
        product_name,
        wordmark,
        tagline,
        bundle_identifier,
        publisher,
        show_vault_card,
        logo,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum MetadataValue {
    Text(String),
    Boolean(bool),
    Integer(u32),
}

fn take_text(
    values: &mut BTreeMap<String, MetadataValue>,
    key: &str,
) -> Result<String, BrandBuildError> {
    match values.remove(key) {
        Some(MetadataValue::Text(value)) => Ok(value),
        Some(_) => Err(BrandBuildError::new(format!("{key} must be a string"))),
        None => Err(BrandBuildError::new(format!(
            "missing brand metadata key: {key}"
        ))),
    }
}

fn take_boolean(
    values: &mut BTreeMap<String, MetadataValue>,
    key: &str,
) -> Result<bool, BrandBuildError> {
    match values.remove(key) {
        Some(MetadataValue::Boolean(value)) => Ok(value),
        Some(_) => Err(BrandBuildError::new(format!("{key} must be a boolean"))),
        None => Err(BrandBuildError::new(format!(
            "missing brand metadata key: {key}"
        ))),
    }
}

fn take_integer(
    values: &mut BTreeMap<String, MetadataValue>,
    key: &str,
) -> Result<u32, BrandBuildError> {
    match values.remove(key) {
        Some(MetadataValue::Integer(value)) => Ok(value),
        Some(_) => Err(BrandBuildError::new(format!("{key} must be an integer"))),
        None => Err(BrandBuildError::new(format!(
            "missing brand metadata key: {key}"
        ))),
    }
}

fn validate_text(
    label: &str,
    value: &str,
    minimum: usize,
    maximum: usize,
) -> Result<(), BrandBuildError> {
    let count = value.chars().count();
    if count < minimum
        || count > maximum
        || value.trim() != value
        || value.contains("  ")
        || !value.chars().all(|character| {
            character.is_ascii_alphanumeric()
                || matches!(character, ' ' | '-' | '.' | ',' | '&' | '/' | ':' | '+')
        })
    {
        return Err(BrandBuildError::new(format!("brand {label} is invalid")));
    }
    Ok(())
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && !value.starts_with('-')
        && !value.ends_with('-')
        && value.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

fn valid_bundle_identifier(value: &str) -> bool {
    value.len() <= 128
        && value.split('.').count() >= 3
        && value.split('.').all(|part| {
            !part.is_empty()
                && part.len() <= 63
                && part
                    .chars()
                    .next()
                    .is_some_and(|character| character.is_ascii_alphabetic())
                && part
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '-')
        })
}

fn validate_tokens(tokens: &BrandTokens) -> Result<(), BrandBuildError> {
    if tokens.schema_version != TOKEN_SCHEMA_VERSION {
        return Err(BrandBuildError::new(
            "unsupported brand token schema version",
        ));
    }
    validate_scheme("dark", &tokens.dark, false)?;
    validate_scheme("light", &tokens.light, true)?;
    Ok(())
}

fn validate_scheme(name: &str, scheme: &ColorScheme, light: bool) -> Result<(), BrandBuildError> {
    let colors = [
        ("surface_0", &scheme.surface_0),
        ("surface_1", &scheme.surface_1),
        ("surface_2", &scheme.surface_2),
        ("surface_3", &scheme.surface_3),
        ("surface_4", &scheme.surface_4),
        ("text_strong", &scheme.text_strong),
        ("text", &scheme.text),
        ("text_soft", &scheme.text_soft),
        ("text_muted", &scheme.text_muted),
        ("accent", &scheme.accent),
        ("accent_alt", &scheme.accent_alt),
        ("family_vault", &scheme.family_vault),
        ("on_accent", &scheme.on_accent),
        ("overlay_white", &scheme.overlay_white),
        ("shadow_ink", &scheme.shadow_ink),
    ];
    let mut parsed = BTreeMap::new();
    for (label, value) in colors {
        parsed.insert(
            label,
            parse_color(value).map_err(|_| {
                BrandBuildError::new(format!("{name}.{label} must be an opaque #RRGGBB color"))
            })?,
        );
    }
    let surfaces = [
        "surface_0",
        "surface_1",
        "surface_2",
        "surface_3",
        "surface_4",
    ];
    for text in ["text_strong", "text", "text_soft", "text_muted"] {
        for surface in surfaces {
            require_contrast(name, text, parsed[text], surface, parsed[surface], 4.5)?;
        }
    }
    for accent in ["accent", "accent_alt", "family_vault"] {
        for surface in surfaces {
            require_contrast(name, accent, parsed[accent], surface, parsed[surface], 3.0)?;
        }
        require_contrast(
            name,
            "on_accent",
            parsed["on_accent"],
            accent,
            parsed[accent],
            4.5,
        )?;
    }
    let fixed = if light {
        [
            (
                "fixed_positive",
                parse_color(FIXED_LIGHT_POSITIVE).expect("fixed color"),
            ),
            (
                "fixed_warning",
                parse_color(FIXED_LIGHT_WARNING).expect("fixed color"),
            ),
            (
                "fixed_critical",
                parse_color(FIXED_LIGHT_CRITICAL).expect("fixed color"),
            ),
            (
                "fixed_info",
                parse_color(FIXED_LIGHT_INFO).expect("fixed color"),
            ),
        ]
    } else {
        [
            (
                "fixed_positive",
                parse_color(FIXED_DARK_POSITIVE).expect("fixed color"),
            ),
            (
                "fixed_warning",
                parse_color(FIXED_DARK_WARNING).expect("fixed color"),
            ),
            (
                "fixed_critical",
                parse_color(FIXED_DARK_CRITICAL).expect("fixed color"),
            ),
            (
                "fixed_info",
                parse_color(FIXED_DARK_INFO).expect("fixed color"),
            ),
        ]
    };
    for (label, color) in fixed {
        for surface in surfaces {
            require_contrast(name, label, color, surface, parsed[surface], 3.0)?;
        }
    }
    Ok(())
}

fn require_contrast(
    scheme: &str,
    foreground_label: &str,
    foreground: [u8; 3],
    background_label: &str,
    background: [u8; 3],
    minimum: f64,
) -> Result<(), BrandBuildError> {
    let ratio = contrast_ratio(foreground, background);
    if ratio + f64::EPSILON < minimum {
        return Err(BrandBuildError::new(format!(
            "{scheme} contrast {foreground_label} on {background_label} is {ratio:.2}:1; requires {minimum:.1}:1"
        )));
    }
    Ok(())
}

fn parse_color(value: &str) -> Result<[u8; 3], ()> {
    if value.len() != 7 || !value.starts_with('#') {
        return Err(());
    }
    let mut output = [0_u8; 3];
    for (index, chunk) in value.as_bytes()[1..].chunks_exact(2).enumerate() {
        let chunk = std::str::from_utf8(chunk).map_err(|_| ())?;
        output[index] = u8::from_str_radix(chunk, 16).map_err(|_| ())?;
    }
    Ok(output)
}

fn contrast_ratio(left: [u8; 3], right: [u8; 3]) -> f64 {
    let left = relative_luminance(left);
    let right = relative_luminance(right);
    (left.max(right) + 0.05) / (left.min(right) + 0.05)
}

fn relative_luminance(color: [u8; 3]) -> f64 {
    let channels = color.map(|channel| {
        let value = f64::from(channel) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });
    0.2126 * channels[0] + 0.7152 * channels[1] + 0.0722 * channels[2]
}

fn append_scheme_primitives(output: &mut String, name: &str, scheme: &ColorScheme) {
    for (token, value) in scheme_pairs(scheme) {
        output.push_str(&format!("  --brand-{name}-{token}: {value};\n"));
    }
}

fn append_fixed_primitives(output: &mut String) {
    for (name, value) in [
        ("dark-positive", FIXED_DARK_POSITIVE),
        ("dark-warning", FIXED_DARK_WARNING),
        ("dark-critical", FIXED_DARK_CRITICAL),
        ("dark-info", FIXED_DARK_INFO),
        ("light-positive", FIXED_LIGHT_POSITIVE),
        ("light-warning", FIXED_LIGHT_WARNING),
        ("light-critical", FIXED_LIGHT_CRITICAL),
        ("light-info", FIXED_LIGHT_INFO),
    ] {
        output.push_str(&format!("  --fixed-{name}: {value};\n"));
    }
}

fn append_semantic_scheme(output: &mut String, name: &str, scheme: &ColorScheme) {
    for (token, _) in scheme_pairs(scheme) {
        output.push_str(&format!("  --{token}: var(--brand-{name}-{token});\n"));
    }
    output.push_str(&format!(
        "  --surface-raised: var(--brand-{name}-surface-2);\n  --surface-sheet: var(--brand-{name}-surface-3);\n  --family-assets: var(--brand-{name}-accent);\n  --family-identity: var(--brand-{name}-accent-alt);\n  --positive: var(--fixed-{name}-positive);\n  --warning: var(--fixed-{name}-warning);\n  --critical: var(--fixed-{name}-critical);\n  --info: var(--fixed-{name}-info);\n"
    ));
}

fn scheme_pairs(scheme: &ColorScheme) -> [(&'static str, &str); 15] {
    [
        ("surface-0", &scheme.surface_0),
        ("surface-1", &scheme.surface_1),
        ("surface-2", &scheme.surface_2),
        ("surface-3", &scheme.surface_3),
        ("surface-4", &scheme.surface_4),
        ("text-strong", &scheme.text_strong),
        ("text", &scheme.text),
        ("text-soft", &scheme.text_soft),
        ("text-muted", &scheme.text_muted),
        ("accent", &scheme.accent),
        ("accent-alt", &scheme.accent_alt),
        ("family-vault", &scheme.family_vault),
        ("on-accent", &scheme.on_accent),
        ("overlay-white", &scheme.overlay_white),
        ("shadow-ink", &scheme.shadow_ink),
    ]
}

fn checked_directory(path: &Path) -> Result<PathBuf, BrandBuildError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| BrandBuildError::new("brand directory is unavailable"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(BrandBuildError::new(
            "brand directory must be a real directory",
        ));
    }
    path.canonicalize()
        .map_err(|_| BrandBuildError::new("brand directory could not be canonicalized"))
}

fn checked_regular_file(
    directory: &Path,
    relative: &Path,
    maximum_bytes: u64,
) -> Result<PathBuf, BrandBuildError> {
    if relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(BrandBuildError::new("brand file path is invalid"));
    }
    let path = directory.join(relative);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|_| BrandBuildError::new("required brand file is unavailable"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > maximum_bytes {
        return Err(BrandBuildError::new("required brand file is invalid"));
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| BrandBuildError::new("brand file could not be canonicalized"))?;
    if !canonical.starts_with(directory) {
        return Err(BrandBuildError::new("brand file escapes its pack"));
    }
    Ok(canonical)
}

fn read_utf8(path: &Path, label: &str) -> Result<String, BrandBuildError> {
    fs::read_to_string(path).map_err(|_| BrandBuildError::new(format!("{label} must be UTF-8")))
}

fn validate_svg(svg: &str) -> Result<(), BrandBuildError> {
    let compact = svg
        .trim()
        .strip_prefix("<!-- SPDX-License-Identifier: Apache-2.0 -->")
        .map(str::trim)
        .unwrap_or_else(|| svg.trim());
    let lower = compact.to_ascii_lowercase();
    let forbidden = [
        "<?",
        "<!",
        "<script",
        "<style",
        "<iframe",
        "<object",
        "<embed",
        "<foreignobject",
        "<animate",
        "<discard",
        "<image",
        "<link",
        "<meta",
        "<set",
        "<use",
        "javascript:",
        "data:",
        "href",
        "style",
        "url(",
        "xml:base",
    ];
    if !lower.starts_with("<svg ")
        || !lower.ends_with("</svg>")
        || forbidden.iter().any(|value| lower.contains(value))
        || contains_svg_event_attribute(&lower)
    {
        return Err(BrandBuildError::new("brand logo is not a safe inline SVG"));
    }
    Ok(())
}

fn contains_svg_event_attribute(svg: &str) -> bool {
    let bytes = svg.as_bytes();
    let mut in_tag = false;
    let mut quote = None;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if let Some(delimiter) = quote {
            if byte == delimiter {
                quote = None;
            }
            index += 1;
            continue;
        }
        match byte {
            b'\'' | b'"' if in_tag => quote = Some(byte),
            b'<' => in_tag = true,
            b'>' => in_tag = false,
            byte if in_tag && byte.is_ascii_whitespace() => {
                index += 1;
                while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                    index += 1;
                }
                let start = index;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || matches!(bytes[index], b'-' | b'_' | b':'))
                {
                    index += 1;
                }
                if index > start && svg[start..index].starts_with("on") {
                    while index < bytes.len() && bytes[index].is_ascii_whitespace() {
                        index += 1;
                    }
                    if bytes.get(index) == Some(&b'=') {
                        return true;
                    }
                }
                continue;
            }
            _ => {}
        }
        index += 1;
    }
    false
}

fn manifest_string(manifest: &str, key: &str) -> Result<String, BrandBuildError> {
    let prefix = format!("{key} = ");
    let mut values = manifest
        .lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix(&prefix))
        .map(serde_json::from_str::<String>);
    let value = values
        .next()
        .ok_or_else(|| BrandBuildError::new(format!("thin app manifest is missing {key}")))?
        .map_err(|_| BrandBuildError::new(format!("thin app manifest {key} is invalid")))?;
    if values.next().is_some() {
        return Err(BrandBuildError::new(format!(
            "thin app manifest contains duplicate {key}"
        )));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::*;

    static TEST_DIRECTORY_COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let sequence = TEST_DIRECTORY_COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "oxid-brand-build-{}-{sequence}",
                std::process::id()
            ));
            fs::create_dir(&path).expect("test directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn default_brand_loads_and_generates_closed_outputs() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let brand = load_brand_pack(root.join("brands/oxid")).expect("default brand");
        assert_eq!(brand.name, "oxid");
        assert_eq!(brand.metadata.product_name, "Oxid");
        assert_eq!(brand.metadata.bundle_identifier, "io.medianox.oxid");
        assert!(brand.metadata.show_vault_card);
        let css = render_brand_css(&brand);
        assert!(css.contains("--brand-dark-surface-0: #070b14;"));
        assert!(css.contains(":root[data-theme=\"light\"]"));
        assert!(!css.contains("javascript:"));

        let output = TestDirectory::new();
        let generated = generate_brand(root.join("brands/oxid"), output.path()).expect("generate");
        assert!(generated.rust_path.is_file());
        assert!(generated.css_path.is_file());
        assert!(generated.logo_path.is_file());
        let rust = fs::read_to_string(generated.rust_path).expect("rust");
        assert!(rust.contains("BrandProfile::new"));
        assert!(rust.contains("io.medianox.oxid"));
    }

    #[test]
    fn default_brand_matches_the_thin_app_manifest() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let brand = load_brand_pack(root.join("brands/oxid")).expect("default brand");
        validate_app_manifest(&brand, root.join("apps/oxid/Dioxus.toml")).expect("manifest");
    }

    #[test]
    fn metadata_denies_unknown_duplicate_missing_and_unsafe_values() {
        let valid = valid_metadata();
        assert!(parse_brand_metadata(&format!("{valid}unknown = true\n")).is_err());
        assert!(parse_brand_metadata(&format!("{valid}product_name = \"Again\"\n")).is_err());
        assert!(parse_brand_metadata(&valid.replace("publisher = \"Publisher\"\n", "")).is_err());
        assert!(
            parse_brand_metadata(&valid.replace(
                "product_name = \"Example\"",
                "product_name = \"Bad; color:red\""
            ))
            .is_err()
        );
        assert!(
            parse_brand_metadata(
                &valid.replace("logo = \"assets/logo.svg\"", "logo = \"../logo.svg\"")
            )
            .is_err()
        );
        assert!(parse_brand_metadata(&valid.replace("io.example.wallet", "invalid")).is_err());
    }

    #[test]
    fn token_schema_denies_unknown_invalid_colors_and_low_contrast() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let tokens = fs::read_to_string(root.join("brands/oxid/tokens.json")).expect("tokens");
        let unknown = tokens.replacen(
            "\"schema_version\": 1,",
            "\"schema_version\": 1, \"unknown\": true,",
            1,
        );
        assert!(serde_json::from_str::<BrandTokens>(&unknown).is_err());

        let invalid = tokens.replacen("#070b14", "rgb(7 11 20)", 1);
        let invalid = serde_json::from_str::<BrandTokens>(&invalid).expect("schema");
        assert!(validate_tokens(&invalid).is_err());

        let low_contrast = tokens.replacen("#f8fafc", "#070b14", 1);
        let low_contrast = serde_json::from_str::<BrandTokens>(&low_contrast).expect("schema");
        let error = validate_tokens(&low_contrast).expect_err("contrast");
        assert!(error.to_string().contains("requires 4.5:1"));
    }

    #[test]
    fn unsafe_svg_and_manifest_drift_fail_closed() {
        assert!(validate_svg("<svg viewBox=\"0 0 1 1\"><script/></svg>").is_err());
        assert!(validate_svg("<svg viewBox=\"0 0 1 1\" onload = \"alert(1)\"></svg>").is_err());
        assert!(
            validate_svg("<svg viewBox=\"0 0 1 1\"><animate attributeName=\"x\"/></svg>").is_err()
        );
        assert!(validate_svg("<svg viewBox=\"0 0 1 1\"><path d=\"M0 0\"/></svg>").is_ok());

        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let brand = load_brand_pack(root.join("brands/oxid")).expect("default brand");
        let directory = TestDirectory::new();
        let manifest = fs::read_to_string(root.join("apps/oxid/Dioxus.toml")).expect("manifest");
        let manifest = manifest.replace("io.medianox.oxid", "io.example.wrong");
        let path = directory.path().join("Dioxus.toml");
        fs::write(&path, manifest).expect("write");
        assert!(validate_app_manifest(&brand, path).is_err());

        let manifest = fs::read_to_string(root.join("apps/oxid/Dioxus.toml")).expect("manifest");
        let manifest = manifest.replace(
            "Scan credential offers and identity requests into your Oxid wallet.",
            "Oxid needs the camera.",
        );
        let path = directory.path().join("Dioxus-purpose.toml");
        fs::write(&path, manifest).expect("write");
        assert!(validate_app_manifest(&brand, path).is_err());
    }

    #[test]
    fn brand_root_is_sorted_and_rejects_non_directory_entries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../brands");
        assert_eq!(check_brand_path(root).expect("brands"), vec!["oxid"]);

        let directory = TestDirectory::new();
        fs::write(directory.path().join("README.md"), "not a pack").expect("write");
        assert!(check_brand_path(directory.path()).is_err());
    }

    fn valid_metadata() -> String {
        "schema_version = 1\nproduct_name = \"Example\"\nwordmark = \"example\"\ntagline = \"Identity wallet\"\nbundle_identifier = \"io.example.wallet\"\npublisher = \"Publisher\"\nlogo = \"assets/logo.svg\"\nshow_vault_card = true\n".to_owned()
    }
}
