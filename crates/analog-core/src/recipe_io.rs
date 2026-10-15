//! `.analog` recipe import and export.
//!
//! An `.analog` file is an XMP packet using a custom namespace,
//! `xmlns:analog="urn:analog:recipe:1.0"` with `analog:schemaVersion="1"`.
//! Import also accepts a bare `.xmp`. The attribute names mirror the internal
//! settings schema so a recipe round-trips faithfully.

use std::collections::HashMap;
use std::fmt::Write as _;

use quick_xml::Reader;
use quick_xml::XmlVersion;
use quick_xml::events::Event;

use crate::error::{Error, Result};
use crate::params::{FilmStock, GrainType, PaperType};
use crate::recipe::{Recipe, SCHEMA_VERSION};

/// The `.analog` XMP namespace URI.
pub const RECIPE_NS: &str = "urn:analog:recipe:1.0";

/// Maximum accepted preset size.
pub const MAX_PRESET_BYTES: usize = 64 * 1024;

/// Serialise a recipe as a `.analog` XMP packet.
pub fn to_recipe_xmp(recipe: &Recipe) -> String {
    let mut attrs = String::new();
    let mut a = |name: &str, value: String| {
        let _ = write!(attrs, "\n    analog:{name}=\"{value}\"");
    };

    if let Some(name) = &recipe.name {
        a("presetName", escape(name));
    }
    a("schemaVersion", SCHEMA_VERSION.to_string());
    a("grainSeed", recipe.grain_seed.to_string());

    // Grain
    a(
        "grainType",
        grain_type_code(recipe.grain.grain_type).to_string(),
    );
    a("intensity", num(recipe.grain.intensity));
    a("grainAmount", num(recipe.grain.grain_amount));
    a("roughness", num(recipe.grain.roughness));
    a("grainColor", num(recipe.grain.grain_color));
    a("colorShift", num(recipe.grain.color_shift));
    a("dissolutionAmount", num(recipe.grain.dissolution_amount));
    a(
        "luminanceHighlights",
        num(recipe.grain.luminance_highlights),
    );
    a("luminanceMids", num(recipe.grain.luminance_mids));
    a("luminanceShadows", num(recipe.grain.luminance_shadows));
    a(
        "depthOfFieldIntensity",
        num(recipe.grain.depth_of_field_intensity),
    );
    a("fineGrainIntensity", num(recipe.grain.fine_grain_intensity));

    // Halation
    a("halationIntensity", num(recipe.halation.halation_intensity));
    a("halationRadius", num(recipe.halation.halation_radius));
    a("halationThreshold", num(recipe.halation.halation_threshold));
    a(
        "halationColorIntensity",
        num(recipe.halation.halation_color_intensity),
    );
    a(
        "halationColorShift",
        num(recipe.halation.halation_color_shift),
    );
    a(
        "halationColorTone",
        num(recipe.halation.halation_color_tone),
    );
    a(
        "halationBackgroundGain",
        num(recipe.halation.halation_background_gain),
    );
    a(
        "halationBackgroundFocus",
        num(recipe.halation.halation_background_focus),
    );
    a(
        "lightSpillIntensity",
        num(recipe.halation.light_spill_intensity),
    );

    // Print simulation
    a("printSimulation", num(recipe.print.print_simulation));
    a(
        "printSimPaperType",
        paper_id(recipe.print.print_sim_paper_type).to_string(),
    );
    a(
        "printSimPaperIntensity",
        num(recipe.print.print_sim_paper_intensity),
    );
    a(
        "printSimHighlightRolloff",
        num(recipe.print.print_sim_highlight_rolloff),
    );
    a("printSimPaperTint", num(recipe.print.print_sim_paper_tint));

    // Film color shift
    a(
        "filmStockEnabled",
        recipe.film.film_stock_enabled.to_string(),
    );
    a("filmStockName", recipe.film.film_stock.id().to_string());
    a("filmStockIntensity", num(recipe.film.film_stock_intensity));

    format!(
        "<?xpacket begin=\"\u{feff}\" id=\"W5M0MpCehiHzreSzNTczkc9d\"?>\n\
         <x:xmpmeta xmlns:x=\"adobe:ns:meta/\">\n\
         \x20<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\">\n\
         \x20 <rdf:Description rdf:about=\"\"\n\
         \x20   xmlns:analog=\"{RECIPE_NS}\"{attrs}/>\n\
         \x20</rdf:RDF>\n\
         </x:xmpmeta>\n\
         <?xpacket end=\"w\"?>"
    )
}

/// Parse a `.analog` or `.xmp` recipe. Unknown or missing attributes fall back
/// to the schema defaults, so a partial preset still loads.
// A preset parser starts from the defaults and overrides what is present; the
// per-field assignments read more clearly than one giant struct literal.
#[allow(clippy::field_reassign_with_default)]
pub fn from_recipe_xmp(xml: &str) -> Result<Recipe> {
    if xml.len() > MAX_PRESET_BYTES {
        return Err(Error::Recipe("preset exceeds 64 KB".into()));
    }
    let attrs = collect_recipe_attributes(xml)?;
    if attrs.is_empty() {
        return Err(Error::Recipe(
            "no analog attributes found (is this a .analog file?)".into(),
        ));
    }

    let mut r = Recipe::default();
    r.name = attrs
        .get("presetName")
        .map(|s| truncate_chars(s, 100))
        .filter(|s| !s.is_empty());

    set(&mut r.grain_seed, get_u64(&attrs, "grainSeed"));

    // Grain
    set(&mut r.grain.intensity, get_f32(&attrs, "intensity"));
    set(&mut r.grain.grain_amount, get_f32(&attrs, "grainAmount"));
    set(&mut r.grain.roughness, get_f32(&attrs, "roughness"));
    set(&mut r.grain.grain_color, get_f32(&attrs, "grainColor"));
    set(&mut r.grain.color_shift, get_f32(&attrs, "colorShift"));
    set(
        &mut r.grain.dissolution_amount,
        get_f32(&attrs, "dissolutionAmount"),
    );
    set(
        &mut r.grain.luminance_highlights,
        get_f32(&attrs, "luminanceHighlights"),
    );
    set(
        &mut r.grain.luminance_mids,
        get_f32(&attrs, "luminanceMids"),
    );
    set(
        &mut r.grain.luminance_shadows,
        get_f32(&attrs, "luminanceShadows"),
    );
    set(
        &mut r.grain.depth_of_field_intensity,
        get_f32(&attrs, "depthOfFieldIntensity"),
    );
    set(
        &mut r.grain.fine_grain_intensity,
        get_f32(&attrs, "fineGrainIntensity"),
    );
    if let Some(code) = get_u64(&attrs, "grainType") {
        r.grain.grain_type = grain_type_from_code(code as u32);
    }

    // Halation
    set(
        &mut r.halation.halation_intensity,
        get_f32(&attrs, "halationIntensity"),
    );
    set(
        &mut r.halation.halation_radius,
        get_f32(&attrs, "halationRadius"),
    );
    set(
        &mut r.halation.halation_threshold,
        get_f32(&attrs, "halationThreshold"),
    );
    set(
        &mut r.halation.halation_color_intensity,
        get_f32(&attrs, "halationColorIntensity"),
    );
    set(
        &mut r.halation.halation_color_shift,
        get_f32(&attrs, "halationColorShift"),
    );
    set(
        &mut r.halation.halation_color_tone,
        get_f32(&attrs, "halationColorTone"),
    );
    set(
        &mut r.halation.halation_background_gain,
        get_f32(&attrs, "halationBackgroundGain"),
    );
    set(
        &mut r.halation.halation_background_focus,
        get_f32(&attrs, "halationBackgroundFocus"),
    );
    set(
        &mut r.halation.light_spill_intensity,
        get_f32(&attrs, "lightSpillIntensity"),
    );

    // Print simulation
    set(
        &mut r.print.print_simulation,
        get_f32(&attrs, "printSimulation"),
    );
    set(
        &mut r.print.print_sim_paper_intensity,
        get_f32(&attrs, "printSimPaperIntensity"),
    );
    set(
        &mut r.print.print_sim_highlight_rolloff,
        get_f32(&attrs, "printSimHighlightRolloff"),
    );
    set(
        &mut r.print.print_sim_paper_tint,
        get_f32(&attrs, "printSimPaperTint"),
    );
    if let Some(paper) = attrs.get("printSimPaperType") {
        r.print.print_sim_paper_type = paper_from_id(paper);
    }

    // Film color shift
    if let Some(v) = attrs.get("filmStockEnabled") {
        r.film.film_stock_enabled = v.eq_ignore_ascii_case("true") || v == "1";
    }
    if let Some(v) = attrs.get("filmStockName").or_else(|| attrs.get("filmType"))
        && let Some(stock) = FilmStock::from_id(v)
    {
        r.film.film_stock = stock;
    }
    set(
        &mut r.film.film_stock_intensity,
        get_f32(&attrs, "filmStockIntensity"),
    );

    Ok(r.clamped())
}

/// Assign `value` to `slot` when present.
fn set<T>(slot: &mut T, value: Option<T>) {
    if let Some(v) = value {
        *slot = v;
    }
}

/// Read the `analog:*` attributes from the first `rdf:Description` element.
fn collect_recipe_attributes(xml: &str) -> Result<HashMap<String, String>> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let mut map = HashMap::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = e.name();
                if name.as_ref() == "rdf:Description" || name.as_ref() == "Description" {
                    for attr in e.attributes().flatten() {
                        let key = attr.key.as_ref();
                        if let Some(stripped) = key.strip_prefix("analog:") {
                            let value = attr
                                .normalized_value(XmlVersion::default())
                                .map_err(|err| Error::Recipe(err.to_string()))?
                                .to_string();
                            map.insert(stripped.to_string(), value);
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Error::Recipe(format!("xml parse error: {e}"))),
            _ => {}
        }
    }
    Ok(map)
}

fn get_f32(attrs: &HashMap<String, String>, key: &str) -> Option<f32> {
    attrs.get(key).and_then(|v| v.parse::<f32>().ok())
}

fn get_u64(attrs: &HashMap<String, String>, key: &str) -> Option<u64> {
    attrs.get(key).and_then(|v| v.parse::<u64>().ok())
}

/// Format a float without a trailing `.0` for whole numbers.
fn num(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn grain_type_code(t: GrainType) -> u32 {
    match t {
        GrainType::Coarse => 0,
        GrainType::Smooth => 1,
        GrainType::Fine => 2,
    }
}

fn grain_type_from_code(code: u32) -> GrainType {
    match code {
        1 => GrainType::Smooth,
        2 => GrainType::Fine,
        _ => GrainType::Coarse,
    }
}

fn paper_id(p: PaperType) -> &'static str {
    match p {
        PaperType::Matte => "matte",
        PaperType::Neutral => "neutral",
        PaperType::Glossy => "glossy",
    }
}

fn paper_from_id(id: &str) -> PaperType {
    match id {
        "matte" | "uncoated" => PaperType::Matte,
        "glossy" | "coated" => PaperType::Glossy,
        _ => PaperType::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::params::{FilmParams, GrainParams, HalationParams, PrintParams};

    fn rich_recipe() -> Recipe {
        Recipe {
            name: Some("portra-night".into()),
            grain_seed: 0xdead_beef,
            grain: GrainParams {
                grain_type: GrainType::Fine,
                intensity: 45.0,
                dissolution_amount: 12.0,
                ..Default::default()
            },
            halation: HalationParams {
                halation_intensity: 70.0,
                halation_radius: 65.0,
                ..Default::default()
            },
            print: PrintParams {
                print_simulation: 55.0,
                print_sim_paper_type: PaperType::Matte,
                print_sim_paper_tint: 20.0,
                ..Default::default()
            },
            film: FilmParams {
                film_stock_enabled: true,
                film_stock: FilmStock::KodakPortra400,
                film_stock_intensity: 85.0,
            },
            ..Recipe::default()
        }
    }

    #[test]
    fn roundtrips_through_xmp() {
        let recipe = rich_recipe();
        let xml = to_recipe_xmp(&recipe);
        let back = from_recipe_xmp(&xml).unwrap();
        assert_eq!(back, recipe);
    }

    #[test]
    fn rejects_non_presets() {
        assert!(from_recipe_xmp("<html></html>").is_err());
    }

    #[test]
    fn accepts_coated_as_glossy() {
        let xml = to_recipe_xmp(&rich_recipe());
        let with_coated = xml.replace(
            "printSimPaperType=\"matte\"",
            "printSimPaperType=\"coated\"",
        );
        let back = from_recipe_xmp(&with_coated).unwrap();
        assert_eq!(back.print.print_sim_paper_type, PaperType::Glossy);
    }

    #[test]
    fn truncated_name_is_capped() {
        let recipe = Recipe {
            name: Some("x".repeat(500)),
            ..Recipe::default()
        };
        let xml = to_recipe_xmp(&recipe);
        let back = from_recipe_xmp(&xml).unwrap();
        assert_eq!(back.name.unwrap().chars().count(), 100);
    }
}
