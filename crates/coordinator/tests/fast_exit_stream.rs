// Regression: the launcher used tokio child pipes to stream game output. On
// Windows, when the JVM crashes right after writing to stderr, tokio lost
// everything but the first line (67 of 950 bytes); sync reads via std threads
// capture the full output (cross-checked against .NET ReadToEnd).
// Requires the JRE and the `klñklkjkl` instance at the paths below.

const JAVAW: &str = "C:\\Program Files\\Java\\jre1.8.0_501\\bin\\javaw.exe";
const INSTANCE: &str =
    "C:\\Users\\llamonega\\AppData\\Roaming\\release-the-launcher\\instances\\klñklkjkl";

fn read_lines<R: std::io::Read>(r: R) -> usize {
    use std::io::BufRead;
    // keep filter_map, NOT map_while (clippy's suggestion): on
    // Windows a mid-stream broken-pipe Err is followed by buffered lines, and
    // map_while stops at the first Err, silently dropping them.
    #[allow(clippy::lines_filter_map_ok)]
    std::io::BufReader::new(r)
        .lines()
        .filter_map(Result::ok)
        .count()
}

#[test]
fn game_fast_exit_stderr_not_lost() {
    let cp = format!(
        "{INSTANCE}\\libraries\\net/minecraftforge\\forge\\1.5.2-7.8.1.738\\forge-1.5.2-7.8.1.738-universal.zip;{INSTANCE}\\libraries\\net/minecraft\\launchwrapper\\1.5\\launchwrapper-1.5.jar;{INSTANCE}\\libraries\\net/java/jinput\\jinput\\2.0.5\\jinput-2.0.5.jar;{INSTANCE}\\libraries\\org/ow2/asm\\asm-all\\4.1\\asm-all-4.1.jar;{INSTANCE}\\libraries\\net/java/jutils\\jutils\\1.0.0\\jutils-1.0.0.jar;{INSTANCE}\\libraries\\net/sf/jopt-simple\\jopt-simple\\4.5\\jopt-simple-4.5.jar;{INSTANCE}\\versions\\1.5.2\\1.5.2.jar"
    );
    let mut cmd = std::process::Command::new(JAVAW);
    cmd.args(["-Xms1G", "-Xmx2G"])
        .arg(format!("-Djava.library.path={INSTANCE}\\natives"))
        .arg(format!("-Dorg.lwjgl.librarypath={INSTANCE}\\natives"))
        .arg("-cp")
        .arg(&cp)
        .args([
            "net.minecraft.launchwrapper.Launch",
            "momorelojero",
            "0",
            "--gameDir",
            &format!("{INSTANCE}\\.minecraft"),
            "--assetsDir",
            &format!("{INSTANCE}\\assets"),
            "--tweakClass",
            "cpw.mods.fml.common.launcher.FMLTweaker",
            "--width",
            "854",
            "--height",
            "480",
        ]);
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let out = child.stdout.take().unwrap();
    let err = child.stderr.take().unwrap();
    let out_handle = std::thread::spawn(move || read_lines(out));
    let err_handle = std::thread::spawn(move || read_lines(err));
    let status = child.wait().unwrap();
    let out_n = out_handle.join().unwrap();
    let err_n = err_handle.join().unwrap();
    assert!(status.success(), "game exited with {status}");
    assert_eq!(out_n, 0, "expected no stdout output");
    assert!(
        err_n >= 8,
        "stderr lost lines on fast exit: {err_n} (expected the full JUL stack)"
    );
}
