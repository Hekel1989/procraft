//! Downloads a file with a resumable, verified transfer: `<file>.part` holds the bytes until the
//! size and SHA-256 match, then it becomes `<file>`.

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use sha2::{Digest, Sha256};

/// How much is read at a time.
const CHUNK: usize = 64 * 1024;
/// How often progress goes to stderr.
const PROGRESS_EVERY: Duration = Duration::from_millis(500);

/// Downloads `url` to `dest`, resuming a `.part` file when the server allows it. A file already
/// there with the right size and hash is left alone.
pub fn download(url: &str, dest: &Path, size: u64, sha256: &str) -> Result<(), String> {
    let name = file_name(dest);
    if dest.is_file() {
        let present = fs::metadata(dest).map(|m| m.len()).unwrap_or(u64::MAX);
        if present == size && sha256_file(dest)? == sha256 {
            eprintln!("{name}: already there");
            return Ok(());
        }
    }

    let part = part_path(dest);
    let mut offset = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if offset > size {
        remove(&part)?;
        offset = 0;
    }
    if offset == size {
        if sha256_file(&part)? == sha256 {
            place(&part, dest)?;
            eprintln!("{name}: already downloaded, verified");
            return Ok(());
        }
        remove(&part)?;
        offset = 0;
    }

    let agent = ureq::Agent::config_builder().http_status_as_error(false).build().new_agent();
    let mut request = agent.get(url).header("Accept-Encoding", "identity");
    if offset > 0 {
        request = request.header("Range", &format!("bytes={offset}-"));
    }
    let mut response = request.call().map_err(|e| format!("{url}: {e}"))?;
    let status = response.status().as_u16();
    let (mut file, mut hasher) = match status {
        206 if offset > 0 => {
            let mut hasher = Sha256::new();
            hash_into(&part, &mut hasher)?;
            let file = OpenOptions::new().append(true).open(&part).map_err(|e| format!("{}: {e}", part.display()))?;
            (file, hasher)
        }
        200 => {
            // a fresh download, or the server ignored the Range and sent everything again
            offset = 0;
            (File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?, Sha256::new())
        }
        _ => return Err(format!("{url}: HTTP {status}")),
    };

    let mut done = offset;
    let mut buffer = vec![0u8; CHUNK];
    let mut last = Instant::now();
    let mut reader = response.body_mut().as_reader();
    loop {
        let read = reader.read(&mut buffer).map_err(|e| format!("{url}: {e}"))?;
        if read == 0 {
            break;
        }
        let Some(chunk) = buffer.get(..read) else { return Err("read past the buffer".to_owned()) };
        file.write_all(chunk).map_err(|e| format!("{}: {e}", part.display()))?;
        hasher.update(chunk);
        done += read as u64;
        if last.elapsed() >= PROGRESS_EVERY {
            progress(&name, done, size);
            last = Instant::now();
        }
    }
    file.flush().map_err(|e| format!("{}: {e}", part.display()))?;
    drop(file);

    if done != size {
        // a short part can resume next time; a long one can't
        if done > size {
            remove(&part)?;
        }
        return Err(format!("{name}: got {done} bytes, expected {size}"));
    }
    let got = hex(hasher.finalize());
    if got != sha256 {
        remove(&part)?;
        return Err(format!("{name}: SHA-256 is {got}, expected {sha256}"));
    }
    place(&part, dest)?;
    progress(&name, size, size);
    eprintln!();
    Ok(())
}

/// Downloads `url` to `dest` without a size or hash to check: for archives whose pins don't include
/// one. Bytes wait in `<file>.part` and are renamed into place once the transfer finishes.
pub fn download_unverified(url: &str, dest: &Path) -> Result<(), String> {
    let name = file_name(dest);
    if dest.is_file() {
        eprintln!("{name}: already there");
        return Ok(());
    }

    let part = part_path(dest);
    let mut offset = fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    let agent = ureq::Agent::config_builder().http_status_as_error(false).build().new_agent();
    let mut request = agent.get(url).header("Accept-Encoding", "identity");
    if offset > 0 {
        request = request.header("Range", &format!("bytes={offset}-"));
    }
    let mut response = request.call().map_err(|e| format!("{url}: {e}"))?;
    let status = response.status().as_u16();
    let mut file = match status {
        206 if offset > 0 => {
            OpenOptions::new().append(true).open(&part).map_err(|e| format!("{}: {e}", part.display()))?
        }
        200 => {
            // a fresh download, or the server ignored the Range and sent everything again
            offset = 0;
            File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?
        }
        416 if offset > 0 => {
            // the part may already hold the whole archive, but there is no hash to tell; start over
            remove(&part)?;
            return Err(format!("{url}: the partial download is unusable, run install again"));
        }
        _ => return Err(format!("{url}: HTTP {status}")),
    };

    let mut done = offset;
    let mut buffer = vec![0u8; CHUNK];
    let mut reader = response.body_mut().as_reader();
    loop {
        let read = reader.read(&mut buffer).map_err(|e| format!("{url}: {e}"))?;
        if read == 0 {
            break;
        }
        let Some(chunk) = buffer.get(..read) else { return Err("read past the buffer".to_owned()) };
        file.write_all(chunk).map_err(|e| format!("{}: {e}", part.display()))?;
        done += read as u64;
    }
    file.flush().map_err(|e| format!("{}: {e}", part.display()))?;
    drop(file);

    if done == 0 {
        remove(&part)?;
        return Err(format!("{name}: the download was empty"));
    }
    place(&part, dest)?;
    eprintln!("{name}: downloaded ({done} bytes)");
    Ok(())
}

/// The SHA-256 of `path`, in lowercase hex.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hash_into(path, &mut hasher)?;
    Ok(hex(hasher.finalize()))
}

/// `<dest>.part`: where the bytes wait until they are verified.
fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest.as_os_str().to_owned();
    name.push(".part");
    PathBuf::from(name)
}

fn file_name(path: &Path) -> String {
    path.file_name().and_then(|name| name.to_str()).unwrap_or("download").to_owned()
}

fn hash_into(path: &Path, hasher: &mut Sha256) -> Result<(), String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut buffer = [0u8; CHUNK];
    loop {
        let read = file.read(&mut buffer).map_err(|e| format!("{}: {e}", path.display()))?;
        if read == 0 {
            break;
        }
        if let Some(chunk) = buffer.get(..read) {
            hasher.update(chunk);
        }
    }
    Ok(())
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write as _;
    let mut text = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        let _ = write!(&mut text, "{byte:02x}");
    }
    text
}

fn remove(path: &Path) -> Result<(), String> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

fn place(part: &Path, dest: &Path) -> Result<(), String> {
    if dest.exists() {
        fs::remove_file(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    }
    fs::rename(part, dest).map_err(|e| format!("{} -> {}: {e}", part.display(), dest.display()))
}

fn progress(name: &str, done: u64, size: u64) {
    let percent = if size == 0 { 100.0 } else { done as f64 * 100.0 / size as f64 };
    eprint!("\r{name}: {percent:.1}% ({:.1} / {:.1} MB)", done as f64 / 1_000_000.0, size as f64 / 1_000_000.0);
    let _ = std::io::stderr().flush();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::{SocketAddr, TcpListener};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::thread::{self, JoinHandle};

    struct Server {
        address: SocketAddr,
        thread: JoinHandle<()>,
        saw_range: Arc<AtomicBool>,
    }

    impl Server {
        /// A server for one request. With `ranges`, a `Range` header gets a 206 from that offset;
        /// without it, every request gets the whole body with 200.
        fn serve(body: Vec<u8>, ranges: bool) -> Server {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let saw_range = Arc::new(AtomicBool::new(false));
            let seen = Arc::clone(&saw_range);
            let thread = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut start = 0;
                loop {
                    let mut line = String::new();
                    reader.read_line(&mut line).unwrap();
                    if line == "\r\n" || line == "\n" || line.is_empty() {
                        break;
                    }
                    let lower = line.to_ascii_lowercase();
                    if let Some(value) = lower.strip_prefix("range: bytes=") {
                        seen.store(true, Ordering::SeqCst);
                        start = value.trim().trim_end_matches('-').parse::<usize>().unwrap_or(0);
                    }
                }
                let partial = ranges && start > 0;
                if !ranges {
                    start = 0;
                }
                let bytes = &body[start..];
                let status = if partial { "206 Partial Content" } else { "200 OK" };
                let mut header =
                    format!("HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n", bytes.len());
                if partial {
                    header.push_str(&format!(
                        "Content-Range: bytes {}-{}/{}\r\n",
                        start,
                        body.len().saturating_sub(1),
                        body.len()
                    ));
                }
                header.push_str("\r\n");
                stream.write_all(header.as_bytes()).unwrap();
                stream.write_all(bytes).unwrap();
                stream.flush().unwrap();
            });
            Server { address, thread, saw_range }
        }

        fn url(&self) -> String {
            format!("http://{}/model.safetensors", self.address)
        }

        fn join(self) -> bool {
            self.thread.join().unwrap();
            self.saw_range.load(Ordering::SeqCst)
        }
    }

    fn temp_dir() -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "procraft-companion-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn sha(bytes: &[u8]) -> String {
        hex(Sha256::digest(bytes).as_slice())
    }

    #[test]
    fn a_fresh_download_lands_verified() {
        let body = b"a small model for the test server".to_vec();
        let server = Server::serve(body.clone(), true);
        let dir = temp_dir();
        let dest = dir.join("model.safetensors");

        download(&server.url(), &dest, body.len() as u64, &sha(&body)).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
        assert!(!server.join());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_part_file_resumes_with_a_range() {
        let body = b"0123456789abcdefghijklmnopqrstuvwxyz".to_vec();
        let server = Server::serve(body.clone(), true);
        let dir = temp_dir();
        let dest = dir.join("model.safetensors");
        fs::write(part_path(&dest), &body[..11]).unwrap();

        download(&server.url(), &dest, body.len() as u64, &sha(&body)).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
        assert!(server.join());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_server_that_ignores_the_range_restarts() {
        let body = b"the whole file, from the beginning".to_vec();
        let server = Server::serve(body.clone(), false);
        let dir = temp_dir();
        let dest = dir.join("model.safetensors");
        fs::write(part_path(&dest), b"stale bytes").unwrap();

        download(&server.url(), &dest, body.len() as u64, &sha(&body)).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
        assert!(server.join());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wrong_hash_is_rejected_and_nothing_is_left() {
        let body = b"not what the pin says".to_vec();
        let server = Server::serve(body.clone(), true);
        let dir = temp_dir();
        let dest = dir.join("model.safetensors");

        let result = download(&server.url(), &dest, body.len() as u64, "00");

        assert!(result.is_err(), "{result:?}");
        assert!(!dest.exists());
        assert!(!part_path(&dest).exists());
        server.join();
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unverified_download_lands_through_a_part_file() {
        let body = b"an archive with no pinned hash".to_vec();
        let server = Server::serve(body.clone(), true);
        let dir = temp_dir();
        let dest = dir.join("archive.tar.gz");

        download_unverified(&server.url(), &dest).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
        assert!(!server.join());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn an_unverified_part_file_resumes_with_a_range() {
        let body = b"the rest of an archive".to_vec();
        let server = Server::serve(body.clone(), true);
        let dir = temp_dir();
        let dest = dir.join("archive.tar.gz");
        fs::write(part_path(&dest), &body[..10]).unwrap();

        download_unverified(&server.url(), &dest).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
        assert!(!part_path(&dest).exists());
        assert!(server.join());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn a_verified_file_is_skipped() {
        let body = b"already downloaded".to_vec();
        let dir = temp_dir();
        let dest = dir.join("model.safetensors");
        fs::write(&dest, &body).unwrap();

        // no server: reaching the network would fail
        download("http://127.0.0.1:1/nope", &dest, body.len() as u64, &sha(&body)).unwrap();

        assert_eq!(fs::read(&dest).unwrap(), body);
        let _ = fs::remove_dir_all(dir);
    }
}
