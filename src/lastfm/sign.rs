const SHIFT: [u32; 64] = [
    7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22,
    5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20,
    4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
    6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
];

const NOISE: [u32; 64] = [
    0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee,
    0xf57c0faf, 0x4787c62a, 0xa8304613, 0xfd469501,
    0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be,
    0x6b901122, 0xfd987193, 0xa679438e, 0x49b40821,
    0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa,
    0xd62f105d, 0x02441453, 0xd8a1e681, 0xe7d3fbc8,
    0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
    0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a,
    0xfffa3942, 0x8771f681, 0x6d9d6122, 0xfde5380c,
    0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70,
    0x289b7ec6, 0xeaa127fa, 0xd4ef3085, 0x04881d05,
    0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665,
    0xf4292244, 0x432aff97, 0xab9423a7, 0xfc93a039,
    0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
    0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1,
    0xf7537e82, 0xbd3af235, 0x2ad7d2bb, 0xeb86d391,
];

fn digest(input: &[u8]) -> [u8; 16] {
    let mut state: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];

    let mut message = input.to_vec();
    let bits = (input.len() as u64).wrapping_mul(8);
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bits.to_le_bytes());

    for block in message.as_chunks::<64>().0 {
        let mut words = [0u32; 16];
        for (i, word) in words.iter_mut().enumerate() {
            let at = i * 4;
            *word = u32::from_le_bytes([
                block[at],
                block[at + 1],
                block[at + 2],
                block[at + 3],
            ]);
        }

        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (mixed, which) = match i {
                0..=15 => ((b & c) | (!b & d), i),
                16..=31 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                32..=47 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let sum = mixed
                .wrapping_add(a)
                .wrapping_add(NOISE[i])
                .wrapping_add(words[which]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(sum.rotate_left(SHIFT[i]));
        }

        state[0] = state[0].wrapping_add(a);
        state[1] = state[1].wrapping_add(b);
        state[2] = state[2].wrapping_add(c);
        state[3] = state[3].wrapping_add(d);
    }

    let mut out = [0u8; 16];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

pub fn md5_hex(input: &[u8]) -> String {
    digest(input)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn api_sig(params: &[(String, String)], secret: &str) -> String {
    let mut sorted: Vec<&(String, String)> = params
        .iter()
        .filter(|(name, _)| name != "format" && name != "callback" && name != "api_sig")
        .collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));

    let mut joined = String::new();
    for (name, value) in sorted {
        joined.push_str(name);
        joined.push_str(value);
    }
    joined.push_str(secret);
    md5_hex(joined.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_published_test_vectors_come_out() {
        let cases: &[(&str, &str)] = &[
            ("", "d41d8cd98f00b204e9800998ecf8427e"),
            ("a", "0cc175b9c0f1b6a831c399e269772661"),
            ("abc", "900150983cd24fb0d6963f7d28e17f72"),
            ("message digest", "f96b697d7cb7938d525a2f31aaf161d0"),
            (
                "abcdefghijklmnopqrstuvwxyz",
                "c3fcd3d76192e4007dfb496cca67e13b",
            ),
            (
                "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789",
                "d174ab98d277d9f5a5611c2c9f419d9f",
            ),
            (
                "12345678901234567890123456789012345678901234567890123456789012345678901234567890",
                "57edf4a22be3c955ac49da2e2107b67a",
            ),
        ];
        for (input, expected) in cases {
            assert_eq!(md5_hex(input.as_bytes()), *expected, "md5({input:?})");
        }
    }

    #[test]
    fn it_survives_the_block_boundary() {
        for length in [54, 55, 56, 57, 63, 64, 65, 119, 120, 128] {
            let input = vec![b'x'; length];
            let out = md5_hex(&input);
            assert_eq!(out.len(), 32, "length {length}");
            assert!(out.chars().all(|c| c.is_ascii_hexdigit()));
        }
        assert_eq!(md5_hex(&[b'x'; 56]), "668a72d5ba17f08e62dabcafad6db14b");
        assert_eq!(md5_hex(&[b'x'; 64]), "c1bb4f81d892b2d57947682aeb252456");
    }

    #[test]
    fn a_signature_is_the_sorted_pairs_plus_the_secret() {
        let params = vec![
            ("method".to_string(), "auth.getSession".to_string()),
            ("api_key".to_string(), "KEY".to_string()),
            ("token".to_string(), "TOKEN".to_string()),
        ];
        let expected = md5_hex(b"api_keyKEYmethodauth.getSessiontokenTOKENSECRET");
        assert_eq!(api_sig(&params, "SECRET"), expected);
    }

    #[test]
    fn format_is_left_out_of_the_signature() {
        let without = vec![("method".to_string(), "track.scrobble".to_string())];
        let mut with = without.clone();
        with.push(("format".to_string(), "json".to_string()));
        with.push(("api_sig".to_string(), "whatever".to_string()));
        assert_eq!(api_sig(&without, "s"), api_sig(&with, "s"));
    }

    #[test]
    fn values_are_signed_exactly_as_they_are() {
        let params = vec![("track".to_string(), "tha".to_string())];
        let spaced = vec![("track".to_string(), "xtal two".to_string())];
        assert_eq!(api_sig(&params, "s"), md5_hex(b"trackthas"));
        assert_eq!(api_sig(&spaced, "s"), md5_hex(b"trackxtal twos"));
    }

    #[test]
    fn a_signature_is_taken_over_utf8() {
        let params = vec![("artist".to_string(), "坂本龍一".to_string())];
        let mut joined = String::from("artist坂本龍一");
        joined.push('s');
        assert_eq!(api_sig(&params, "s"), md5_hex(joined.as_bytes()));
    }
}
