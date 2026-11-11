//! The machine: an Enigma I / M3 with three rotors from I–VIII, reflector A, B or C, ring settings
//! and a plugboard. Letters are 0..26.

/// (name, wiring, turnover letters: the window letters at which the next rotor steps).
pub const ROTORS: [(&str, &str, &str); 8] = [
    ("I", "EKMFLGDQVZNTOWYHXUSPAIBRCJ", "Q"),
    ("II", "AJDKSIRUXBLHWTMCQGZNPYFVOE", "E"),
    ("III", "BDFHJLCPRTXVZNYEIWGAKMUSQO", "V"),
    ("IV", "ESOVPZJAYQUIRHXLNFTGKDCMWB", "J"),
    ("V", "VZBRGITYUPSDNHLXAWMJQOFECK", "Z"),
    ("VI", "JPGVOUMFYQBENHZRDKASXLICTW", "ZM"),
    ("VII", "NZJHGRCXMYSWBOUFAIVLPEKQDT", "ZM"),
    ("VIII", "FKQHTLXOCBJSPDZRAMEWNIUYGV", "ZM"),
];

/// (name, wiring).
pub const REFLECTORS: [(&str, &str); 3] =
    [("A", "EJMZALYXVBWFCRQUONTSPIKHGD"), ("B", "YRUHQSLDPXNGOKMIEBFZCWVJAT"), ("C", "FVPJIAOYEDRZXWGCTKUQSBNMHL")];

fn idx(c: u8) -> u8 {
    c - b'A'
}

#[derive(Clone, Debug)]
struct Rotor {
    forward: [u8; 26],
    backward: [u8; 26],
    notches: Vec<u8>,
    ring: u8,
    pos: u8,
}

impl Rotor {
    fn new(n: usize, ring: u8, pos: u8) -> Rotor {
        let (_, wiring, notches) = ROTORS[n];
        let mut forward = [0; 26];
        let mut backward = [0; 26];
        for (i, c) in wiring.bytes().enumerate() {
            forward[i] = idx(c);
            backward[idx(c) as usize] = i as u8;
        }
        Rotor { forward, backward, notches: notches.bytes().map(idx).collect(), ring, pos }
    }

    fn at_notch(&self) -> bool {
        self.notches.contains(&self.pos)
    }

    fn step(&mut self) {
        self.pos = (self.pos + 1) % 26;
    }

    fn through(&self, x: u8, map: &[u8; 26]) -> u8 {
        let shift = (self.pos + 26 - self.ring) % 26;
        (map[((x + shift) % 26) as usize] + 26 - shift) % 26
    }
}

/// One machine, set up and turned to its start.
#[derive(Clone, Debug)]
pub struct Machine {
    /// Left, middle, right.
    rotors: [Rotor; 3],
    reflector: [u8; 26],
    plugboard: [u8; 26],
}

impl Machine {
    /// `rotors`: indices into [`ROTORS`], left to right. `rings`, `start`: 0..26 each.
    pub fn new(rotors: [usize; 3], reflector: usize, rings: [u8; 3], start: [u8; 3], pairs: &[(u8, u8)]) -> Machine {
        let mut plugboard: [u8; 26] = std::array::from_fn(|i| i as u8);
        for &(a, b) in pairs {
            plugboard[a as usize] = b;
            plugboard[b as usize] = a;
        }
        let mut refl = [0; 26];
        for (i, c) in REFLECTORS[reflector].1.bytes().enumerate() {
            refl[i] = idx(c);
        }
        Machine {
            rotors: std::array::from_fn(|i| Rotor::new(rotors[i], rings[i], start[i])),
            reflector: refl,
            plugboard,
        }
    }

    /// The letters in the windows, left to right.
    pub fn window(&self) -> String {
        self.rotors.iter().map(|r| (b'A' + r.pos) as char).collect()
    }

    /// Steps (with the middle rotor's double step), then enciphers one letter.
    pub fn press(&mut self, x: u8) -> u8 {
        let [l, m, r] = &mut self.rotors;
        if m.at_notch() {
            m.step();
            l.step();
        } else if r.at_notch() {
            m.step();
        }
        r.step();
        let mut x = self.plugboard[x as usize];
        for rotor in self.rotors.iter().rev() {
            x = rotor.through(x, &rotor.forward);
        }
        x = self.reflector[x as usize];
        for rotor in &self.rotors {
            x = rotor.through(x, &rotor.backward);
        }
        self.plugboard[x as usize]
    }

    /// Enciphers a letter, keeping its case; anything else passes as is, without stepping.
    pub fn press_char(&mut self, c: char) -> char {
        if !c.is_ascii_alphabetic() {
            return c;
        }
        let out = (b'A' + self.press(idx(c.to_ascii_uppercase() as u8))) as char;
        if c.is_ascii_lowercase() { out.to_ascii_lowercase() } else { out }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m3(rotors: [usize; 3], rings: &str, start: &str, plugs: &str) -> Machine {
        let l = |s: &str| -> [u8; 3] { std::array::from_fn(|i| idx(s.as_bytes()[i])) };
        let pairs: Vec<_> = plugs.split_whitespace().map(|p| (idx(p.as_bytes()[0]), idx(p.as_bytes()[1]))).collect();
        Machine::new(rotors, 1, l(rings), l(start), &pairs)
    }

    fn run(m: &mut Machine, text: &str) -> String {
        text.chars().map(|c| m.press_char(c)).collect()
    }

    #[test]
    fn the_textbook_case() {
        // I-II-III, B, rings AAA, start AAA: AAAAA → BDZGO.
        assert_eq!(run(&mut m3([0, 1, 2], "AAA", "AAA", ""), "AAAAA"), "BDZGO");
    }

    #[test]
    fn double_step() {
        let mut m = m3([0, 1, 2], "AAA", "ADU", "");
        let windows: Vec<_> = (0..3).map(|_| (m.press(0), m.window()).1).collect();
        assert_eq!(windows, ["ADV", "AEW", "BFX"]);
    }

    #[test]
    fn rings_and_plugs() {
        // A widely published check: I-II-III, B, rings BBB, start AAA, plugs AB → no letter maps to itself,
        // and the same setup deciphers what it enciphers.
        let plain = "THEQUICKBROWNFOXJUMPSOVERTHELAZYDOG";
        let cipher = run(&mut m3([0, 1, 2], "BBB", "AAA", "AB CD EF"), plain);
        assert!(cipher.chars().zip(plain.chars()).all(|(a, b)| a != b));
        assert_eq!(run(&mut m3([0, 1, 2], "BBB", "AAA", "AB CD EF"), &cipher), plain);
    }

    #[test]
    fn operation_barbarossa() {
        // The Barbarossa message (1941), part 1: II IV V, B, rings BUL, plugs AV BS CG DL FU HZ IN KM OW RX,
        // start BLA.
        let mut m = m3([1, 3, 4], "BUL", "BLA", "AV BS CG DL FU HZ IN KM OW RX");
        let cipher = "EDPUDNRGYSZRCXNUYTPOMRMBOFKTBZREZKMLXLVEFGUEYSIOZVEQMIKUBPMMYLKLTTDEISMDICAGYKUACTCDOMOHWXMUUIAUBSTSLRNBZSZWNRFXWFYSSXJZVIJHIDISHPRKLKAYUPADTXQSPINQMATLPIFSVKDASCTACDPBOPVHJK";
        assert!(run(&mut m, cipher).starts_with(
            "AUFKLXABTEILUNGXVONXKURTINOWAXKURTINOWAXNORDWESTLXSEBEZXSEBEZXUAFFLIEGERSTRASZERIQTUNGXDUBROWKIXDUBROWKI"
        ));
    }
}
