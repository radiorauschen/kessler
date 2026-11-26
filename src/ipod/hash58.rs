//! The signature the iPod classic and the nano of the third and fourth
//! generation want on their database (`hash58`, after where it lives in the
//! header: 0x58).
//!
//! Without it those iPods show an empty library, however good the database
//! is. The scheme is the one libgpod uses (`itdb_hash58.c`):
//!
//! 1. A key from the device's FireWire GUID: the least common multiple of
//!    each pair of its first eight bytes, both bytes of it looked up in the
//!    AES S-box and in its inverse, sixteen bytes in all, hashed with SHA-1
//!    after eighteen fixed bytes.
//! 2. An HMAC-SHA1 with that key over the whole database, written with the
//!    database's own number (0x18), the twenty bytes at 0x32 and the hash
//!    itself set to nought, and the hashing scheme at 0x30 set to one.
//! 3. The twenty bytes of it at 0x58, and the number and the bytes at 0x32
//!    put back.
//!
//! Each iPod has its own GUID, so a database signed for one is unsigned for
//! any other.

use sha1_smol::Sha1;

/// The eighteen bytes hashed before the device's own to make the key.
const FIXED: [u8; 18] = [
    0x67, 0x23, 0xFE, 0x30, 0x45, 0x33, 0xF8, 0x90, 0x99, 0x21, 0x07, 0xC1, 0xD0, 0x12, 0xB2, 0xA1, 0x07, 0x81,
];

/// Where the pieces of the header are that the signature is about.
const DB_ID: usize = 0x18;
const SCHEME: usize = 0x30;
const UNKNOWN_32: usize = 0x32;
pub const HASH: usize = 0x58;
/// The shortest header the signature fits into.
pub const SHORTEST_HEADER: usize = HASH + 20;

/// A device's FireWire GUID as written in its `SysInfo`, `SysInfoExtended`
/// or as its USB serial number: sixteen hexadecimal digits, sometimes after
/// `0x`. Anything else — shorter, longer, not hexadecimal, all noughts — is
/// not one, and nothing is signed with it.
pub fn parse_guid(text: &str) -> Option<[u8; 8]> {
    let digits = text.trim();
    let digits = digits.strip_prefix("0x").or_else(|| digits.strip_prefix("0X")).unwrap_or(digits);
    if digits.len() != 16 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let mut guid = [0u8; 8];
    for (k, byte) in guid.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digits[k * 2..k * 2 + 2], 16).ok()?;
    }
    (guid != [0; 8]).then_some(guid)
}

/// Sign a whole written database in place for the device with this GUID.
///
/// The header has to be long enough to hold the signature — see
/// [`SHORTEST_HEADER`]; the writer makes it so for every device that is
/// signed for.
pub fn sign(database: &mut [u8], guid: &[u8; 8]) {
    assert!(
        database.len() >= SHORTEST_HEADER && &database[..4] == b"mhbd",
        "a database to sign starts with a header long enough for the signature"
    );
    let mut db_id = [0u8; 8];
    db_id.copy_from_slice(&database[DB_ID..DB_ID + 8]);
    let mut unknown = [0u8; 20];
    unknown.copy_from_slice(&database[UNKNOWN_32..UNKNOWN_32 + 20]);

    database[DB_ID..DB_ID + 8].fill(0);
    database[UNKNOWN_32..UNKNOWN_32 + 20].fill(0);
    database[HASH..HASH + 20].fill(0);
    database[SCHEME..SCHEME + 2].copy_from_slice(&1u16.to_le_bytes());

    let hash = hmac(&key(guid), database);
    database[HASH..HASH + 20].copy_from_slice(&hash);
    database[DB_ID..DB_ID + 8].copy_from_slice(&db_id);
    database[UNKNOWN_32..UNKNOWN_32 + 20].copy_from_slice(&unknown);
}

/// The key for this device: sixty-four bytes, the first twenty of them a
/// SHA-1, the rest nought, as the HMAC wants a block.
fn key(guid: &[u8; 8]) -> [u8; 64] {
    let (forward, inverse) = s_boxes();
    let mut y = [0u8; 16];
    for i in 0..4 {
        let both = lcm(u32::from(guid[i * 2]), u32::from(guid[i * 2 + 1]));
        let hi = ((both >> 8) & 0xFF) as usize;
        let lo = (both & 0xFF) as usize;
        y[i * 4] = forward[hi];
        y[i * 4 + 1] = inverse[hi];
        y[i * 4 + 2] = forward[lo];
        y[i * 4 + 3] = inverse[lo];
    }
    let mut sha = Sha1::new();
    sha.update(&FIXED);
    sha.update(&y);
    let mut key = [0u8; 64];
    key[..20].copy_from_slice(&sha.digest().bytes());
    key
}

/// The least common multiple, and one where either is nought — the way the
/// scheme was first written, which the iPods check against.
fn lcm(a: u32, b: u32) -> u32 {
    if a == 0 || b == 0 {
        return 1;
    }
    let (mut x, mut y) = (a, b);
    while y != 0 {
        (x, y) = (y, x % y);
    }
    a * b / x
}

/// HMAC-SHA1 with a key already a block long.
fn hmac(key: &[u8; 64], message: &[u8]) -> [u8; 20] {
    let pad = |with: u8| key.map(|k| k ^ with);
    let mut inner = Sha1::new();
    inner.update(&pad(0x36));
    inner.update(message);
    let mut outer = Sha1::new();
    outer.update(&pad(0x5C));
    outer.update(&inner.digest().bytes());
    outer.digest().bytes()
}

/// The AES S-box and its inverse, worked out rather than written down: the
/// multiplicative inverse in GF(2⁸), then the affine map. Two tables of 256
/// bytes copied by hand are two chances of a wrong byte nobody would see.
fn s_boxes() -> ([u8; 256], [u8; 256]) {
    let mut forward = [0u8; 256];
    let mut inverse = [0u8; 256];
    let (mut p, mut q) = (1u8, 1u8);
    loop {
        p ^= (p << 1) ^ if p & 0x80 != 0 { 0x1B } else { 0 };
        q ^= q << 1;
        q ^= q << 2;
        q ^= q << 4;
        if q & 0x80 != 0 {
            q ^= 0x09;
        }
        let s = q ^ q.rotate_left(1) ^ q.rotate_left(2) ^ q.rotate_left(3) ^ q.rotate_left(4) ^ 0x63;
        forward[usize::from(p)] = s;
        inverse[usize::from(s)] = p;
        if p == 1 {
            break;
        }
    }
    forward[0] = 0x63;
    inverse[0x63] = 0;
    (forward, inverse)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first and last rows of both tables as libgpod prints them.
    #[test]
    fn the_tables_are_the_aes_ones() {
        let (forward, inverse) = s_boxes();
        assert_eq!(&forward[..8], &[0x63, 0x7C, 0x77, 0x7B, 0xF2, 0x6B, 0x6F, 0xC5]);
        assert_eq!(&forward[248..], &[0x41, 0x99, 0x2D, 0x0F, 0xB0, 0x54, 0xBB, 0x16]);
        assert_eq!(&inverse[..8], &[0x52, 0x09, 0x6A, 0xD5, 0x30, 0x36, 0xA5, 0x38]);
        assert_eq!(&inverse[248..], &[0xE1, 0x69, 0x14, 0x63, 0x55, 0x21, 0x0C, 0x7D]);
        for b in 0..=255u8 {
            assert_eq!(inverse[usize::from(forward[usize::from(b)])], b);
        }
    }

    #[test]
    fn a_guid_is_sixteen_hex_digits() {
        assert_eq!(parse_guid("000A27001CE1B2D3"), Some([0x00, 0x0A, 0x27, 0x00, 0x1C, 0xE1, 0xB2, 0xD3]));
        assert_eq!(parse_guid(" 0x000a27001ce1b2d3\n"), Some([0x00, 0x0A, 0x27, 0x00, 0x1C, 0xE1, 0xB2, 0xD3]));
        assert_eq!(parse_guid("000A27001CE1B2"), None);
        assert_eq!(parse_guid("000A27001CE1B2D3FF"), None);
        assert_eq!(parse_guid("000A27001CE1B2DZ"), None);
        assert_eq!(parse_guid("0000000000000000"), None);
    }

    #[test]
    fn the_least_common_multiple_as_the_scheme_has_it() {
        assert_eq!(lcm(4, 6), 12);
        assert_eq!(lcm(0x0A, 0x27), 390);
        assert_eq!(lcm(0, 0x27), 1);
        assert_eq!(lcm(0xFF, 0xFE), 0xFF * 0xFE);
    }

    /// HMAC-SHA1 against RFC 2202's second case, the key padded to a block
    /// as the scheme's is.
    #[test]
    fn the_hmac_is_hmac_sha1() {
        let mut key = [0u8; 64];
        key[..4].copy_from_slice(b"Jefe");
        let mac = hmac(&key, b"what do ya want for nothing?");
        let hex: String = mac.iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "effcdf6ae5eb2fa2d27416d5f184df9c259a7c79");
    }

    /// Signing changes nothing but the scheme and the hash, and the same
    /// database signed for another device comes out differently.
    #[test]
    fn signing_touches_only_its_own_bytes() {
        let mut db = vec![0u8; 0xF4 + 40];
        db[..4].copy_from_slice(b"mhbd");
        db[4..8].copy_from_slice(&0xF4u32.to_le_bytes());
        db[DB_ID..DB_ID + 8].copy_from_slice(&[9; 8]);
        db[UNKNOWN_32..UNKNOWN_32 + 20].copy_from_slice(&[7; 20]);
        for (k, b) in db.iter_mut().enumerate().skip(0xF4) {
            *b = k as u8;
        }
        let before = db.clone();
        let guid = parse_guid("000A27001CE1B2D3").unwrap();
        sign(&mut db, &guid);
        for (k, (a, b)) in before.iter().zip(&db).enumerate() {
            if !(SCHEME..SCHEME + 2).contains(&k) && !(HASH..HASH + 20).contains(&k) {
                assert_eq!(a, b, "byte {k:#x} changed");
            }
        }
        assert_eq!(&db[SCHEME..SCHEME + 2], &[1, 0]);
        assert!(db[HASH..HASH + 20].iter().any(|b| *b != 0));

        let mut again = before.clone();
        sign(&mut again, &guid);
        assert_eq!(again, db, "the same device, the same signature");
        let mut other = before;
        sign(&mut other, &parse_guid("000A27001CE1B2D4").unwrap());
        assert_ne!(other[HASH..HASH + 20], db[HASH..HASH + 20]);
    }
}
