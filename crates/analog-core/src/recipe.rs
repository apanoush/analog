//! A recipe is the complete, reproducible description of a render.
//!
//! It bundles the four effect blocks with a deterministic grain seed.
//! [`Recipe::default`] reproduces the shipped schema defaults (grain is active
//! at intensity 50); use [`Recipe::neutral`] for an explicit pass-through.

use serde::{Deserialize, Serialize};

use crate::params::{FilmParams, GrainParams, HalationParams, PrintParams};

/// Current recipe schema version, mirroring `analog:schemaVersion="2"`.
///
/// Version 2 changed the `grainType` wire codes to match the original settings
/// schema (0 = Fine, 1 = Coarse, 2 = Smooth). Version 1 recipes are migrated on
/// import.
pub const SCHEMA_VERSION: u32 = 2;

/// The full set of parameters that defines a rendered look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recipe {
    pub schema_version: u32,
    /// Optional human-readable preset name (`analog:presetName`).
    pub name: Option<String>,
    /// Deterministic grain seed (`analog:grainSeed`); identical seeds must
    /// produce byte-identical grain on every backend.
    pub grain_seed: u64,
    pub grain: GrainParams,
    pub halation: HalationParams,
    pub print: PrintParams,
    pub film: FilmParams,
}

impl Default for Recipe {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            name: None,
            grain_seed: 0,
            grain: GrainParams::default(),
            halation: HalationParams::default(),
            print: PrintParams::default(),
            film: FilmParams::default(),
        }
    }
}

impl Recipe {
    /// A pass-through recipe: every effect strength is zero, film disabled.
    pub fn neutral() -> Self {
        let mut r = Self::default();
        r.grain.intensity = 0.0;
        r.halation.halation_intensity = 0.0;
        r.halation.light_spill_intensity = 0.0;
        r.print.print_simulation = 0.0;
        r.film.film_stock_enabled = false;
        r.film.film_stock_intensity = 0.0;
        r
    }

    /// Clamp every block into its documented range.
    pub fn clamped(self) -> Self {
        Self {
            grain: self.grain.clamped(),
            halation: self.halation.clamped(),
            print: self.print.clamped(),
            film: self.film.clamped(),
            ..self
        }
    }

    /// True when rendering this recipe leaves the input image unchanged.
    ///
    /// Grain is active only when both `intensity` is non-zero and a particle
    /// amount is configured; the other blocks gate on their own intensities.
    pub fn is_noop(&self) -> bool {
        let grain_off = self.grain.intensity == 0.0;
        let halation_off =
            self.halation.halation_intensity == 0.0 && self.halation.light_spill_intensity == 0.0;
        let print_off = self.print.print_simulation == 0.0;
        let film_off = !self.film.film_stock_enabled || self.film.film_stock_intensity == 0.0;
        grain_off && halation_off && print_off && film_off
    }

    /// True when any depth-dependent control is active.
    ///
    /// Such a recipe needs a depth map for full effect; without one the
    /// depth-dependent controls are ignored.
    pub fn needs_depth(&self) -> bool {
        self.grain.depth_of_field_intensity > 0.0 || self.halation.halation_background_focus > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutral_recipe_is_noop() {
        assert!(Recipe::neutral().is_noop());
    }

    #[test]
    fn default_recipe_has_grain_active() {
        // Schema default grain intensity is 50, so the shipped default is not a
        // pass-through.
        assert!(!Recipe::default().is_noop());
    }

    #[test]
    fn enabling_grain_is_not_noop() {
        let mut r = Recipe::default();
        r.grain.intensity = 40.0;
        assert!(!r.is_noop());
    }

    #[test]
    fn recipe_roundtrips_through_serde() {
        let mut r = Recipe::default();
        r.grain.intensity = 33.0;
        r.film.film_stock_enabled = true;
        r.film.film_stock_intensity = 70.0;
        r.grain_seed = 0xdead_beef;
        let json = serde_json::to_string(&r).unwrap();
        let back: Recipe = serde_json::from_str(&json).unwrap();
        assert_eq!(r, back);
    }
}
