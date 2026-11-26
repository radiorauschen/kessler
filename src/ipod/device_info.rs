//! Identifying which iPod is attached, and what that implies for writing.
//!
//! The model number in `iPod_Control/Device/SysInfo` maps to a generation, and
//! the generation decides whether the device demands a cryptographic checksum
//! in the database header. Getting this wrong is not a cosmetic problem: a
//! device that wants a checksum and does not find one shows an *empty library*
//! even though every file is still there.
//!
//! Model and checksum tables follow libgpod's `itdb_device.c`.

use std::path::Path;

/// iPod hardware generations, as far as Kessler needs to tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Generation {
    First,
    Second,
    /// The 2003 iPod with the row of touch buttons. *Not* the "iPod Classic".
    Third,
    Fourth,
    Photo,
    Mini1,
    Mini2,
    Nano1,
    Nano2,
    Nano3,
    Nano4,
    /// 5G "iPod Video".
    Video1,
    /// 5.5G.
    Video2,
    Shuffle1,
    Shuffle2,
    Shuffle3,
    Shuffle4,
    /// The 2007 iPod Classic (6G).
    Classic1,
    /// 2008 iPod Classic 120GB (6.5G).
    Classic2,
    /// 2009 iPod Classic 160GB (7G).
    Classic3,
    Unknown,
}

impl Generation {
    /// What Kessler prints for the user.
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

    /// The checksum this generation's firmware demands in the database header.
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

    /// File extensions this generation's firmware can actually play.
    ///
    /// FLAC and Ogg Vorbis appear on no iPod of any generation. Copying one
    /// over is a silent failure: the file transfers, occupies space, and the
    /// device simply never lists it.
    pub fn playable_formats(self) -> &'static [&'static str] {
        match self {
            Self::First | Self::Second => &["mp3", "wav", "aif", "aiff"],
            Self::Third => &["mp3", "m4a", "aac", "wav", "aif", "aiff"],
            Self::Shuffle1 | Self::Shuffle2 | Self::Shuffle3 | Self::Shuffle4 => &["mp3", "m4a", "aac", "wav"],
            Self::Unknown => &["mp3"],
            _ => &["mp3", "m4a", "aac", "m4b", "wav", "aif", "aiff", "alac"],
        }
    }

    /// A shuffle: no screen, and what it plays is its `iTunesSD` — see
    /// `itunessd`. kessler treats it plainly:
    /// it copies and removes music, and leaves alone what nobody sees on a
    /// device without a screen — tags and Sound Check written back.
    pub fn is_shuffle(self) -> bool {
        matches!(self, Self::Shuffle1 | Self::Shuffle2 | Self::Shuffle3 | Self::Shuffle4)
    }

    /// Whether it has playlists one can choose on it: the first two
    /// shuffles play one list, the whole of it.
    pub fn has_playlists(self) -> bool {
        !matches!(self, Self::Shuffle1 | Self::Shuffle2)
    }

    pub fn plays(self, extension: &str) -> bool {
        let ext = extension.trim_start_matches('.').to_ascii_lowercase();
        self.playable_formats().contains(&ext.as_str())
    }

    /// How many `F##` folders the device's Music directory uses.
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

/// The checksum scheme a device requires over its iTunesDB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checksum {
    /// No signature needed — Kessler can write this device.
    None,
    /// HMAC-SHA1 over the database, keyed from the FireWire GUID.
    Hash58,
    /// Not identified.
    Unknown,
}

impl Checksum {
    #[cfg(test)]
    pub fn is_supported_for_writing(self) -> bool {
        matches!(self, Self::None)
    }
}

/// Map a 4-character model number to its generation.
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

impl Generation {
    /// Every model one can say a device is, when it cannot say so itself —
    /// see [`write_model`]: the order of the table above, the oldest first.
    pub const CHOOSABLE: [Generation; 20] = [
        Generation::First,
        Generation::Second,
        Generation::Third,
        Generation::Fourth,
        Generation::Photo,
        Generation::Mini1,
        Generation::Mini2,
        Generation::Nano1,
        Generation::Nano2,
        Generation::Nano3,
        Generation::Nano4,
        Generation::Video1,
        Generation::Video2,
        Generation::Shuffle1,
        Generation::Shuffle2,
        Generation::Shuffle3,
        Generation::Shuffle4,
        Generation::Classic1,
        Generation::Classic2,
        Generation::Classic3,
    ];

    /// One model number of this generation, as `SysInfo` writes it.
    fn a_model_number(self) -> Option<&'static str> {
        Some(match self {
            Self::First => "8513",
            Self::Second => "8737",
            Self::Third => "8976",
            Self::Fourth => "9282",
            Self::Photo => "A079",
            Self::Mini1 => "9160",
            Self::Mini2 => "9800",
            Self::Nano1 => "A350",
            Self::Nano2 => "A477",
            Self::Nano3 => "A978",
            Self::Nano4 => "B480",
            Self::Video1 => "A002",
            Self::Video2 => "A444",
            Self::Shuffle1 => "9724",
            Self::Shuffle2 => "A546",
            Self::Shuffle3 => "C306",
            Self::Shuffle4 => "C584",
            Self::Classic1 => "B029",
            Self::Classic2 => "B562",
            Self::Classic3 => "C293",
            Self::Unknown => return None,
        })
    }
}

/// Say what model a device is, where it cannot say so itself: its
/// `iPod_Control/Device/SysInfo` is gone — a restore or a fresh format takes
/// it away — and nothing is written to a device nobody recognises. Written
/// back with one model number of the generation chosen, as gtkpod does, so
/// the device is known again from then on, to other programs too. A
/// `SysInfo` that is there keeps its other lines.
pub fn write_model(mount_point: &Path, generation: Generation) -> std::io::Result<()> {
    let Some(number) = generation.a_model_number() else {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "no model to write"));
    };
    let dir = mount_point.join("iPod_Control").join("Device");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("SysInfo");
    let kept: String = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.trim_start().starts_with("ModelNumStr"))
        .map(|line| format!("{line}\n"))
        .collect();
    std::fs::write(&path, format!("{kept}ModelNumStr: x{number}\n"))
}

/// What Kessler knows about the attached hardware.
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub generation: Generation,
    /// Raw `ModelNumStr` from SysInfo, if it was readable.
    pub model_number: Option<String>,
    pub serial: Option<String>,
    /// The FireWire GUID the database is signed with (`hash58`), and where
    /// it was found: SysInfoExtended, SysInfo, or the USB serial number,
    /// which on these iPods is the same sixteen digits. Looked for on USB
    /// only for a device that wants the signature.
    pub guid: Option<([u8; 8], &'static str)>,
    /// The database format the device says it wants, from SysInfoExtended's
    /// `DBVersion`: 0 to 2 unsigned, 3 and up signed. The device's own word,
    /// so it outranks the model: a model chosen wrongly by hand cannot make
    /// kessler write an unsigned database to one that wants a signature.
    pub db_version: Option<u32>,
    /// The database already on the device carries a signature — the hash58
    /// field of its header is not empty: whatever wrote it knew the device
    /// wants one, and it is not written unsigned over it.
    pub signed_database: bool,
    /// It wants a later signature than hash58 — its database says a later
    /// hashing scheme, or SysInfoExtended a `DBVersion` past 3 — which kessler
    /// cannot write: the nano of the fifth generation and after.
    pub later_signature: bool,
}

impl DeviceInfo {
    /// Read what we can from `iPod_Control/Device/`.
    ///
    /// A missing or unreadable SysInfo is not an error: plenty of iPods have
    /// been reformatted and lost it. It just means the generation is unknown,
    /// and unknown generations are not written to.
    pub fn read(mount_point: &Path) -> Self {
        let device_dir = mount_point.join("iPod_Control").join("Device");

        let sysinfo = std::fs::read_to_string(device_dir.join("SysInfo")).ok();
        let model_number = sysinfo
            .as_deref()
            .and_then(|s| sysinfo_field(s, "ModelNumStr"));
        let serial = sysinfo
            .as_deref()
            .and_then(|s| sysinfo_field(s, "pszSerialNumber"));

        let extended = std::fs::read_to_string(device_dir.join("SysInfoExtended")).ok();
        let serial = serial.or_else(|| extended.as_deref().and_then(|s| plist_string_after_key(s, "SerialNumber")));

        let generation = model_number
            .as_deref()
            .map(normalise_model)
            .map(|m| generation_for_model(&m))
            .filter(|g| *g != Generation::Unknown)
            .or_else(|| serial.as_deref().and_then(model_for_serial).map(generation_for_model).filter(|g| *g != Generation::Unknown))
            .or_else(|| shuffle_by_its_list(mount_point))
            .or_else(|| by_usb(mount_point, model_number.is_none()))
            .unwrap_or(Generation::Unknown);

        let firewire_id = extended
            .as_deref()
            .and_then(|s| plist_string_after_key(s, "FirewireGuid").or_else(|| plist_string_after_key(s, "FireWireGUID")));
        let db_version = extended.as_deref().and_then(|s| plist_integer_after_key(s, "DBVersion"));
        let found = database_signature(mount_point);

        let mut info = Self {
            generation,
            model_number,
            serial,
            guid: firewire_id
                .as_deref()
                .and_then(super::hash58::parse_guid)
                .map(|g| (g, "SysInfoExtended"))
                .or_else(|| {
                    sysinfo
                        .as_deref()
                        .and_then(|s| sysinfo_field(s, "FirewireGuid"))
                        .as_deref()
                        .and_then(super::hash58::parse_guid)
                        .map(|g| (g, "SysInfo"))
                }),
            db_version,
            signed_database: found != Signature::None,
            later_signature: found == Signature::Later || db_version.is_some_and(|v| v >= 4),
        };
        if info.guid.is_none() && info.checksum() == Checksum::Hash58 {
            info.guid = usb_lookup(mount_point, &mut Vec::new())
                .and_then(|usb| usb.serial)
                .as_deref()
                .and_then(super::hash58::parse_guid)
                .map(|g| (g, "usb"));
        }
        info
    }

    /// The GUID to sign the database with, for a device that wants it
    /// signed and where one was found; None for every device written
    /// unsigned.
    pub fn signing(&self) -> Option<[u8; 8]> {
        (self.checksum() == Checksum::Hash58).then_some(self.guid.map(|(g, _)| g)).flatten()
    }

    /// What the device wants: a signature where its database already has one
    /// or where it says so itself (`DBVersion` 3 and up), else what its model
    /// wants.
    ///
    /// A later scheme than hash58 is one kessler does not know: Unknown, and
    /// not written.
    pub fn checksum(&self) -> Checksum {
        if self.later_signature {
            return Checksum::Unknown;
        }
        if self.signed_database || self.db_version.is_some_and(|v| v >= 3) {
            return Checksum::Hash58;
        }
        self.generation.checksum()
    }

    /// Whether Kessler may write this device's database, and why not if not.
    pub fn writable(&self) -> std::result::Result<(), String> {
        match self.checksum() {
            Checksum::None => Ok(()),
            Checksum::Hash58 if self.guid.is_some() => Ok(()),
            Checksum::Hash58 => Err(format!(
                "{} signs its database with a checksum (hash58) made from its \
                 FireWire GUID, and kessler found none — not in SysInfoExtended, \
                 not in SysInfo, not as the USB serial number. Writing an unsigned \
                 database would make the iPod show an empty library, so nothing \
                 was written.",
                if self.generation == Generation::Unknown || self.signed_database || self.db_version.is_some_and(|v| v >= 3) {
                    "this iPod"
                } else {
                    self.generation.label()
                }
            )),
            Checksum::Unknown if self.later_signature => Err(
                "this iPod wants a later signature on its database than the one \
                 kessler can write (hash58): the nano of the fifth generation and \
                 after. Writing would make it show an empty library, so nothing \
                 was written."
                    .into(),
            ),
            Checksum::Unknown => Err(format!(
                "could not identify this iPod{}. Some models refuse an unsigned \
                 database and then show an empty library, so kessler will not \
                 write to hardware it cannot recognise.",
                match &self.model_number {
                    Some(m) => format!(" (model {m})"),
                    None => " — no model number in iPod_Control/Device/SysInfo, nor one on USB".into(),
                }
            )),
        }
    }
}

/// The generation an iPod's USB product number stands for (Apple, `05ac`),
/// as the public list of USB numbers (`usb.ids`) names them: one number for a
/// whole generation, the minis' two and the video's two each sharing one. The
/// numbers that name no one generation (`1206`–`1208`) are left out.
fn generation_for_usb(product: u16) -> Option<Generation> {
    Some(match product {
        0x1201 => Generation::Third,
        0x1203 => Generation::Fourth,
        0x1204 => Generation::Photo,
        0x1205 => Generation::Mini1,
        0x1209 => Generation::Video1,
        0x120A => Generation::Nano1,
        0x1260 => Generation::Nano2,
        0x1261 => Generation::Classic1,
        0x1262 => Generation::Nano3,
        0x1263 => Generation::Nano4,
        0x1300 => Generation::Shuffle1,
        0x1301 => Generation::Shuffle2,
        0x1302 => Generation::Shuffle3,
        0x1303 => Generation::Shuffle4,
        _ => return None,
    })
}

/// The model an iPod is by the number it gives on USB, where nothing else
/// said it — what iTunes asks the device when it writes `SysInfo`, which a
/// restore or a fresh format without iTunes leaves out, and another program
/// may write without a model number in it. Where the model number is
/// missing (`missing`) it is written into `SysInfo` (`write_model`, the
/// file's other lines kept), so this is asked once and the device is known
/// from then on, to other programs too; a model number there that is not
/// known is the device's own word and stays — the USB number counts only for
/// now. Nothing on a FireWire connection, which has no such number.
fn by_usb(mount_point: &Path, missing: bool) -> Option<Generation> {
    let Usb { vendor, product, .. } = usb_lookup(mount_point, &mut Vec::new())?;
    if vendor != 0x05AC {
        return None;
    }
    let generation = generation_for_usb(product)?;
    if !missing {
        return Some(generation);
    }
    if let Err(e) = write_model(mount_point, generation) {
        tracing::debug!("SysInfo not written: {e}");
    } else {
        tracing::info!("model known by usb ({product:04x}): {}", generation.label());
    }
    Some(generation)
}

/// What USB says about the device a volume is on — see [`usb_lookup`].
#[derive(Debug, Clone, PartialEq, Eq)]
struct Usb {
    vendor: u16,
    product: u16,
    /// Its serial number: on an iPod, its FireWire GUID.
    serial: Option<String>,
}

/// How the USB numbers were looked for, step by step, for the report: what
/// each step found, or where it stopped.
pub fn usb_trace(mount_point: &Path) -> Vec<String> {
    let mut said = Vec::new();
    let found = usb_lookup(mount_point, &mut said);
    said.push(match found {
        Some(Usb { vendor, product, serial }) => {
            let serial = match serial {
                Some(serial) => format!(" · seriennummer {serial}"),
                None => String::new(),
            };
            match (vendor, generation_for_usb(product)) {
                (0x05AC, Some(generation)) => format!("usb {vendor:04x}:{product:04x} → {}{serial}", generation.label()),
                _ => format!("usb {vendor:04x}:{product:04x} — keine generation dazu bekannt{serial}"),
            }
        }
        None => "keine usb-kennung gefunden".into(),
    });
    said
}

/// Under `/sys`, from the block device `/proc/mounts` names for the volume,
/// up to the USB device it hangs off. Each step said in `said`.
#[cfg(target_os = "linux")]
fn usb_lookup(mount_point: &Path, said: &mut Vec<String>) -> Option<Usb> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    let wanted = mount_point.to_string_lossy().into_owned();
    let device = mounts.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let device = parts.next()?;
        let at = parts.next()?.replace("\\040", " ");
        (at.trim_end_matches('/') == wanted.trim_end_matches('/')).then(|| device.to_string())
    });
    said.push(format!("eingehängt von: {}", device.as_deref().unwrap_or("nicht in /proc/mounts")));
    let name = Path::new(&device?).file_name()?.to_owned();
    let mut at = std::fs::canonicalize(Path::new("/sys/class/block").join(name)).ok()?;
    said.push(format!("in /sys: {}", at.display()));
    loop {
        let read = |file: &str| {
            std::fs::read_to_string(at.join(file)).ok().and_then(|s| u16::from_str_radix(s.trim(), 16).ok())
        };
        if let (Some(vendor), Some(product)) = (read("idVendor"), read("idProduct")) {
            let serial = std::fs::read_to_string(at.join("serial")).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
            return Some(Usb { vendor, product, serial });
        }
        if !at.pop() || at.as_os_str().len() <= "/sys".len() {
            said.push("kein usb-gerät darüber".into());
            return None;
        }
    }
}

/// On a Mac: `diskutil` names the whole disk the volume is on, and in
/// `ioreg`'s tree of USB devices the one whose media is that disk says its
/// numbers. Each step said in `said`.
#[cfg(target_os = "macos")]
fn usb_lookup(mount_point: &Path, said: &mut Vec<String>) -> Option<Usb> {
    let out = std::process::Command::new("/usr/sbin/diskutil").arg("info").arg("-plist").arg(mount_point).output().ok()?;
    let plist = String::from_utf8_lossy(&out.stdout);
    let disk = plist_string_after_key(&plist, "ParentWholeDisk");
    let bus = plist_string_after_key(&plist, "BusProtocol");
    said.push(format!(
        "diskutil: {} · {}",
        disk.as_deref().unwrap_or("keine platte"),
        bus.as_deref().unwrap_or("bus unbekannt")
    ));
    let disk = disk?;
    let tree = std::process::Command::new("/usr/sbin/ioreg").args(["-r", "-c", "IOUSBHostDevice", "-l", "-w0"]).output().ok()?;
    let tree = String::from_utf8_lossy(&tree.stdout);
    let found = usb_in_ioreg(&tree, &disk);
    said.push(match found {
        Some(_) => format!("in ioreg: {disk} gefunden"),
        None if tree.contains(&format!("\"BSD Name\" = \"{disk}\"")) => format!("in ioreg: {disk} ohne usb-kennung davor"),
        None => format!("in ioreg: kein usb-gerät mit {disk}"),
    });
    found
}

/// In `ioreg`'s tree of USB devices, the numbers of the one whose media is
/// `disk`: the nearest `"idVendor" = …` and `"idProduct" = …` before the
/// line that names the disk — its own device's, however many hubs stand
/// above it in the tree. The first ones of the tree were the first hub's.
/// Its serial number from the same device's lines only, from where that
/// device begins (`+-o`) on: a hub's would sign the database for no iPod.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn usb_in_ioreg(tree: &str, disk: &str) -> Option<Usb> {
    let bsd = format!("\"BSD Name\" = \"{disk}\"");
    let before = &tree[..tree.find(&bsd)?];
    let last = |key: &str| {
        let needle = format!("\"{key}\" = ");
        let at = before.rfind(&needle)?;
        let rest = &before[at + needle.len()..];
        let n = rest.split(|c: char| !c.is_ascii_digit()).next()?.parse::<u32>().ok().and_then(|n| u16::try_from(n).ok())?;
        Some((at, n))
    };
    let (vendor_at, vendor) = last("idVendor")?;
    let (_, product) = last("idProduct")?;
    let own = before[..vendor_at].rfind("+-o ").unwrap_or(0);
    let serial = ["USB Serial Number", "kUSBSerialNumberString"].iter().find_map(|key| {
        let needle = format!("\"{key}\" = \"");
        let at = before.rfind(&needle).filter(|at| *at >= own)?;
        let rest = &before[at + needle.len()..];
        Some(rest[..rest.find('"')?].to_string()).filter(|s| !s.is_empty())
    });
    Some(Usb { vendor, product, serial })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn usb_lookup(_mount_point: &Path, said: &mut Vec<String>) -> Option<Usb> {
    said.push("auf diesem system nicht gefragt".into());
    None
}

/// The files in `iPod_Control/Device`, for the report: which of what the
/// model is read from are there.
pub fn device_files(mount_point: &Path) -> String {
    let dir = mount_point.join("iPod_Control").join("Device");
    match std::fs::read_dir(&dir) {
        Ok(entries) => {
            let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
            names.sort();
            if names.is_empty() { "leer".into() } else { names.join(", ") }
        }
        Err(_) => "kein ordner iPod_Control/Device".into(),
    }
}

/// A shuffle known by the list it plays from, when its model number says
/// nothing — missing, or one the table lacks. Only a shuffle has an
/// `iTunesSD`, and its shape tells the two kinds apart: the third and fourth
/// begin with `bdhs`, the first and second are a short head and 558 bytes a
/// track. Which of each pair cannot be told; both of a pair are written
/// alike, so only the picture can be wrong. Every shuffle takes an unsigned
/// database — it never reads it — so nothing is risked by writing to it.
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

/// `Key: value` lookup in the plain-text SysInfo file.
fn sysinfo_field(contents: &str, key: &str) -> Option<String> {
    contents.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        (k.trim().eq_ignore_ascii_case(key)).then(|| v.trim().to_string())
    })
}

/// The device writes its model as a single prefix letter plus the four
/// characters that identify the model: `x9460`, `xA146`. Apple's own part
/// numbers add a region suffix (`M9460LL/A`).
///
/// libgpod drops one leading letter and looks the rest up verbatim; doing the
/// same, plus trimming anything past the fourth character, also handles the
/// part-number spelling.
fn normalise_model(raw: &str) -> String {
    readings(raw)
        .into_iter()
        .find(|reading| generation_for_model(reading) != Generation::Unknown)
        .unwrap_or_else(|| {
            readings(raw).pop().unwrap_or_default()
        })
}

/// Every way of reading what the device calls itself, fewest letters dropped
/// first.
///
/// Apple writes the same part number in more than one shape, and `SysInfo`
/// adds a prefix of its own on top: the iPod 3G is `M8976`, and the file may
/// say `xM8976`; the 5G is `A146`, written `xA146` — but also `MA146` in
/// Apple's own materials, and so possibly `xMA146`. Dropping exactly one
/// leading letter reads `xA146` correctly and turns `xM8976` into `M897`,
/// which is nothing at all. That is not a guess worth making: a device the
/// program does not recognise is a device it refuses to write to, so guessing
/// wrong means a working iPod is told it cannot be written to.
///
/// So every reading is offered and the table decides. It cannot go wrong in
/// the dangerous direction — a number nothing recognises is still unknown, and
/// unknown is still refused.
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

/// Pull `<key>NAME</key><string>VALUE</string>` out of SysInfoExtended.
///
/// A five-line scan beats adding a plist parser for one field.
/// Pull `<key>NAME</key><integer>VALUE</integer>` out of SysInfoExtended.
fn plist_integer_after_key(xml: &str, key: &str) -> Option<u32> {
    let needle = format!("<key>{key}</key>");
    let rest = &xml[xml.find(&needle)? + needle.len()..];
    let rest = rest.trim_start();
    let rest = rest.strip_prefix("<integer>")?;
    rest[..rest.find("</integer>")?].trim().parse().ok()
}

/// What signature the database on the device carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Signature {
    None,
    Hash58,
    /// A later scheme than hash58, as the header's 0x30 says.
    Later,
}

/// The signature the database on the device carries: the hashing scheme its
/// header names at 0x30 (one for hash58, more for the later ones), else
/// whether the twenty bytes of the hash58 at 0x58 are filled — empty in an
/// unsigned database.
fn database_signature(mount_point: &Path) -> Signature {
    use std::io::Read;
    let path = mount_point.join("iPod_Control").join("iTunes").join("iTunesDB");
    let Ok(mut file) = std::fs::File::open(path) else { return Signature::None };
    let mut head = [0u8; 0x6C];
    if file.read_exact(&mut head).is_err() || &head[..4] != b"mhbd" {
        return Signature::None;
    }
    let header = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
    if header >= 0x32 {
        match u16::from_le_bytes([head[0x30], head[0x31]]) {
            0 => {}
            1 => return Signature::Hash58,
            _ => return Signature::Later,
        }
    }
    let end = header.clamp(0x58, 0x6C);
    if head[0x58..end].iter().any(|b| *b != 0) { Signature::Hash58 } else { Signature::None }
}

/// The model number the last three characters of an Apple serial number
/// stand for — libgpod's table (from podsleuth), for the models that take an
/// unsigned database. None for a serial too short or a suffix not listed.
fn model_for_serial(serial: &str) -> Option<&'static str> {
    let serial = serial.trim();
    let suffix = serial.get(serial.len().checked_sub(3)?..)?.to_ascii_uppercase();
    SERIAL_MODELS.iter().find(|(s, _)| *s == suffix).map(|(_, m)| *m)
}

/// Serial suffixes and the model numbers they mean. Where libgpod lists a
/// suffix twice, the later one, as its table keeps it.
const SERIAL_MODELS: &[(&str, &str)] = &[
    ("LG6", "8541"), ("NAM", "8541"), ("MJ2", "8541"), ("ML1", "8709"), ("MME", "8709"),
    ("MMB", "8737"), ("MMC", "8738"), ("NGE", "8740"), ("NGH", "8740"), ("MMF", "8741"),
    ("NRH", "8976"), ("NLW", "8946"), ("QQF", "9460"), ("PQ5", "9244"), ("PNT", "9244"),
    ("NLY", "8948"), ("NM7", "8948"), ("PNU", "9245"),
    ("PS9", "9282"), ("Q8U", "9282"), ("V9V", "9787"), ("S2X", "9787"), ("PQ7", "9268"),
    ("TDU", "A079"), ("TDS", "A079"), ("TM2", "A127"), ("SAZ", "9830"), ("SB1", "9830"),
    ("SAY", "9829"), ("R5Q", "9585"), ("R5R", "9586"), ("R5T", "9586"),
    ("PFW", "9160"), ("PRC", "9160"), ("QKL", "9436"), ("QKQ", "9436"), ("QKK", "9435"),
    ("QKP", "9435"), ("QKJ", "9434"), ("QKN", "9434"), ("QKM", "9437"), ("QKR", "9437"),
    ("S41", "9800"), ("S4C", "9800"), ("S43", "9802"), ("S45", "9804"), ("S47", "9806"),
    ("S4J", "9806"), ("S42", "9801"), ("S44", "9803"), ("S48", "9807"),
    ("UNA", "A350"), ("UNB", "A350"), ("UPR", "A352"), ("UPS", "A352"), ("SZB", "A004"),
    ("SZV", "A004"), ("SZW", "A004"), ("SZC", "A005"), ("SZT", "A005"), ("TJT", "A099"),
    ("TJU", "A099"), ("TK2", "A107"), ("TK3", "A107"),
    ("VQ5", "A477"), ("VQ6", "A477"), ("V8T", "A426"), ("V8U", "A426"), ("V8W", "A428"),
    ("V8X", "A428"), ("VQH", "A487"), ("VQJ", "A487"), ("VQK", "A489"), ("VKL", "A489"),
    ("WL2", "A725"), ("WL3", "A725"), ("X9A", "A726"), ("X9B", "A726"), ("VQT", "A497"),
    ("VQU", "A497"),
    ("SZ9", "A002"), ("WEC", "A002"), ("WED", "A002"), ("WEG", "A002"), ("WEH", "A002"),
    ("WEL", "A002"), ("TXK", "A146"), ("TXM", "A146"), ("WEF", "A146"), ("WEJ", "A146"),
    ("WEK", "A146"), ("SZA", "A003"), ("SZU", "A003"), ("TXL", "A147"), ("TXN", "A147"),
    ("V9K", "A444"), ("V9L", "A444"), ("WU9", "A444"), ("VQM", "A446"), ("V9M", "A446"),
    ("V9N", "A446"), ("WEE", "A446"), ("V9P", "A448"), ("V9Q", "A448"), ("V9R", "A450"),
    ("V9S", "A450"), ("V95", "A450"), ("V96", "A450"), ("WUC", "A450"), ("W9G", "A664"),
];

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

    /// The same iPod, written every way an iPod writes itself.
    ///
    /// This decides whether a device may be written to at all, so getting it
    /// wrong is not cosmetic: an unrecognised number is refused, and a working
    /// iPod is then told it cannot be synced. `xM8976` — a 3G with both the
    /// file's prefix and Apple's own letter in front of the number — used to
    /// come out as `M897`, which is nothing.
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

    /// A shuffle whose model number the table lacks, or which has none, is
    /// known by its `iTunesSD`; any other device without one stays unknown.
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

    /// A database kessler wrote is not taken for a signed one; one with a
    /// hash in its header is, and the device is not written then — nor one
    /// that says itself it wants a signature, whatever model it is said to be.
    /// The serial names the model where the model number is missing.
    #[test]
    fn a_signature_wanted_or_found_keeps_kessler_from_writing() {
        let dir = std::env::temp_dir().join(format!("kessler-signed-{}", std::process::id()));
        let itunes = dir.join("iPod_Control").join("iTunes");
        std::fs::create_dir_all(&itunes).unwrap();
        write_model(&dir, Generation::Video1).unwrap();
        let db = crate::ipod::itunesdb::ItunesDb { version: 0, db_id: 7, language: [0, 0], library_id: 0, tracks: Vec::new(), playlists: Vec::new() };
        std::fs::write(itunes.join("iTunesDB"), crate::ipod::itunesdb::writer::write(&db)).unwrap();
        let info = DeviceInfo::read(&dir);
        assert!(!info.signed_database);
        assert!(info.writable().is_ok());
        let mut signed = vec![0u8; 0xBC];
        signed[..4].copy_from_slice(b"mhbd");
        signed[4..8].copy_from_slice(&0xBCu32.to_le_bytes());
        signed[0x60] = 0x5A;
        std::fs::write(itunes.join("iTunesDB"), &signed).unwrap();
        assert!(DeviceInfo::read(&dir).writable().is_err());
        std::fs::remove_file(itunes.join("iTunesDB")).unwrap();
        let device = dir.join("iPod_Control").join("Device");
        std::fs::write(device.join("SysInfoExtended"), "<dict><key>DBVersion</key>\n<integer>3</integer></dict>").unwrap();
        assert_eq!(DeviceInfo::read(&dir).db_version, Some(3));
        assert!(DeviceInfo::read(&dir).writable().is_err());
        std::fs::remove_file(device.join("SysInfoExtended")).unwrap();
        std::fs::write(device.join("SysInfo"), "pszSerialNumber: 4H5101QQF\n").unwrap();
        assert_eq!(DeviceInfo::read(&dir).generation, Generation::Third);
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A classic is written once its GUID is found, signed; without one, not
    /// at all; and a device that wants a later signature never, whatever
    /// GUID it has.
    #[test]
    fn a_classic_is_signed_with_its_guid_and_a_later_one_is_left_alone() {
        let dir = std::env::temp_dir().join(format!("kessler-guid-{}", std::process::id()));
        let device = dir.join("iPod_Control").join("Device");
        let itunes = dir.join("iPod_Control").join("iTunes");
        std::fs::create_dir_all(&device).unwrap();
        std::fs::create_dir_all(&itunes).unwrap();

        std::fs::write(device.join("SysInfo"), "ModelNumStr: xB029\n").unwrap();
        let info = DeviceInfo::read(&dir);
        assert_eq!(info.generation, Generation::Classic1);
        assert_eq!(info.signing(), None);
        assert!(info.writable().unwrap_err().contains("FireWire GUID"));

        std::fs::write(device.join("SysInfo"), "ModelNumStr: xB029\nFirewireGuid: 0x000A27001CE1B2D3\n").unwrap();
        let info = DeviceInfo::read(&dir);
        assert!(info.writable().is_ok());
        assert_eq!(info.signing(), Some([0x00, 0x0A, 0x27, 0x00, 0x1C, 0xE1, 0xB2, 0xD3]));
        assert_eq!(info.guid.map(|(_, from)| from), Some("SysInfo"));

        std::fs::write(
            device.join("SysInfoExtended"),
            "<dict><key>FireWireGUID</key><string>000A2700AAAABBBB</string><key>DBVersion</key><integer>3</integer></dict>",
        )
        .unwrap();
        let info = DeviceInfo::read(&dir);
        assert_eq!(info.guid.map(|(_, from)| from), Some("SysInfoExtended"));
        assert_eq!(info.signing(), Some([0x00, 0x0A, 0x27, 0x00, 0xAA, 0xAA, 0xBB, 0xBB]));

        let db = crate::ipod::itunesdb::ItunesDb { version: 0, db_id: 7, language: *b"de", library_id: 1, tracks: Vec::new(), playlists: Vec::new() };
        std::fs::write(itunes.join("iTunesDB"), crate::ipod::itunesdb::writer::write_signed(&db, &info.signing().unwrap())).unwrap();
        let info = DeviceInfo::read(&dir);
        assert!(info.signed_database && !info.later_signature);
        assert!(info.writable().is_ok());

        std::fs::remove_file(device.join("SysInfoExtended")).unwrap();
        std::fs::remove_file(itunes.join("iTunesDB")).unwrap();
        std::fs::write(device.join("SysInfo"), "ModelNumStr: x9460\nFirewireGuid: 0x000A27001CE1B2D3\n").unwrap();
        let info = DeviceInfo::read(&dir);
        assert_eq!(info.generation, Generation::Third);
        assert_eq!(info.signing(), None);
        assert!(info.writable().is_ok());

        std::fs::write(device.join("SysInfo"), "ModelNumStr: xB029\nFirewireGuid: 0x000A27001CE1B2D3\n").unwrap();
        std::fs::write(device.join("SysInfoExtended"), "<dict><key>DBVersion</key><integer>4</integer></dict>").unwrap();
        let info = DeviceInfo::read(&dir);
        assert!(info.later_signature);
        assert_eq!(info.signing(), None);
        assert!(info.writable().unwrap_err().contains("later signature"));
        std::fs::remove_file(device.join("SysInfoExtended")).unwrap();
        let mut later = vec![0u8; 0xF4];
        later[..4].copy_from_slice(b"mhbd");
        later[4..8].copy_from_slice(&0xF4u32.to_le_bytes());
        later[0x30] = 2;
        std::fs::write(itunes.join("iTunesDB"), &later).unwrap();
        assert!(DeviceInfo::read(&dir).writable().is_err());
        std::fs::remove_dir_all(&dir).ok();
    }

    /// On USB the serial is the iPod's own, from its own lines in the tree,
    /// not a hub's above it.
    #[test]
    fn the_usb_serial_is_the_ipods_own() {
        let tree = r#"+-o USB2.0 Hub@02100000  <class IOUSBHostDevice>
  |   "USB Serial Number" = "HUB0000000000001"
  |   "idVendor" = 8457
  |   "idProduct" = 10263
  | +-o iPod@02130000  <class IOUSBHostDevice>
  |   |   "USB Product Name" = "iPod"
  |   |   "idProduct" = 4705
  |   |   "USB Serial Number" = "000A27001CE1B2D3"
  |   |   "idVendor" = 1452
  |   | +-o IOUSBMassStorageDriver
  |   |   |                 "BSD Name" = "disk4"
  | +-o Other@02140000  <class IOUSBHostDevice>
  |   |   "idProduct" = 4609
  |   |   "idVendor" = 1452
  |   |                 "BSD Name" = "disk5"
"#;
        let ipod = usb_in_ioreg(tree, "disk4").unwrap();
        assert_eq!((ipod.vendor, ipod.product), (0x05AC, 0x1261));
        assert_eq!(ipod.serial.as_deref(), Some("000A27001CE1B2D3"));
        assert_eq!(usb_in_ioreg(tree, "disk5").unwrap().serial, None);
    }

    /// The USB numbers name the generations as the public list does; the
    /// third is writable, a classic still wants its signature.
    #[test]
    fn a_usb_number_names_its_generation() {
        assert_eq!(generation_for_usb(0x1201), Some(Generation::Third));
        assert_eq!(generation_for_usb(0x1209), Some(Generation::Video1));
        assert_eq!(generation_for_usb(0x1207), None);
        assert_eq!(Generation::Third.checksum(), Checksum::None);
        assert_eq!(generation_for_usb(0x1261).map(Generation::checksum), Some(Checksum::Hash58));
    }

    /// The iPod behind a hub: its own numbers, not the hub's, which come
    /// first in the tree (as `ioreg` printed it on a Mac with a USB-C hub).
    #[test]
    fn the_ipod_behind_a_hub_is_found_by_its_own_numbers() {
        let tree = r#"+-o USB2.0 Hub@02100000  <class IOUSBHostDevice>
  |   "USB Product Name" = "USB2.0 Hub"
  |   "idVendor" = 8457
  |   "idProduct" = 10263
  | |   |   "USB Product Name" = "Anker USB-C Hub Device"
  | |   |   "idVendor" = 10522
  | |   |   "idProduct" = 33637
  | |   |   "idProduct" = 4609
  | |   |   "USB Product Name" = "iPod USB Interface"
  | |   |   "idVendor" = 1452
  | |   |   |   "USB Device Info" = {"bcdDevice"=0,"idProduct"=4609,"idVendor"=1452}
  | |   |   |   "idVendor" = 1452
  | |   |                 |   "BSD Name" = "disk6"
  | |   |                   | |   "BSD Name" = "disk6s1"
  |     |   "idProduct" = 29280
  |     |   "idVendor" = 8523
"#;
        assert_eq!(usb_in_ioreg(tree, "disk6"), Some(Usb { vendor: 0x05AC, product: 0x1201, serial: None }));
        assert_eq!(usb_in_ioreg(tree, "disk9"), None);
        assert_eq!(generation_for_usb(0x1201), Some(Generation::Third));
    }

    /// A device without its `SysInfo` is unknown; a model said for it is
    /// written back and read as that model, for every model one can choose,
    /// and the file's other lines stay.
    #[test]
    fn a_model_chosen_is_written_and_read_back() {
        let dir = std::env::temp_dir().join(format!("kessler-model-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(DeviceInfo::read(&dir).generation, Generation::Unknown);
        for generation in Generation::CHOOSABLE {
            write_model(&dir, generation).unwrap();
            assert_eq!(DeviceInfo::read(&dir).generation, generation, "{}", generation.label());
        }
        let sysinfo = dir.join("iPod_Control").join("Device").join("SysInfo");
        std::fs::write(&sysinfo, "pszSerialNumber: ABC\nModelNumStr: xZZZZ\n").unwrap();
        write_model(&dir, Generation::Third).unwrap();
        let read = DeviceInfo::read(&dir);
        assert_eq!(read.generation, Generation::Third);
        assert_eq!(read.serial.as_deref(), Some("ABC"));
        std::fs::remove_dir_all(&dir).ok();
    }
}
