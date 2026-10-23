/// Only the fixed provider setup link may leave the app via a new window.
pub fn allowed(url: &str) -> bool {
    url == "https://platform.deepseek.com/"
}

#[cfg(target_os = "macos")]
pub fn open(url: &str) {
    if allowed(url) {
        if let Err(error) = std::process::Command::new("/usr/bin/open").arg(url).spawn() {
            eprintln!("momo: cannot open model provider: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn allows_only_the_provider_setup_page() {
        assert!(super::allowed("https://platform.deepseek.com/"));
        for value in [
            "http://platform.deepseek.com/",
            "https://platform.deepseek.com.evil.test/",
            "https://platform.deepseek.com/?redirect=evil",
            "file:///tmp/test",
            "https://user@platform.deepseek.com/",
        ] {
            assert!(!super::allowed(value));
        }
    }
}
