use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generation {
    First,
    Second,
    Third,
    Fourth,
    Photo,
    Mini1,
    Mini2,
    Nano1,
    Nano2,
    Nano3,
    Nano4,
    Video1,
    Video2,
    Shuffle1,
    Shuffle2,
    Shuffle3,
    Shuffle4,
    Classic1,
    Classic2,
    Classic3,
    Unknown,
}

impl Generation {
    pub fn label(self) -> &'static str {
        match self {
            Self::First => "iPod 1st generation",
            Self::Second => "iPod 2nd generation",
            Self::Third => "iPod 3rd generation (2003, touch buttons)",
            Self::Fourth => "iPod 4th generation",
            Self::Photo => "iPod photo",
            Self::Mini1 => "iPod mini 1st generation",
            Self::Mini2 => "iPod mini 2nd generation",
            Self::Nano1 => "iPod nano 1st generation",
            Self::Nano2 => "iPod nano 2nd generation",
            Self::Nano3 => "iPod nano 3rd generation",
            Self::Nano4 => "iPod nano 4th generation",
            Self::Video1 => "iPod 5th generation (video)",
            Self::Video2 => "iPod 5.5th generation (video)",
            Self::Shuffle1 => "iPod shuffle 1st generation",
            Self::Shuffle2 => "iPod shuffle 2nd generation",
            Self::Shuffle3 => "iPod shuffle 3rd generation",
            Self::Shuffle4 => "iPod shuffle 4th generation",
            Self::Classic1 => "iPod classic (2007, 6th generation)",
            Self::Classic2 => "iPod classic 120GB (2008)",
            Self::Classic3 => "iPod classic 160GB (2009, 7th generation)",
            Self::Unknown => "unknown iPod",
        }
    }

    pub fn checksum(self) -> Checksum {
        match self {
            Self::First
            | Self::Second
            | Self::Third
            | Self::Fourth
            | Self::Photo
            | Self::Mini1
            | Self::Mini2
            | Self::Nano1
            | Self::Nano2
            | Self::Video1
            | Self::Video2
            | Self::Shuffle1
            | Self::Shuffle2
            | Self::Shuffle3
            | Self::Shuffle4 => Checksum::None,

            Self::Classic1 | Self::Classic2 | Self::Classic3 | Self::Nano3 | Self::Nano4 => {
                Checksum::Hash58
            }

            Self::Unknown => Checksum::Unknown,
        }
    }

    pub fn playable_formats(self) -> &'static [&'static str] {
        match self {
            Self::First | Self::Second => &["mp3", "wav", "aif", "aiff"],
            Self::Third => &["mp3", "m4a", "aac", "wav", "aif", "aiff"],
            Self::Shuffle1 | Self::Shuffle2 | Self::Shuffle3 | Self::Shuffle4 => &["mp3", "m4a", "aac", "wav"],
            Self::Unknown => &["mp3"],
            _ => &["mp3", "m4a", "aac", "m4b", "wav", "aif", "aiff", "alac"],
        }
    }

    pub fn is_shuffle(self) -> bool {
        matches!(self, Self::Shuffle1 | Self::Shuffle2 | Self::Shuffle3 | Self::Shuffle4)
    }

    pub fn has_playlists(self) -> bool {
        !matches!(self, Self::Shuffle1 | Self::Shuffle2)
    }

    pub fn plays(self, extension: &str) -> bool {
        let ext = extension.trim_start_matches('.').to_ascii_lowercase();
        self.playable_formats().contains(&ext.as_str())
    }

    pub fn music_folders(self) -> u32 {
        match self {
            Self::First | Self::Second => 20,
            Self::Third | Self::Fourth | Self::Photo | Self::Video1 | Self::Video2 => 50,
            Self::Classic1 | Self::Classic2 | Self::Classic3 => 50,
            Self::Mini1 | Self::Mini2 => 20,
            Self::Nano1 | Self::Nano2 => 6,
            Self::Nano3 | Self::Nano4 => 14,
            Self::Shuffle1 | Self::Shuffle2 | Self::Shuffle3 | Self::Shuffle4 => 3,
            Self::Unknown => 20,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checksum {
    None,
    Hash58,
    Unknown,
}

impl Checksum {
    #[cfg(test)]
    pub fn is_supported_for_writing(self) -> bool {
        matches!(self, Self::None)
    }
}

fn generation_for_model(model: &str) -> Generation {
    use Generation::*;
    match model {
        "8513" | "8541" | "8697" | "8709" => First,
        "8737" | "8740" | "8738" | "8741" => Second,
        "8976" | "8946" | "9460" | "9244" | "8948" | "9245" => Third,
        "9282" | "9787" | "9268" | "E436" => Fourth,
        "A079" | "A127" | "9829" | "9585" | "9830" | "9586" | "S492" => Photo,
        "9160" | "9436" | "9435" | "9434" | "9437" => Mini1,
        "9800" | "9802" | "9804" | "9806" | "9801" | "9803" | "9805" | "9807" => Mini2,
        "A350" | "A352" | "A004" | "A099" | "A005" | "A107" => Nano1,
        "A477" | "A426" | "A428" | "A487" | "A489" | "A725" | "A726" | "A497" => Nano2,
        "A978" | "A980" | "B261" | "B249" | "B253" | "B257" => Nano3,
        "B480" | "B651" | "B654" | "B657" | "B660" | "B663" | "B666" | "B598" | "B732"
        | "B735" => Nano4,
        "A002" | "A146" | "A003" | "A147" | "A452" => Video1,
        "A444" | "A446" | "A664" | "A448" | "A450" => Video2,
        "9724" | "9725" => Shuffle1,
        "A546" | "A947" | "A949" | "A951" | "A953" | "C167" | "B225" | "B233" | "B231"
        | "B227" | "B228" | "B229" | "B518" | "B520" | "B522" | "B524" | "B526" => Shuffle2,
        "C306" | "C323" | "C381" | "C384" | "C387" | "B867" | "C164" | "C303" | "C307"
        | "C328" | "C331" => Shuffle3,
        "C584" | "C585" | "C749" | "C750" | "C751" | "D773" | "D774" | "D775" | "D776"
        | "D777" | "D778" | "D779" | "D780" | "E949" | "KM72" | "KM92" | "KME2" | "KMG2"
        | "KMJ2" | "KML2" => Shuffle4,
        "B029" | "B147" | "B145" | "B150" => Classic1,
        "B562" | "B565" => Classic2,
        "C293" | "C297" => Classic3,
        _ => Generation::Unknown,
    }
}

#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub generation: Generation,
    pub model_number: Option<String>,
    pub serial: Option<String>,
    #[allow(dead_code)]
    pub firewire_id: Option<String>,
}

impl DeviceInfo {
    pub fn read(mount_point: &Path) -> Self {
        let device_dir = mount_point.join("iPod_Control").join("Device");

        let sysinfo = std::fs::read_to_string(device_dir.join("SysInfo")).ok();
        let model_number = sysinfo
            .as_deref()
            .and_then(|s| sysinfo_field(s, "ModelNumStr"));
        let serial = sysinfo
            .as_deref()
            .and_then(|s| sysinfo_field(s, "pszSerialNumber"));

        let generation = model_number
            .as_deref()
            .map(normalise_model)
            .map(|m| generation_for_model(&m))
            .filter(|g| *g != Generation::Unknown)
            .or_else(|| shuffle_by_its_list(mount_point))
            .unwrap_or(Generation::Unknown);

        let firewire_id = std::fs::read_to_string(device_dir.join("SysInfoExtended"))
            .ok()
            .and_then(|s| plist_string_after_key(&s, "FirewireGuid"));

        Self {
            generation,
            model_number,
            serial,
            firewire_id,
        }
    }

    pub fn checksum(&self) -> Checksum {
        self.generation.checksum()
    }

    pub fn writable(&self) -> std::result::Result<(), String> {
        match self.checksum() {
            Checksum::None => Ok(()),
            Checksum::Hash58 => Err(format!(
                "{} signs its database with a checksum (hash58) that kessler \
                 cannot compute yet. Writing an unsigned database would make the \
                 iPod show an empty library, so nothing was written.",
                self.generation.label()
            )),
            Checksum::Unknown => Err(format!(
                "could not identify this iPod{}. Some models refuse an unsigned \
                 database and then show an empty library, so kessler will not \
                 write to hardware it cannot recognise.",
                match &self.model_number {
                    Some(m) => format!(" (model {m})"),
                    None => " — no readable iPod_Control/Device/SysInfo".into(),
                }
            )),
        }
    }
}

fn shuffle_by_its_list(mount_point: &Path) -> Option<Generation> {
    let list = std::fs::read(mount_point.join("iPod_Control").join("iTunes").join("iTunesSD")).ok()?;
    if list.starts_with(b"bdhs") {
        Some(Generation::Shuffle4)
    } else if list.len() >= 18 && (list.len() - 18) % 558 == 0 {
        Some(Generation::Shuffle2)
    } else {
        None
    }
}

fn sysinfo_field(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        (k.trim().eq_ignore_ascii_case(key)).then(|| v.trim().to_string())
    })
}

fn normalise_model(raw: &str) -> String {
    readings(raw)
        .into_iter()
        .find(|reading| generation_for_model(reading) != Generation::Unknown)
        .unwrap_or_else(|| {
            readings(raw).pop().unwrap_or_default()
        })
}

fn readings(raw: &str) -> Vec<String> {
    let alnum: Vec<char> = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_uppercase())
        .collect();

    let mut readings = Vec::new();
    for dropped in 0..=2 {
        if dropped > 0 && !alnum.get(dropped - 1).is_some_and(|c| c.is_ascii_alphabetic()) {
            break;
        }
        let rest = &alnum[dropped.min(alnum.len())..];
        if rest.len() < 4 {
            break;
        }
        readings.push(rest[..4].iter().collect());
    }
    if readings.is_empty() {
        readings.push(alnum.into_iter().collect());
    }
    readings
}

fn plist_string_after_key(xml: &str, key: &str) -> Option<String> {
    let needle = format!("<key>{key}</key>");
    let rest = &xml[xml.find(&needle)? + needle.len()..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")?;
    Some(rest[start..start + end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_generation_plays_mp3_but_not_flac() {
        let g = Generation::Third;
        assert!(g.plays("mp3"));
        assert!(g.plays("MP3"));
        assert!(g.plays(".m4a"));
        assert!(g.plays("wav"));
        assert!(!g.plays("flac"));
        assert!(!g.plays("ogg"));
        assert!(!g.plays("alac"));
    }

    #[test]
    fn later_models_gained_apple_lossless() {
        assert!(Generation::Video1.plays("alac"));
        assert!(!Generation::Video1.plays("flac"));
    }

    #[test]
    fn identifies_the_2003_third_generation() {
        assert_eq!(generation_for_model("9460"), Generation::Third);
        assert_eq!(Generation::Third.checksum(), Checksum::None);
        assert!(Generation::Third.checksum().is_supported_for_writing());
    }

    #[test]
    fn identifies_the_2007_classic_as_needing_a_checksum() {
        assert_eq!(generation_for_model("B145"), Generation::Classic1);
        assert_eq!(generation_for_model("C293"), Generation::Classic3);
        assert_eq!(Generation::Classic1.checksum(), Checksum::Hash58);
        assert!(!Generation::Classic1.checksum().is_supported_for_writing());
    }

    #[test]
    fn unknown_hardware_is_not_written_to() {
        assert_eq!(generation_for_model("ZZZZ"), Generation::Unknown);
        assert_eq!(Generation::Unknown.checksum(), Checksum::Unknown);
        assert!(!Generation::Unknown.checksum().is_supported_for_writing());
    }

    #[test]
    fn a_part_number_is_read_whichever_way_it_is_written() {
        for spelling in ["8976", "M8976", "x8976", "xM8976", "M8976LL/A"] {
            assert_eq!(
                generation_for_model(&normalise_model(spelling)),
                Generation::Third,
                "{spelling} was not read as a 3G"
            );
        }
        for spelling in ["A146", "xA146", "MA146", "xMA146", "MA146LL/A"] {
            assert_eq!(
                generation_for_model(&normalise_model(spelling)),
                Generation::Video1,
                "{spelling} was not read as a 5G"
            );
        }
        assert_eq!(normalise_model("xA146"), "A146");
        assert_eq!(normalise_model("x9460"), "9460");

        for nonsense in ["quatsch", "ZZZZ", "", "12"] {
            assert_eq!(
                generation_for_model(&normalise_model(nonsense)),
                Generation::Unknown,
                "{nonsense} was read as a real iPod"
            );
        }
    }

    #[test]
    fn resolves_a_third_generation_ipod_end_to_end() {
        let raw = "BoardHwName: iPod\npszSerialNumber: ABC\nModelNumStr: x9460\n";
        let model = sysinfo_field(raw, "ModelNumStr").unwrap();
        let generation = generation_for_model(&normalise_model(&model));
        assert_eq!(generation, Generation::Third);
        assert_eq!(generation.checksum(), Checksum::None);
        assert_eq!(generation.music_folders(), 50);
    }

    #[test]
    fn reads_sysinfo_fields() {
        let s = "BoardHwName: iPod\npszSerialNumber: ABC123\nModelNumStr: xA146\n";
        assert_eq!(sysinfo_field(s, "ModelNumStr").as_deref(), Some("xA146"));
        assert_eq!(sysinfo_field(s, "pszSerialNumber").as_deref(), Some("ABC123"));
        assert_eq!(sysinfo_field(s, "NoSuchKey"), None);
    }

    #[test]
    fn reads_firewire_id_from_plist() {
        let xml = "<dict><key>FirewireGuid</key><string>000A27001CE1B2D3</string></dict>";
        assert_eq!(
            plist_string_after_key(xml, "FirewireGuid").as_deref(),
            Some("000A27001CE1B2D3")
        );
    }

    #[test]
    fn missing_device_dir_yields_unknown_and_refuses_writes() {
        let info = DeviceInfo::read(std::path::Path::new("/nonexistent"));
        assert_eq!(info.generation, Generation::Unknown);
        assert!(info.writable().is_err());
    }

    #[test]
    fn a_shuffle_is_known_by_its_list() {
        let dir = std::env::temp_dir().join(format!("kessler-sd-{}", std::process::id()));
        let itunes = dir.join("iPod_Control").join("iTunes");
        std::fs::create_dir_all(&itunes).unwrap();
        assert_eq!(DeviceInfo::read(&dir).generation, Generation::Unknown);
        std::fs::write(itunes.join("iTunesSD"), b"bdhs\x03\x00\x00\x00").unwrap();
        assert_eq!(DeviceInfo::read(&dir).generation, Generation::Shuffle4);
        std::fs::write(itunes.join("iTunesSD"), vec![0u8; 18 + 2 * 558]).unwrap();
        assert_eq!(DeviceInfo::read(&dir).generation, Generation::Shuffle2);
        std::fs::write(itunes.join("iTunesSD"), b"nonsense").unwrap();
        assert_eq!(DeviceInfo::read(&dir).generation, Generation::Unknown);
        let device = dir.join("iPod_Control").join("Device");
        std::fs::create_dir_all(&device).unwrap();
        std::fs::write(device.join("SysInfo"), "ModelNumStr: xD775\n").unwrap();
        assert_eq!(DeviceInfo::read(&dir).generation, Generation::Shuffle4);
        std::fs::write(device.join("SysInfo"), "ModelNumStr: xA146\n").unwrap();
        assert_eq!(DeviceInfo::read(&dir).generation, Generation::Video1);
        std::fs::remove_dir_all(&dir).ok();
    }
}
