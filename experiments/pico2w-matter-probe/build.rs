use std::{env, fs, path::PathBuf};
use verhoeff::Verhoeff;

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::write(out.join("memory.x"), include_bytes!("../../memory.x")).unwrap();
    println!("cargo:rustc-link-search={}", out.display());
    println!("cargo:rustc-link-arg-bins=--nmagic");
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rerun-if-changed=../../memory.x");
    println!("cargo:rerun-if-changed=probe.local.toml");
    println!("cargo:rerun-if-changed=controller-seed.local.bin");

    // Parse errors must not echo the local settings or credentials.
    let source = fs::read_to_string("probe.local.toml").unwrap_or_default();
    let settings: toml::Table = source
        .parse()
        .unwrap_or_else(|_| panic!("Invalid probe.local.toml. Check TOML syntax locally."));
    let field = |section: &str, name: &str| {
        settings
            .get(section)
            .and_then(|v| v.get(name))
            .and_then(|v| v.as_str())
            .unwrap_or("")
    };
    let ssid = field("wifi", "ssid");
    let password = field("wifi", "password");
    let code: String = field("matter", "setup_code")
        .chars()
        .filter(|c| *c != '-' && !c.is_whitespace())
        .collect();
    let configured = !ssid.is_empty() && !code.is_empty();
    let (passcode, short_discriminator) = if configured {
        assert!(ssid.len() <= 32, "Wi-Fi SSID is too long.");
        assert!(
            (8..=63).contains(&password.len()),
            "Wi-Fi WPA2 passphrase must be 8 to 63 bytes."
        );
        assert!(
            code.len() == 11 && code.bytes().all(|c| c.is_ascii_digit()),
            "Enter the 11-digit Matter manual setup code locally."
        );
        assert!(
            code.validate_verhoeff_check_digit(),
            "Matter setup-code checksum is invalid."
        );
        let c1: u32 = code[..1].parse().unwrap();
        let c2: u32 = code[1..6].parse().unwrap();
        let c3: u32 = code[6..10].parse().unwrap();
        assert!(
            c1 <= 3 && c2 <= 65535 && c3 <= 8191,
            "Invalid Matter setup-code fields."
        );
        let passcode = (c2 & 0x3fff) | (c3 << 14);
        assert!(
            (1..=99999998).contains(&passcode)
                && ![
                    11111111, 22222222, 33333333, 44444444, 55555555, 66666666, 77777777, 88888888,
                    12345678, 87654321
                ]
                .contains(&passcode),
            "Invalid Matter passcode."
        );
        (passcode, ((c1 << 2) | (c2 >> 14)) as u8)
    } else {
        (0, 0)
    };

    // Preserve this private seed across builds to reuse the existing fabric.
    let seed_path = PathBuf::from("controller-seed.local.bin");
    if !seed_path.exists() {
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).expect("Cannot generate controller identity seed.");
        fs::write(&seed_path, seed).expect("Cannot save controller identity seed.");
    }
    let seed = fs::read(&seed_path).expect("Cannot read controller identity seed.");
    assert!(
        seed.len() == 32,
        "Controller identity seed must be 32 bytes."
    );
    let config = format!(
        "pub const CONFIGURED: bool = {configured};\npub const WIFI_SSID: &str = {ssid:?};\npub const WIFI_PASSWORD: &str = {password:?};\npub const PASSCODE: u32 = {passcode};\npub const SHORT_DISCRIMINATOR: u8 = {short_discriminator};\npub const CONTROLLER_SEED: [u8;32] = {seed:?};\n"
    );
    fs::write(out.join("probe_config.rs"), config).unwrap();
}
