//! End-to-end CLI checks. These run the real binary so argument parsing,
//! recipe loading, rendering and file output stay covered.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_analog-cli");

fn run(args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to run {BIN}: {e}"))
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("analog-cli-tests-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_test_image(path: &Path) {
    let data: Vec<u8> = (0..64 * 48 * 4)
        .map(|i| match i % 4 {
            3 => 255,
            c => ((i * 7 + c * 41) % 256) as u8,
        })
        .collect();
    analog_core::Image::from_rgba8(64, 48, data)
        .unwrap()
        .encode(path)
        .unwrap();
}

#[test]
fn cli_end_to_end() {
    let dir = temp_dir("end-to-end");
    let input = dir.join("in.png");
    let output = dir.join("out.png");
    write_test_image(&input);

    let info = run(&["info", input.to_str().unwrap()]);
    assert!(info.status.success(), "info failed: {info:?}");
    let text = String::from_utf8_lossy(&info.stdout);
    assert!(text.contains("64x48"), "unexpected info output: {text}");

    let preset_json = dir.join("recipe.json");
    let preset_xmp = dir.join("recipe.xmp");
    let preset = run(&[
        "preset",
        preset_xmp.to_str().unwrap(),
        preset_json.to_str().unwrap(),
    ]);
    // No input preset exists yet: this must fail cleanly.
    assert!(!preset.status.success());

    let gpu = run(&["gpu-info"]);
    if !gpu.status.success() {
        eprintln!("skipping render check: no GPU");
        return;
    }

    let render = run(&[
        "render",
        input.to_str().unwrap(),
        output.to_str().unwrap(),
        "--grain",
        "40",
        "--seed",
        "7",
    ]);
    assert!(render.status.success(), "render failed: {render:?}");
    let rendered = analog_core::Image::decode(&output).unwrap();
    assert_eq!((rendered.width(), rendered.height()), (64, 48));

    // An XMP recipe can be loaded on the next run.
    let recipe_path = dir.join("grain.xmp");
    let recipe = analog_core::Recipe {
        grain_seed: 7,
        grain: analog_core::GrainParams {
            intensity: 40.0,
            ..Default::default()
        },
        ..Default::default()
    };
    std::fs::write(&recipe_path, analog_core::to_recipe_xmp(&recipe)).unwrap();
    let json_path = dir.join("grain.json");
    let preset_ok = run(&[
        "preset",
        recipe_path.to_str().unwrap(),
        json_path.to_str().unwrap(),
    ]);
    assert!(preset_ok.status.success(), "preset failed: {preset_ok:?}");
    let json = std::fs::read_to_string(&json_path).unwrap();
    assert!(json.contains("\"grain\""), "unexpected JSON: {json}");
    let back = dir.join("back.xmp");
    let preset_back = run(&[
        "preset",
        json_path.to_str().unwrap(),
        back.to_str().unwrap(),
    ]);
    assert!(
        preset_back.status.success(),
        "preset back failed: {preset_back:?}"
    );
    assert!(analog_core::from_recipe_xmp(&std::fs::read_to_string(&back).unwrap()).is_ok());

    let output2 = dir.join("out2.png");
    let render2 = run(&[
        "render",
        input.to_str().unwrap(),
        output2.to_str().unwrap(),
        "--recipe",
        recipe_path.to_str().unwrap(),
    ]);
    assert!(
        render2.status.success(),
        "recipe render failed: {render2:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn missing_input_reports_an_error() {
    let dir = temp_dir("missing");
    let missing = dir.join("does-not-exist.png");
    let out = run(&["info", missing.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(!out.stderr.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
