#[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd"))]
fn main() -> anyhow::Result<()> {
    use clap::Parser;
    use mangowm_mod::cli::{run_cli, Cli};

    // Exit quietly on a closed pipe (`mangowm-mod schema | head`) instead of panicking.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }

    let cli = Cli::parse();
    run_cli(cli)
}

/// MangoWM only runs on Linux and the BSDs it is packaged for.
#[cfg(not(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd")))]
fn main() {
    eprintln!(
        "mangowm-mod: MangoWM is a Wayland compositor for Linux (and the BSDs); this system is not supported."
    );
    std::process::exit(1);
}
