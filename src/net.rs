use std::process::Command;

pub fn curl() -> Command {
    let mut command = Command::new("curl");
    command.args(["--proto", "=http,https", "--proto-redir", "=http,https"]);
    command
}

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
