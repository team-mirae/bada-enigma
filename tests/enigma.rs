//! bada enigma's behaviour, typed through `compat::host::Host`.

use enigma::compat::Report;
use enigma::compat::host::Host;
use enigma::{Enigma, Form, Settings, Space};

fn host() -> Host<Enigma> {
    Host::default()
}

#[test]
fn letters_are_enciphered() {
    let mut h = host();
    h.type_keys("a a a a a").unwrap();
    assert_eq!(h.preedit(), "BDZGO");
    assert_eq!(h.menu(), ["BDZGO", "bdzgo", "BDZGO"]);
    h.type_keys("Enter").unwrap();
    assert_eq!(h.text(), "BDZGO\n");
}

#[test]
fn every_session_starts_from_the_starting_state() {
    let mut h = host();
    h.type_keys("a a a Space a a a Space").unwrap();
    assert_eq!(h.text(), "BDZ BDZ ");
}

#[test]
fn backspace_unwinds_the_rotors() {
    let mut h = host();
    h.type_keys("a a a BS BS a").unwrap();
    assert_eq!(h.preedit(), "BD");
    h.type_keys("BS BS a a a a a").unwrap();
    assert_eq!(h.preedit(), "BDZGO");
}

#[test]
fn space_can_type_x() {
    let mut h = host();
    h.set_settings(Settings { space: Space::X, forms: vec![Form::Plain, Form::Groups], ..Settings::default() });
    h.type_keys("h e l l o Space w o r l d").unwrap();
    assert_eq!(h.preedit(), "helloxworld");
    h.type_keys("Tab").unwrap();
    assert_eq!(h.preedit().len(), 13, "groups of five: {}", h.preedit());
}

#[test]
fn it_deciphers_itself() {
    let set = Settings {
        rotors: vec!["IV".into(), "ii".into(), "VIII".into()],
        reflector: "c".into(),
        rings: "kzq".into(),
        start: "WTF".into(),
        plugboard: "ab cd ef".into(),
        ..Settings::default()
    };
    let mut h = host();
    h.set_settings(set.clone());
    h.type_keys("a t t a c k a t d a w n Enter").unwrap();
    let cipher = h.text().trim().to_lowercase();
    let mut h = host();
    h.set_settings(set);
    let keys: Vec<_> = cipher.chars().map(String::from).collect();
    h.type_keys(&keys.join(" ")).unwrap();
    assert_eq!(h.preedit(), "ATTACKATDAWN");
}

#[test]
fn settings_are_cleaned() {
    let mut h = host();
    let s = h.set_settings(Settings {
        rotors: vec!["I".into(), "I".into(), "III".into()],
        reflector: "Z".into(),
        rings: "AB".into(),
        start: "xyz".into(),
        plugboard: "ab, bc cd ee q fghi zy".into(),
        forms: vec![],
        ..Settings::default()
    });
    let d = Settings::default();
    assert_eq!((s.rotors, s.reflector, s.rings), (d.rotors, d.reflector, d.rings));
    assert_eq!(s.start, "XYZ");
    assert_eq!(s.plugboard, "AB CD ZY");
    assert_eq!(s.forms, d.forms);
}

#[test]
fn settings_mid_session_reencipher_it() {
    let mut h = host();
    h.type_keys("a a a").unwrap();
    h.set_settings(Settings { start: "ADU".into(), ..Settings::default() });
    let mut fresh = host();
    fresh.set_settings(Settings { start: "ADU".into(), ..Settings::default() });
    fresh.type_keys("a a a").unwrap();
    // The host shows the old preedit until the next key; what is staged has changed.
    assert_eq!(h.ime.staged(), fresh.preedit());
}

#[test]
fn zenkaku_enciphers_the_selection() {
    let mut h = host();
    h.type_keys("eisu").unwrap();
    h.set_text("Aa, a!");
    h.select(0, 6);
    h.type_keys("zenkaku").unwrap();
    assert_eq!(h.text(), "Bd, z!");
}
