//! The hash algorithms of the key hash (`docs/spec/equality.md` 7.2): `mix64`, the keys made from
//! the seed, and SipHash-1-3. Written from the published descriptions of the algorithms; the
//! values may change between versions of the implementation (hash values are never stored).

/// The constant mixed into a fixed random seed to make the hash seed
/// (`docs/spec/equality.md` 7.1), so that the hash seed differs from the random sequence's.
const FIXED_SEED_SALT: u64 = 0x5BD1_E995_5BD1_E995;

/// The constant mixed into the seed to make the second key `k1`.
const K1_SALT: u64 = 0x9E37_79B9_7F4A_7C15;

/// The bits a NaN key hashes as (`docs/spec/equality.md` 2.2).
const NAN_BITS: u64 = 0x7FF8_0000_0000_0000;

/// `mix64(x)` (`docs/spec/equality.md` 7.2): two xor-shift-multiply rounds and a final xor-shift,
/// modulo 2^64.
pub fn mix64(mut x: u64) -> u64 {
    x ^= x >> 30;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 27;
    x = x.wrapping_mul(0x94D0_49BB_1331_11EB);
    x ^ (x >> 31)
}

/// The hash seed for a run whose random seed is fixed to `r` (`PUY_RANDOM_SEED`, `puy test
/// --seed`; `docs/spec/equality.md` 7.1). The isolate's number is not mixed in: the seed is one per
/// process.
pub fn seed_from_fixed_random_seed(r: u64) -> u64 {
    mix64(r ^ FIXED_SEED_SALT)
}

/// The two 64-bit keys made from the seed (`docs/spec/equality.md` 7.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HashKeys {
    pub k0: u64,
    pub k1: u64,
}

impl HashKeys {
    pub fn new(seed: u64) -> HashKeys {
        HashKeys {
            k0: mix64(seed),
            k1: mix64(seed ^ K1_SALT),
        }
    }

    /// `HashInt`: the 64 bits of a fixed-size value.
    pub fn int(&self, v: u64) -> i64 {
        mix64(v ^ self.k0) as i64
    }

    /// `HashMix` / `HashCombine`: `h` with `x` mixed in (the order matters).
    pub fn mix(&self, h: u64, x: u64) -> i64 {
        mix64(mix64(h ^ self.k1).wrapping_add(x)) as i64
    }

    /// `HashF64`: the bits of `x` with `-0.0` made `0.0` and every NaN made one NaN, hashed as
    /// `HashInt`.
    pub fn f64(&self, x: f64) -> i64 {
        self.int(f64_key_bits(x))
    }

    /// `HashStr`: SipHash-1-3 of the code units, each as two bytes, low byte first
    /// (`docs/spec/equality.md` 2.3).
    pub fn units(&self, units: impl Iterator<Item = u16>) -> i64 {
        let mut s = Sip13::new(self.k0, self.k1);
        for u in units {
            s.write(&u.to_le_bytes());
        }
        s.finish() as i64
    }

    /// `HashUtf8`: SipHash-1-3 of the bytes of a `UTF8String` (`docs/spec/equality.md` 2.3).
    pub fn bytes(&self, bytes: &[u8]) -> i64 {
        let mut s = Sip13::new(self.k0, self.k1);
        s.write(bytes);
        s.finish() as i64
    }

    /// `HashSetBig`: the words of a big set without its trailing zero words, starting from
    /// `HashInt` of their number and mixing each in with `HashMix`, so that two sets `SetEq` takes
    /// as equal hash alike whatever the sizes of their blocks (`docs/spec/bytecode.md` 2.21).
    pub fn set_words(&self, words: &[i64]) -> i64 {
        let used = words.iter().rposition(|w| *w != 0).map_or(0, |i| i + 1);
        let words = &words[..used];
        words.iter().fold(self.int(words.len() as u64), |h, w| {
            self.mix(h as u64, *w as u64)
        })
    }
}

/// The bits a `Double` key hashes as (`docs/spec/equality.md` 2.2).
pub fn f64_key_bits(x: f64) -> u64 {
    if x.is_nan() {
        NAN_BITS
    } else if x == 0.0 {
        0
    } else {
        x.to_bits()
    }
}

/// `KeyEqF64`: two NaNs are equal; otherwise IEEE 754 `=` (`docs/spec/equality.md` 2.2).
pub fn key_eq_f64(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a == b
}

/// SipHash-1-3: one compression round per 8-byte block, three finalization rounds. Bytes are
/// written in any pieces; the result depends only on their sequence.
pub struct Sip13 {
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    /// The bytes of the block not yet complete, little-endian in the low `ntail` bytes.
    tail: u64,
    ntail: usize,
    /// The number of bytes written (its low 8 bits go in the last block).
    len: u64,
}

impl Sip13 {
    pub fn new(k0: u64, k1: u64) -> Sip13 {
        Sip13 {
            v0: k0 ^ 0x736f_6d65_7073_6575,
            v1: k1 ^ 0x646f_7261_6e64_6f6d,
            v2: k0 ^ 0x6c79_6765_6e65_7261,
            v3: k1 ^ 0x7465_6462_7974_6573,
            tail: 0,
            ntail: 0,
            len: 0,
        }
    }

    fn round(&mut self) {
        self.v0 = self.v0.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(13) ^ self.v0;
        self.v0 = self.v0.rotate_left(32);
        self.v2 = self.v2.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(16) ^ self.v2;
        self.v0 = self.v0.wrapping_add(self.v3);
        self.v3 = self.v3.rotate_left(21) ^ self.v0;
        self.v2 = self.v2.wrapping_add(self.v1);
        self.v1 = self.v1.rotate_left(17) ^ self.v2;
        self.v2 = self.v2.rotate_left(32);
    }

    fn compress(&mut self, m: u64) {
        self.v3 ^= m;
        self.round();
        self.v0 ^= m;
    }

    pub fn write(&mut self, bytes: &[u8]) {
        self.len = self.len.wrapping_add(bytes.len() as u64);
        for &b in bytes {
            self.tail |= u64::from(b) << (8 * self.ntail);
            self.ntail += 1;
            if self.ntail == 8 {
                let m = self.tail;
                self.compress(m);
                self.tail = 0;
                self.ntail = 0;
            }
        }
    }

    pub fn finish(mut self) -> u64 {
        let m = self.tail | ((self.len & 0xff) << 56);
        self.compress(m);
        self.v2 ^= 0xff;
        for _ in 0..3 {
            self.round();
        }
        self.v0 ^ self.v1 ^ self.v2 ^ self.v3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // SipHash-1-3 matches the published test vectors (the key 00..0f and the messages 00..(n-1),
    // as in the Rust standard library's tests), written in one piece and byte by byte.
    #[test]
    fn siphash13_test_vectors() {
        let k0 = 0x0706_0504_0302_0100;
        let k1 = 0x0f0e_0d0c_0b0a_0908;
        let vectors: [(usize, [u8; 8]); 6] = [
            (0, [0xdc, 0xc4, 0x0f, 0x05, 0x58, 0x01, 0xac, 0xab]),
            (1, [0x93, 0xca, 0x57, 0x7d, 0xf3, 0x9b, 0xf4, 0xc9]),
            (7, [0x40, 0x11, 0xb1, 0x9b, 0x98, 0x7d, 0x92, 0xd3]),
            (8, [0x8e, 0x9a, 0x29, 0x8d, 0x11, 0x95, 0x90, 0x36]),
            (15, [0x56, 0x99, 0x51, 0x2a, 0x6d, 0xd8, 0x20, 0xd3]),
            (63, [0xa8, 0xb3, 0xbb, 0xb7, 0x62, 0x90, 0x19, 0x9d]),
        ];
        for (n, expected) in vectors {
            let msg: Vec<u8> = (0..n as u8).collect();
            let mut whole = Sip13::new(k0, k1);
            whole.write(&msg);
            assert_eq!(whole.finish(), u64::from_le_bytes(expected), "{n}");
            let mut bytewise = Sip13::new(k0, k1);
            for b in &msg {
                bytewise.write(std::slice::from_ref(b));
            }
            assert_eq!(
                bytewise.finish(),
                u64::from_le_bytes(expected),
                "{n} bytewise"
            );
        }
    }

    // `mix64` and the keys follow the definitions of `docs/spec/equality.md` 7.2 and 7.1 (values
    // computed by hand from the definitions, outside this implementation).
    #[test]
    fn mix64_and_keys_follow_the_definitions() {
        assert_eq!(mix64(0), 0);
        assert_eq!(mix64(1), 0x5692_161D_100B_05E5);
        let keys = HashKeys::new(1);
        assert_eq!(keys.k0, 0x5692_161D_100B_05E5);
        assert_eq!(keys.k1, 0xE4D9_7177_1B65_2C20);
        assert_eq!(seed_from_fixed_random_seed(42), 0x4337_81C3_62D3_0A38);
    }
}
