//! How kessler goes out to the network: through curl, always the same way.
//!
//! Many of the addresses it asks come from elsewhere — a podcast's feed, a
//! station directory, a search answer — and are handed to curl as an
//! argument. Two things keep such an address from being more than an
//! address:
//!
//! - Only `http` and `https`, also where a server redirects. curl reads
//!   `file://` too, and a feed naming a picture `file:///…` would otherwise
//!   have a file of this machine read in as if it were that picture.
//! - The address after `--`, so one beginning with a dash is not taken as one
//!   of curl's own options (`-o…` would write the answer wherever it said).
//!
//! Certificates are checked as curl checks them by default; nothing here
//! turns that off.

use std::process::Command;

/// curl, held to `http` and `https`, before and after any redirect. The
/// address is added with [`address`], last.
pub fn curl() -> Command {
    let mut command = Command::new("curl");
    command.args(["--proto", "=http,https", "--proto-redir", "=http,https"]);
    command
}

/// The address, after the end of the options — the last argument.
pub fn address<'a>(command: &'a mut Command, url: &str) -> &'a mut Command {
    command.arg("--").arg(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(command: &Command) -> Vec<String> {
        command.get_args().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn an_address_is_never_an_option() {
        let mut command = curl();
        address(&mut command, "-o/tmp/somewhere");
        let args = args(&command);
        assert_eq!(&args[args.len() - 2..], ["--", "-o/tmp/somewhere"]);
        assert!(args.windows(2).any(|w| w == ["--proto", "=http,https"]));
        assert!(args.windows(2).any(|w| w == ["--proto-redir", "=http,https"]));
    }
}
