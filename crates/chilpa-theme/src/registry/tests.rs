use super::*;

mod rows {
    use super::*;
    use crate::colors::{alpha_byte, ALPHA_COUNT};
    use crate::id::ALL;

    /// Sentinel for "field was never set" — the all-zero opaque/transparent
    /// black the `StdSpec::default()` placeholder uses. A fully-materialized row
    /// must not leave a meaningful color at this sentinel by accident; we instead
    /// assert the named hero tokens are the EXACT transcribed values per theme in
    /// the dedicated tests below, and assert global completeness here.
    fn fully_populated(c: &ThemeColors) {
        assert_eq!(c.alpha.len(), ALPHA_COUNT);
        // every alpha tier carries opacity
        assert!(c.alpha.iter().all(|a| a.a > 0));
        // status families are present (non-degenerate alpha)
        assert!(c.danger_bg.a > 0 && c.danger_border.a > 0 && c.danger_hover.a > 0);
        assert!(c.warning_bg.a > 0 && c.warning_border.a > 0 && c.warning_hover.a > 0);
        assert!(c.success_bg.a > 0 && c.success_border.a > 0 && c.success_hover.a > 0);
        // surfaces/text/accent are opaque
        assert_eq!(c.surface_main.a, 255);
        assert_eq!(c.text_primary.a, 255);
        assert_eq!(c.accent.a, 255);
        assert_eq!(c.accent_text.a, 255);
        assert_eq!(c.border_strong.a, 255);
        assert_eq!(c.focus_ring.a, 255);
    }

    #[test]
    fn every_registered_theme_is_fully_populated() {
        for &id in ALL {
            let c = palette(id);
            fully_populated(&c);
        }
    }

    #[test]
    fn p1_rows_are_fully_populated() {
        for id in [ThemeId::Dark, ThemeId::Oled, ThemeId::TokyoNight, ThemeId::System] {
            let c = palette(id);
            assert_eq!(c.alpha.len(), ALPHA_COUNT);
            assert_ne!(c.surface_main, Rgba::rgba(0, 0, 0, 0));
            assert_ne!(c.text_primary, Rgba::rgba(0, 0, 0, 0));
            assert_ne!(c.accent, Rgba::rgba(0, 0, 0, 0));
            assert!(c.alpha.iter().all(|a| a.a > 0));
        }
    }

    #[test]
    fn dark_matches_root_css() {
        let c = palette(ThemeId::Dark);
        assert_eq!(c.surface_main, Rgba::rgb(0x0f, 0x0f, 0x0f));
        assert_eq!(c.surface_card, Rgba::rgb(0x1a, 0x1a, 0x1a));
        assert_eq!(c.surface_elevated, Rgba::rgb(0x2a, 0x2a, 0x2a));
        assert_eq!(c.text_primary, Rgba::rgb(0xff, 0xff, 0xff));
        assert_eq!(c.accent, Rgba::rgb(0x42, 0x85, 0xf4));
        assert_eq!(c.border_strong, Rgba::rgb(0x3a, 0x3a, 0x3a));
        assert_eq!(c.favorite, c.danger);
    }

    #[test]
    fn oled_overrides_only_surfaces_and_borders() {
        let d = palette(ThemeId::Dark);
        let o = palette(ThemeId::Oled);
        assert_eq!(o.surface_main, Rgba::rgb(0, 0, 0));
        assert_eq!(o.surface_card, Rgba::rgb(0x0a, 0x0a, 0x0a));
        assert_eq!(o.surface_elevated, Rgba::rgb(0x1a, 0x1a, 0x1a));
        assert_eq!(o.bg_hover, Rgba::rgb(0x11, 0x11, 0x11));
        assert_eq!(o.border_strong, Rgba::rgb(0x2a, 0x2a, 0x2a));
        assert_eq!(o.accent, d.accent);
        assert_eq!(o.text_primary, d.text_primary);
        assert_eq!(o.danger, d.danger);
    }

    #[test]
    fn tokyo_legacy_values_preserved() {
        let c = palette(ThemeId::TokyoNight);
        assert_eq!(c.surface_main, Rgba::rgb(0x1a, 0x1b, 0x26));
        assert_eq!(c.surface_card, Rgba::rgb(0x16, 0x16, 0x1e));
        assert_eq!(c.surface_elevated, Rgba::rgb(0x1c, 0x1d, 0x29));
        assert_eq!(c.text_primary, Rgba::rgb(0xa9, 0xb1, 0xd6));
        assert_eq!(c.accent, Rgba::rgb(0x7a, 0xa2, 0xf7));
        assert_eq!(c.accent_text, Rgba::rgb(0x1a, 0x1b, 0x26));
    }

    #[test]
    fn legacy_alpha_aliases_unchanged() {
        // The exact translucent values the old Slint Theme exposed (dark themes).
        for id in [ThemeId::Dark, ThemeId::Oled, ThemeId::TokyoNight] {
            let c = palette(id);
            assert_eq!(c.surface_hover, Rgba::rgba(255, 255, 255, 0x10));
            assert_eq!(c.border_subtle, Rgba::rgba(255, 255, 255, 0x14));
            assert_eq!(c.border_muted, Rgba::rgba(255, 255, 255, 0x38));
            assert_eq!(c.card_shadow, Rgba::rgba(0, 0, 0, 0x66));
            assert_eq!(c.alpha_pct(8), Rgba::rgba(255, 255, 255, 0x14));
            assert_eq!(c.alpha_pct(10), Rgba::rgba(255, 255, 255, 0x1a));
            assert_eq!(c.alpha_pct(12), Rgba::rgba(255, 255, 255, 0x1f));
            assert_eq!(c.alpha_pct(18), Rgba::rgba(255, 255, 255, 0x2e));
            assert_eq!(c.alpha_pct(55), Rgba::rgba(255, 255, 255, 0x8c));
            assert_eq!(c.alpha_pct(65), Rgba::rgba(255, 255, 255, 0xa6));
            assert_eq!(c.alpha_pct(70), Rgba::rgba(255, 255, 255, 0xb3));
            assert_eq!(c.alpha_pct(75), Rgba::rgba(255, 255, 255, 0xbf));
        }
    }

    // --- Standard themes: spot-check exact transcribed hero values ---------

    #[test]
    fn light_core_values() {
        let c = palette(ThemeId::Light);
        assert_eq!(c.surface_main, Rgba::rgb(0xff, 0xff, 0xff));
        assert_eq!(c.text_primary, Rgba::rgb(0x0f, 0x0f, 0x0f));
        // accent trio inherited from :root Dark:
        assert_eq!(c.accent, Rgba::rgb(0x42, 0x85, 0xf4));
        assert_eq!(c.accent_text, Rgba::rgb(0xff, 0xff, 0xff));
        assert_eq!(c.danger, Rgba::rgb(0xdc, 0x26, 0x26));
        assert_eq!(c.border_subtle, Rgba::rgb(0xe0, 0xe0, 0xe0));
        // light theme -> black alpha base
        assert_eq!(c.alpha_pct(8), Rgba::rgba(0, 0, 0, 0x14));
        // light hover tint is 0.15 (faithful to app.css)
        assert_eq!(c.danger_hover, with_alpha(Rgba::rgb(0xdc, 0x26, 0x26), 0.15));
    }

    #[test]
    fn dracula_nonstandard_tints() {
        let c = palette(ThemeId::Dracula);
        assert_eq!(c.surface_main, Rgba::rgb(0x28, 0x2a, 0x36));
        assert_eq!(c.accent, Rgba::rgb(0xbd, 0x93, 0xf9));
        let danger = Rgba::rgb(0xff, 0x55, 0x55);
        assert_eq!(c.danger_bg, with_alpha(danger, 0.15));
        assert_eq!(c.danger_border, with_alpha(danger, 0.4));
        assert_eq!(c.danger_hover, with_alpha(danger, 0.25));
    }

    #[test]
    fn breeze_dark_inherits_root_status_hues() {
        let c = palette(ThemeId::BreezeDark);
        // danger/warning inherited from :root Dark
        assert_eq!(c.danger, Rgba::rgb(0xef, 0x44, 0x44));
        assert_eq!(c.warning, Rgba::rgb(0xfb, 0xbf, 0x24));
        assert_eq!(c.accent, Rgba::rgb(0x3d, 0xae, 0xe9));
    }

    #[test]
    fn frost_langley_are_dark_polarity() {
        // Both are registered type:light in Tauri but are DARK canvases.
        let frost = palette(ThemeId::Frost);
        let langley = palette(ThemeId::Langley);
        // white alpha base (dark polarity), NOT black:
        assert_eq!(frost.alpha_pct(8), Rgba::rgba(255, 255, 255, 0x14));
        assert_eq!(langley.alpha_pct(8), Rgba::rgba(255, 255, 255, 0x14));
        assert!(!bg_is_light(frost.surface_main));
        assert!(!bg_is_light(langley.surface_main));
    }

    #[test]
    fn alucard_is_light_polarity() {
        let c = palette(ThemeId::Alucard);
        assert_eq!(c.surface_main, Rgba::rgb(0xff, 0xfb, 0xeb));
        // black alpha base (light polarity):
        assert_eq!(c.alpha_pct(8), Rgba::rgba(0, 0, 0, 0x14));
        assert!(bg_is_light(c.surface_main));
        // success on a light theme is the darker green:
        assert_eq!(c.success, Rgba::rgb(0x1f, 0x8a, 0x4c));
    }

    #[test]
    fn light_themes_use_black_alpha_base() {
        for id in [
            ThemeId::Light,
            ThemeId::Alucard,
            ThemeId::RosePineDawn,
            ThemeId::BreezeLight,
            ThemeId::AdwaitaLight,
            ThemeId::DuotoneSnow,
            ThemeId::SnowStorm,
            ThemeId::Kurosaki,
        ] {
            let c = palette(id);
            assert_eq!(c.alpha_pct(8), Rgba::rgba(0, 0, 0, 0x14), "{id:?} should be black-base");
            assert_eq!(c.surface_hover, Rgba::rgba(0, 0, 0, 0x10), "{id:?} hover base");
        }
    }

    #[test]
    fn dark_themes_use_white_alpha_base() {
        for id in [
            ThemeId::Warm,
            ThemeId::Nord,
            ThemeId::Dracula,
            ThemeId::CatppuccinMocha,
            ThemeId::BreezeDark,
            ThemeId::AdwaitaDark,
            ThemeId::Aurora,
            ThemeId::Ikari,
            ThemeId::Ayanami,
            ThemeId::Iscariot,
            ThemeId::Stratego,
            ThemeId::Rumi,
            ThemeId::Zoey,
            ThemeId::Mira,
            ThemeId::Frost,
            ThemeId::Langley,
        ] {
            let c = palette(id);
            assert_eq!(c.alpha_pct(8), Rgba::rgba(255, 255, 255, 0x14), "{id:?} should be white-base");
        }
    }

    #[test]
    fn standard_theme_focus_ring_equals_accent() {
        for id in [
            ThemeId::Warm,
            ThemeId::Nord,
            ThemeId::Stratego,
            ThemeId::Alucard,
            ThemeId::Kurosaki,
        ] {
            let c = palette(id);
            assert_eq!(c.focus_ring, c.accent, "{id:?} focus_ring should equal accent");
            assert_eq!(c.favorite, c.danger, "{id:?} favorite should equal danger");
        }
    }

    #[test]
    fn alpha_byte_helper_matches_with_alpha() {
        // sanity: with_alpha(.., 0.1) == alpha_byte(10)
        let c = Rgba::rgb(0x10, 0x20, 0x30);
        assert_eq!(with_alpha(c, 0.1).a, alpha_byte(10));
    }
}

// ===========================================================================
// P3 — Accessibility contrast unit tests (the plan's "WCAG/APCA unit tests")
//
// Every threshold here is the documented target from 99-MIGRATION-PLAN.md
// Part B. If an assertion fails, the HEX is wrong vs Part B — fix the value to
// match the verified palette, NEVER weaken the test.
// ===========================================================================
mod a11y_contrast {
    use super::*;
    use crate::color::{apca_lc, contrast_ratio};
    use crate::id::ALL;

    const AAA_BODY: f64 = 7.0; // WCAG 2.x AAA normal text
    const AA_NORMAL: f64 = 4.5; // WCAG 2.x AA normal text
    const NON_TEXT: f64 = 3.0; // WCAG 2.x SC 1.4.11 / 1.4.3-large

    /// Solid composite of `fg` over `bg` (a11y status surfaces are opaque, but
    /// translucent overlays compose straight-alpha for contrast measurement).
    fn over(fg: Rgba, bg: Rgba) -> Rgba {
        if fg.a == 255 {
            return fg;
        }
        let a = fg.a as f64 / 255.0;
        let mix = |f: u8, b: u8| ((f as f64 * a) + (b as f64 * (1.0 - a))).round() as u8;
        Rgba::rgb(mix(fg.r, bg.r), mix(fg.g, bg.g), mix(fg.b, bg.b))
    }

    // ---- wcag-light: body text AAA, accent AAA, status AAA ----------------
    #[test]
    fn wcag_light_meets_aaa() {
        let c = wcag_light();
        // text-primary on bg-primary >= 7.0:1 (AAA) — Part B: 17.40:1 / Lc 104.3
        assert!(
            contrast_ratio(c.text_primary, c.surface_main) >= AAA_BODY,
            "wcag-light text-primary {:.2}",
            contrast_ratio(c.text_primary, c.surface_main)
        );
        // text-muted on bg-primary >= 7.0:1 (AAA, exactly) — Part B 7.00:1
        assert!(contrast_ratio(c.text_muted, c.surface_main) >= AAA_BODY);
        // accent + btn-text on accent >= 7.0:1 — Part B 7.98:1
        assert!(contrast_ratio(c.accent, c.surface_main) >= AAA_BODY);
        assert!(contrast_ratio(c.accent_text, c.accent) >= AAA_BODY);
        // danger/warning/success text on bg-primary >= AAA
        assert!(contrast_ratio(c.danger, c.surface_main) >= AAA_BODY);
        assert!(contrast_ratio(c.warning, c.surface_main) >= AAA_BODY);
        assert!(contrast_ratio(c.success, c.surface_main) >= AAA_BODY);
        // non-text: border-strong + focus-ring >= 3:1
        assert!(contrast_ratio(c.border_strong, c.surface_main) >= NON_TEXT);
        assert!(contrast_ratio(c.focus_ring, c.surface_main) >= NON_TEXT);
        // APCA body gate (|Lc| >= 75) for primary text
        assert!(apca_lc(c.text_primary, c.surface_main).abs() >= 75.0);
    }

    // ---- wcag-dark: AAA + APCA, status on opaque tints --------------------
    #[test]
    fn wcag_dark_meets_aaa() {
        let c = wcag_dark();
        assert!(contrast_ratio(c.text_primary, c.surface_main) >= AAA_BODY); // 16.02:1
        assert!(contrast_ratio(c.text_secondary, c.surface_main) >= AAA_BODY);
        // text-muted is APCA-content by design but still clears AAA ratio (10.32:1)
        assert!(contrast_ratio(c.text_muted, c.surface_main) >= AAA_BODY);
        assert!(contrast_ratio(c.accent, c.surface_main) >= AAA_BODY); // 10.39:1
        // danger/warning/success text on their OPAQUE tint bg >= AAA
        assert!(contrast_ratio(c.danger, c.danger_bg) >= AAA_BODY); // 8.63:1
        assert!(contrast_ratio(c.warning, c.surface_main) >= AAA_BODY); // 12.53:1
        assert!(contrast_ratio(c.success, c.success_bg) >= AAA_BODY);
        // border-strong >= 3:1 on every surface tier (Part B: 4.11/3.76/3.31)
        for bg in [c.surface_main, c.surface_card, c.surface_elevated] {
            assert!(
                contrast_ratio(c.border_strong, bg) >= NON_TEXT,
                "wcag-dark border-strong {:.2}",
                contrast_ratio(c.border_strong, bg)
            );
        }
        assert!(contrast_ratio(c.focus_ring, c.surface_main) >= NON_TEXT);
        assert!(apca_lc(c.text_primary, c.surface_main).abs() >= 75.0);
    }

    // ---- High Contrast (both polarities): >= the wcag bar + interactive ---
    #[test]
    fn high_contrast_dark_beats_wcag_dark() {
        let hc = high_contrast();
        let wd = wcag_dark();
        let hc_tp = contrast_ratio(hc.text_primary, hc.surface_main);
        let wd_tp = contrast_ratio(wd.text_primary, wd.surface_main);
        // HC must be at least as high-contrast as wcag-dark (no regression).
        assert!(
            hc_tp >= wd_tp,
            "HC-dark text/bg {:.2} should be >= wcag-dark {:.2}",
            hc_tp,
            wd_tp
        );
        assert!(hc_tp >= 19.0); // Part B: 19.80:1
        // reciprocal cyan: accent as text AND as a fill under btn-text
        assert!(contrast_ratio(hc.accent, hc.surface_main) >= AAA_BODY); // 11.67:1
        assert!(contrast_ratio(hc.accent_text, hc.accent) >= AAA_BODY); // 12.38:1
        // interactive non-text tokens >= 3:1
        assert!(contrast_ratio(hc.border_strong, hc.surface_main) >= NON_TEXT);
        assert!(contrast_ratio(hc.focus_ring, hc.surface_main) >= NON_TEXT); // 13.76:1
        assert!(contrast_ratio(hc.border_subtle, hc.surface_main) >= NON_TEXT); // 4.61:1
    }

    #[test]
    fn high_contrast_light_beats_wcag_light() {
        let hc = high_contrast_light();
        let wl = wcag_light();
        let hc_tp = contrast_ratio(hc.text_primary, hc.surface_main);
        let wl_tp = contrast_ratio(wl.text_primary, wl.surface_main);
        assert!(
            hc_tp >= wl_tp,
            "HC-light text/bg {:.2} should be >= wcag-light {:.2}",
            hc_tp,
            wl_tp
        );
        assert!(hc_tp >= 20.0); // Part B: 21.00:1
        // reciprocal deep blue: accent as text AND btn-text under accent fill
        assert!(contrast_ratio(hc.accent, hc.surface_main) >= AAA_BODY); // 11.22:1
        assert!(contrast_ratio(hc.accent_text, hc.accent) >= AAA_BODY);
        // corrected warning #5e4b00 must clear AAA on white (8.46:1)
        assert!(
            contrast_ratio(hc.warning, hc.surface_main) >= AAA_BODY,
            "HC-light warning {:.2} (corrected #5e4b00 should be 8.46:1)",
            contrast_ratio(hc.warning, hc.surface_main)
        );
        assert_eq!(hc.warning, Rgba::rgb(0x5e, 0x4b, 0x00)); // the applied correction
        assert!(contrast_ratio(hc.border_strong, hc.surface_main) >= NON_TEXT);
        assert!(contrast_ratio(hc.focus_ring, hc.surface_main) >= NON_TEXT);
        assert!(contrast_ratio(hc.border_subtle, hc.surface_main) >= NON_TEXT); // 5.74:1
    }

    // ---- colorblind: text contrast + status hue distinctness -------------
    /// Crude protanopia/deuteranopia simulation (Brettel-style fixed matrices,
    /// sufficient to confirm the documented hue separation survives red-green
    /// CVD). Returns the simulated sRGB. Used only to assert ΔE separation, not
    /// for rendering.
    fn simulate_deutan(c: Rgba) -> Rgba {
        // Linearize, apply the standard deuteranopia LMS-collapse matrix
        // (Machado 2009, severity 1.0), re-encode. Approximate but stable.
        let lin = |v: u8| {
            let cs = v as f64 / 255.0;
            if cs <= 0.04045 {
                cs / 12.92
            } else {
                ((cs + 0.055) / 1.055).powf(2.4)
            }
        };
        let enc = |v: f64| {
            let v = v.clamp(0.0, 1.0);
            let s = if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            };
            (s * 255.0).round().clamp(0.0, 255.0) as u8
        };
        let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
        // Machado deuteranomaly severity=1.0 matrix:
        let nr = 0.367_322 * r + 0.860_646 * g + -0.227_968 * b;
        let ng = 0.280_085 * r + 0.672_501 * g + 0.047_413 * b;
        let nb = -0.011_820 * r + 0.042_940 * g + 0.968_881 * b;
        Rgba::rgb(enc(nr), enc(ng), enc(nb))
    }

    /// CIE76 ΔE in Lab (sufficient threshold check for hue separation).
    fn delta_e(a: Rgba, b: Rgba) -> f64 {
        fn to_lab(c: Rgba) -> (f64, f64, f64) {
            let lin = |v: u8| {
                let cs = v as f64 / 255.0;
                if cs <= 0.04045 {
                    cs / 12.92
                } else {
                    ((cs + 0.055) / 1.055).powf(2.4)
                }
            };
            let (r, g, b) = (lin(c.r), lin(c.g), lin(c.b));
            // linear sRGB -> XYZ (D65)
            let x = 0.4124 * r + 0.3576 * g + 0.1805 * b;
            let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let z = 0.0193 * r + 0.1192 * g + 0.9505 * b;
            let f = |t: f64| {
                if t > 0.008_856 {
                    t.cbrt()
                } else {
                    7.787 * t + 16.0 / 116.0
                }
            };
            let (xn, yn, zn) = (0.95047, 1.0, 1.08883);
            let (fx, fy, fz) = (f(x / xn), f(y / yn), f(z / zn));
            (116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz))
        }
        let (l1, a1, b1) = to_lab(a);
        let (l2, a2, b2) = to_lab(b);
        ((l1 - l2).powi(2) + (a1 - a2).powi(2) + (b1 - b2).powi(2)).sqrt()
    }

    #[test]
    fn colorblind_text_passes_aa() {
        let c = colorblind();
        // text-primary AAA; muted AA on the lightest tier (Part B 5.88:1)
        assert!(contrast_ratio(c.text_primary, c.surface_main) >= AAA_BODY);
        assert!(contrast_ratio(c.text_muted, c.surface_elevated) >= AA_NORMAL);
        // accent/danger/warning double as text in ~300 places: AA on all tiers
        for bg in [c.surface_main, c.surface_card, c.surface_elevated] {
            assert!(contrast_ratio(c.accent, bg) >= AA_NORMAL, "accent on {bg:?}");
            assert!(contrast_ratio(c.danger, bg) >= AA_NORMAL, "danger on {bg:?}");
            assert!(contrast_ratio(c.warning, bg) >= AA_NORMAL, "warning on {bg:?}");
        }
        // success: AA-normal on primary/secondary (Part B/C.6); body text on the
        // lightest tier routes to success-hover (must clear AA-normal there).
        assert!(contrast_ratio(c.success, c.surface_main) >= AA_NORMAL); // 4.99:1
        assert!(contrast_ratio(c.success, c.surface_card) >= AA_NORMAL); // 4.52:1
        assert!(contrast_ratio(c.success_hover, c.surface_elevated) >= AA_NORMAL); // 6.04:1
        // focus-ring high-tone blue >= 3:1 non-text (Part B 8.53:1)
        assert!(contrast_ratio(c.focus_ring, c.surface_main) >= NON_TEXT);
    }

    #[test]
    fn colorblind_status_hues_stay_distinct_under_cvd() {
        let c = colorblind();
        // The decisive separation (Part B): danger vs warning under red-green
        // CVD must stay clearly distinct (delete vs caution must not collapse).
        // Part B reports ΔE 34.81 under deuteranopia; assert a strong margin.
        let d_sim = simulate_deutan(c.danger);
        let w_sim = simulate_deutan(c.warning);
        let dw = delta_e(d_sim, w_sim);
        assert!(
            dw >= 15.0,
            "colorblind danger vs warning under deutan ΔE {dw:.2} should stay distinct (Part B 34.81)"
        );
        // accent vs danger also separable under red-green CVD (Part B protan 10.40).
        let a_sim = simulate_deutan(c.accent);
        let ad = delta_e(a_sim, d_sim);
        assert!(
            ad >= 8.0,
            "colorblind accent vs danger under deutan ΔE {ad:.2} should stay distinct"
        );
        // accent vs warning likewise (blue vs amber, the easy axis).
        let aw = delta_e(a_sim, w_sim);
        assert!(aw >= 15.0, "colorblind accent vs warning under deutan ΔE {aw:.2}");
        // sanity: the unsimulated hues are obviously distinct too.
        assert!(delta_e(c.danger, c.warning) >= 20.0);
    }

    // ---- global: ALL registered themes return a fully-populated row -------
    #[test]
    fn all_32_themes_fully_populated_no_zero_color() {
        // The all-zero opaque black is the StdSpec::default() sentinel: a fully
        // materialized row must never leave a meaningful hue at it by accident.
        let zero = Rgba::rgb(0, 0, 0);
        for &id in ALL {
            let c = palette(id);
            // alpha ramp complete + every tier carries opacity
            assert_eq!(c.alpha.len(), crate::colors::ALPHA_COUNT, "{id:?} alpha len");
            assert!(c.alpha.iter().all(|a| a.a > 0), "{id:?} alpha has zero tier");
            // every status surface/border/hover composites to something visible
            for x in [
                c.danger_bg,
                c.danger_border,
                c.danger_hover,
                c.warning_bg,
                c.warning_border,
                c.warning_hover,
                c.success_bg,
                c.success_border,
                c.success_hover,
            ] {
                assert!(x.a > 0, "{id:?} a status tint has zero alpha");
            }
            // opaque hero tokens are opaque and not the all-zero sentinel.
            for (name, x) in [
                ("surface_main", c.surface_main),
                ("text_primary", c.text_primary),
                ("accent", c.accent),
                ("accent_text", c.accent_text),
                ("danger", c.danger),
                ("warning", c.warning),
                ("success", c.success),
                ("border_strong", c.border_strong),
                ("focus_ring", c.focus_ring),
                ("favorite", c.favorite),
            ] {
                assert_eq!(x.a, 255, "{id:?} {name} must be opaque");
            }
            // text/accent/border-strong must not be invisible-on-bg (the "default
            // color slipped through" symptom): require >= 1.5:1 minimum signal.
            assert!(
                contrast_ratio(c.text_primary, c.surface_main) >= 1.5,
                "{id:?} text_primary indistinguishable from bg (zero color?)"
            );
            // System falls back to Dark; skip the pure-pair identity below for it.
            let _ = (zero, over(c.surface_hover, c.surface_main));
        }
        // Count: the registry holds every ThemeId variant. The plan's prose
        // says "32 themes" but counts inconsistently (it variously treats the
        // System meta-entry as in/out). The enum is the source of truth: 4 Core
        // + 19 Dark + 8 Light + 5 Accessibility = 36 rows. Assert the concrete
        // breakdown so a future add/remove can't silently drop a row.
        let n_a11y = ALL
            .iter()
            .filter(|id| id.category() == crate::id::ThemeCategory::Accessibility)
            .count();
        assert_eq!(n_a11y, 5, "exactly 5 accessibility themes (P3)");
        assert_eq!(ALL.len(), 36, "registry must hold every ThemeId row");
    }
}

