//! Parameter data model for the four effect blocks.
//!
//! Slider ranges and defaults follow the settings schema. Every block is a
//! plain data struct with a [`Default`] that reproduces the shipped neutral
//! values.

use serde::{Deserialize, Serialize};

/// Inclusive normalised slider range used by most parameters.
pub const UNIT_RANGE: std::ops::RangeInclusive<f32> = 0.0..=100.0;

fn clamp_unit(v: f32) -> f32 {
    v.clamp(*UNIT_RANGE.start(), *UNIT_RANGE.end())
}

/// Grain particle generator, exposed in the UI as Coarse / Smooth / Fine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum GrainType {
    /// Internal `grainType == 0` (shipped default).
    #[default]
    Coarse,
    /// Internal `grainType == 1`.
    Smooth,
    /// Internal `grainType == 2`.
    Fine,
}

/// Print simulation paper stock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PaperType {
    Matte,
    #[default]
    Neutral,
    Glossy,
}

/// Film stock used by the Film Color Shift block.
///
/// Nineteen stocks are defined: neutral plus 18 named stocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum FilmStock {
    #[default]
    #[serde(rename = "neutral")]
    Neutral,
    #[serde(rename = "kodak_portra_160")]
    KodakPortra160,
    #[serde(rename = "kodak_portra_160vc")]
    KodakPortra160Vc,
    #[serde(rename = "kodak_portra_400")]
    KodakPortra400,
    #[serde(rename = "kodak_portra_800")]
    KodakPortra800,
    #[serde(rename = "fujifilm_pro_400h")]
    FujifilmPro400h,
    #[serde(rename = "fuji_pro_160c")]
    FujiPro160c,
    #[serde(rename = "kodak_ektar_100")]
    KodakEktar100,
    #[serde(rename = "kodak_gold_100")]
    KodakGold100,
    #[serde(rename = "kodak_gold_200")]
    KodakGold200,
    #[serde(rename = "kodak_ultramax_400")]
    KodakUltramax400,
    #[serde(rename = "fuji_superia_400")]
    FujiSuperia400,
    #[serde(rename = "fuji_superia_800")]
    FujiSuperia800,
    #[serde(rename = "agfa_vista_400")]
    AgfaVista400,
    #[serde(rename = "kodak_tmax_100")]
    KodakTmax100,
    #[serde(rename = "kodak_tmax_3200")]
    KodakTmax3200,
    #[serde(rename = "ilford_hp5_400")]
    IlfordHp5_400,
    #[serde(rename = "ilford_fp4_125")]
    IlfordFp4_125,
    #[serde(rename = "kodak_trix_400")]
    KodakTrix400,
}

impl FilmStock {
    /// Stable snake_case identifier used by `.analog` recipes and the CLI.
    pub fn id(self) -> &'static str {
        match self {
            Self::Neutral => "neutral",
            Self::KodakPortra160 => "kodak_portra_160",
            Self::KodakPortra160Vc => "kodak_portra_160vc",
            Self::KodakPortra400 => "kodak_portra_400",
            Self::KodakPortra800 => "kodak_portra_800",
            Self::FujifilmPro400h => "fujifilm_pro_400h",
            Self::FujiPro160c => "fuji_pro_160c",
            Self::KodakEktar100 => "kodak_ektar_100",
            Self::KodakGold100 => "kodak_gold_100",
            Self::KodakGold200 => "kodak_gold_200",
            Self::KodakUltramax400 => "kodak_ultramax_400",
            Self::FujiSuperia400 => "fuji_superia_400",
            Self::FujiSuperia800 => "fuji_superia_800",
            Self::AgfaVista400 => "agfa_vista_400",
            Self::KodakTmax100 => "kodak_tmax_100",
            Self::KodakTmax3200 => "kodak_tmax_3200",
            Self::IlfordHp5_400 => "ilford_hp5_400",
            Self::IlfordFp4_125 => "ilford_fp4_125",
            Self::KodakTrix400 => "kodak_trix_400",
        }
    }

    /// Parse a stock from its stable snake_case id.
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "neutral" => Self::Neutral,
            "kodak_portra_160" => Self::KodakPortra160,
            "kodak_portra_160vc" => Self::KodakPortra160Vc,
            "kodak_portra_400" => Self::KodakPortra400,
            "kodak_portra_800" => Self::KodakPortra800,
            "fujifilm_pro_400h" => Self::FujifilmPro400h,
            "fuji_pro_160c" => Self::FujiPro160c,
            "kodak_ektar_100" => Self::KodakEktar100,
            "kodak_gold_100" => Self::KodakGold100,
            "kodak_gold_200" => Self::KodakGold200,
            "kodak_ultramax_400" => Self::KodakUltramax400,
            "fuji_superia_400" => Self::FujiSuperia400,
            "fuji_superia_800" => Self::FujiSuperia800,
            "agfa_vista_400" => Self::AgfaVista400,
            "kodak_tmax_100" => Self::KodakTmax100,
            "kodak_tmax_3200" => Self::KodakTmax3200,
            "ilford_hp5_400" => Self::IlfordHp5_400,
            "ilford_fp4_125" => Self::IlfordFp4_125,
            "kodak_trix_400" => Self::KodakTrix400,
            _ => return None,
        })
    }
}

impl std::fmt::Display for FilmStock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            Self::Neutral => "Neutral",
            Self::KodakPortra160 => "Kodak Portra 160",
            Self::KodakPortra160Vc => "Kodak Portra 160VC",
            Self::KodakPortra400 => "Kodak Portra 400",
            Self::KodakPortra800 => "Kodak Portra 800",
            Self::FujifilmPro400h => "Fujifilm Pro 400H",
            Self::FujiPro160c => "Fuji Pro 160C",
            Self::KodakEktar100 => "Kodak Ektar 100",
            Self::KodakGold100 => "Kodak Gold 100",
            Self::KodakGold200 => "Kodak Gold 200",
            Self::KodakUltramax400 => "Kodak UltraMax 400",
            Self::FujiSuperia400 => "Fuji Superia 400",
            Self::FujiSuperia800 => "Fuji Superia 800",
            Self::AgfaVista400 => "Agfa Vista 400",
            Self::KodakTmax100 => "Kodak T-MAX 100",
            Self::KodakTmax3200 => "Kodak T-MAX 3200",
            Self::IlfordHp5_400 => "Ilford HP5 Plus 400",
            Self::IlfordFp4_125 => "Ilford FP4 Plus 125",
            Self::KodakTrix400 => "Kodak Tri-X 400",
        };
        f.write_str(name)
    }
}

/// All film stocks, for pickers and iteration.
pub const ALL_FILM_STOCKS: [FilmStock; 19] = [
    FilmStock::Neutral,
    FilmStock::KodakPortra160,
    FilmStock::KodakPortra160Vc,
    FilmStock::KodakPortra400,
    FilmStock::KodakPortra800,
    FilmStock::FujifilmPro400h,
    FilmStock::FujiPro160c,
    FilmStock::KodakEktar100,
    FilmStock::KodakGold100,
    FilmStock::KodakGold200,
    FilmStock::KodakUltramax400,
    FilmStock::FujiSuperia400,
    FilmStock::FujiSuperia800,
    FilmStock::AgfaVista400,
    FilmStock::KodakTmax100,
    FilmStock::KodakTmax3200,
    FilmStock::IlfordHp5_400,
    FilmStock::IlfordFp4_125,
    FilmStock::KodakTrix400,
];

/// Grain block parameters.
///
/// Defaults reproduce a neutral (no grain) render except for the tonal
/// luminance controls, which sit at their neutral value of 50.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GrainParams {
    pub grain_type: GrainType,
    pub intensity: f32,
    pub grain_amount: f32,
    pub roughness: f32,
    pub grain_color: f32,
    pub color_shift: f32,
    pub dissolution_amount: f32,
    pub luminance_highlights: f32,
    pub luminance_mids: f32,
    pub luminance_shadows: f32,
    pub depth_of_field_intensity: f32,
    pub fine_grain_intensity: f32,
}

impl Default for GrainParams {
    fn default() -> Self {
        Self {
            grain_type: GrainType::default(),
            intensity: 50.0,
            grain_amount: 50.0,
            roughness: 50.0,
            grain_color: 50.0,
            color_shift: 50.0,
            dissolution_amount: 0.0,
            luminance_highlights: 50.0,
            luminance_mids: 50.0,
            luminance_shadows: 50.0,
            depth_of_field_intensity: 0.0,
            fine_grain_intensity: 0.0,
        }
    }
}

impl GrainParams {
    /// Clamp every field into its documented range.
    pub fn clamped(mut self) -> Self {
        self.intensity = clamp_unit(self.intensity);
        self.grain_amount = clamp_unit(self.grain_amount);
        self.roughness = clamp_unit(self.roughness);
        self.grain_color = clamp_unit(self.grain_color);
        self.color_shift = clamp_unit(self.color_shift);
        self.dissolution_amount = clamp_unit(self.dissolution_amount);
        self.luminance_highlights = clamp_unit(self.luminance_highlights);
        self.luminance_mids = clamp_unit(self.luminance_mids);
        self.luminance_shadows = clamp_unit(self.luminance_shadows);
        self.depth_of_field_intensity = clamp_unit(self.depth_of_field_intensity);
        self.fine_grain_intensity = clamp_unit(self.fine_grain_intensity);
        self
    }
}

/// Halation block parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HalationParams {
    pub halation_intensity: f32,
    pub halation_radius: f32,
    /// 0..=255, not a 0..=100 slider.
    pub halation_threshold: f32,
    pub halation_color_intensity: f32,
    pub halation_color_shift: f32,
    pub halation_color_tone: f32,
    pub halation_background_gain: f32,
    pub halation_background_focus: f32,
    pub light_spill_intensity: f32,
}

impl Default for HalationParams {
    fn default() -> Self {
        Self {
            halation_intensity: 0.0,
            halation_radius: 50.0,
            halation_threshold: 150.0,
            halation_color_intensity: 60.0,
            halation_color_shift: 50.0,
            halation_color_tone: 100.0,
            halation_background_gain: 50.0,
            halation_background_focus: 0.0,
            light_spill_intensity: 0.0,
        }
    }
}

impl HalationParams {
    /// Clamp every field into its documented range.
    pub fn clamped(mut self) -> Self {
        self.halation_intensity = clamp_unit(self.halation_intensity);
        self.halation_radius = clamp_unit(self.halation_radius);
        self.halation_threshold = self.halation_threshold.clamp(0.0, 255.0);
        self.halation_color_intensity = clamp_unit(self.halation_color_intensity);
        self.halation_color_shift = clamp_unit(self.halation_color_shift);
        self.halation_color_tone = clamp_unit(self.halation_color_tone);
        self.halation_background_gain = clamp_unit(self.halation_background_gain);
        self.halation_background_focus = clamp_unit(self.halation_background_focus);
        self.light_spill_intensity = clamp_unit(self.light_spill_intensity);
        self
    }
}

/// Print simulation block parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PrintParams {
    pub print_simulation: f32,
    pub print_sim_paper_type: PaperType,
    pub print_sim_paper_intensity: f32,
    pub print_sim_highlight_rolloff: f32,
    /// Signed: -50 (cool) ..= +50 (warm).
    pub print_sim_paper_tint: f32,
}

impl Default for PrintParams {
    fn default() -> Self {
        Self {
            print_simulation: 0.0,
            print_sim_paper_type: PaperType::default(),
            print_sim_paper_intensity: 50.0,
            print_sim_highlight_rolloff: 0.0,
            print_sim_paper_tint: 0.0,
        }
    }
}

impl PrintParams {
    /// Clamp every field into its documented range.
    pub fn clamped(mut self) -> Self {
        self.print_simulation = clamp_unit(self.print_simulation);
        self.print_sim_paper_intensity = clamp_unit(self.print_sim_paper_intensity);
        self.print_sim_highlight_rolloff = clamp_unit(self.print_sim_highlight_rolloff);
        self.print_sim_paper_tint = self.print_sim_paper_tint.clamp(-50.0, 50.0);
        self
    }
}

/// Film Color Shift block parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilmParams {
    pub film_stock_enabled: bool,
    pub film_stock: FilmStock,
    pub film_stock_intensity: f32,
}

impl Default for FilmParams {
    fn default() -> Self {
        Self {
            film_stock_enabled: false,
            film_stock: FilmStock::default(),
            film_stock_intensity: 0.0,
        }
    }
}

impl FilmParams {
    /// Clamp every field into its documented range.
    pub fn clamped(mut self) -> Self {
        self.film_stock_intensity = clamp_unit(self.film_stock_intensity);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grain_defaults_match_schema() {
        let g = GrainParams::default();
        assert_eq!(g.grain_type, GrainType::Coarse);
        assert_eq!(g.intensity, 50.0);
        assert_eq!(g.grain_amount, 50.0);
        assert_eq!(g.roughness, 50.0);
        assert_eq!(g.dissolution_amount, 0.0);
        assert_eq!(g.depth_of_field_intensity, 0.0);
    }

    #[test]
    fn halation_defaults_match_schema() {
        let h = HalationParams::default();
        assert_eq!(h.halation_intensity, 0.0);
        assert_eq!(h.halation_threshold, 150.0);
        assert_eq!(h.halation_color_intensity, 60.0);
        assert_eq!(h.halation_color_tone, 100.0);
    }

    #[test]
    fn clamping_respects_ranges() {
        let g = GrainParams {
            intensity: 900.0,
            color_shift: -20.0,
            ..Default::default()
        }
        .clamped();
        assert_eq!(g.intensity, 100.0);
        assert_eq!(g.color_shift, 0.0);

        let h = HalationParams {
            halation_threshold: 400.0,
            ..Default::default()
        }
        .clamped();
        assert_eq!(h.halation_threshold, 255.0);

        let p = PrintParams {
            print_sim_paper_tint: -80.0,
            ..Default::default()
        }
        .clamped();
        assert_eq!(p.print_sim_paper_tint, -50.0);
    }

    #[test]
    fn film_stock_ids_are_stable() {
        assert_eq!(FilmStock::Neutral.id(), "neutral");
        assert_eq!(FilmStock::IlfordHp5_400.id(), "ilford_hp5_400");
    }

    #[test]
    fn film_stock_id_roundtrips() {
        for s in [
            FilmStock::Neutral,
            FilmStock::KodakPortra400,
            FilmStock::IlfordHp5_400,
            FilmStock::KodakTrix400,
        ] {
            assert_eq!(FilmStock::from_id(s.id()), Some(s));
        }
        assert_eq!(FilmStock::from_id("nope"), None);
    }

    #[test]
    fn film_stock_serde_uses_stable_ids() {
        let json = serde_json::to_string(&FilmStock::KodakPortra400).unwrap();
        assert_eq!(json, "\"kodak_portra_400\"");
        let back: FilmStock = serde_json::from_str("\"ilford_hp5_400\"").unwrap();
        assert_eq!(back, FilmStock::IlfordHp5_400);
    }
}
