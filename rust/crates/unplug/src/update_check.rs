// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! Looking for a newer Unplug, and installing it: the Cairn launcher's update, for a single exe. Nothing is downloaded or installed by itself:
//! the window says what is new and asks. The exe is fetched from this repository's own address, checked against the SHA-256 GitHub lists for
//! it, and put in the place of the running one, which is kept beside it until the next start.

use crate::http::Http;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const RELEASES_URL: &str = "https://api.github.com/repos/cmpnn-romain/Unplug/releases?per_page=8";
const DOWNLOAD_PREFIX: &str = "https://github.com/cmpnn-romain/Unplug/releases/download/";
/// The newest release publishes this file; a plain download is not held to the API's 60 requests an hour for one address.
const MANIFEST_URL: &str = "https://github.com/cmpnn-romain/Unplug/releases/latest/download/update.json";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub tag: String,
    pub page_url: String,
    pub download_url: Option<String>,
    pub sha256: Option<String>,
    pub size: i64,
    pub notes: String,
}

/// The numbers of a release tag or of the running version (`v0.3.1`, `0.3.1+abc`: those before any suffix); `None` when it is not `X.Y.Z`.
pub fn normalize(s: &str) -> Option<[u32; 3]> {
    let t = s.trim().trim_start_matches(['v', 'V']);
    let t = t.split(['+', '-']).next().unwrap_or("");
    let parts: Vec<&str> = t.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let mut v = [0u32; 3];
    for (i, p) in parts.iter().enumerate() {
        if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        v[i] = p.parse().ok()?;
    }
    Some(v)
}

/// The newest release in the list that is newer than `current`, with its exe.
pub fn newer(releases_json: &str, current: &str) -> Option<UpdateInfo> {
    let cur = normalize(current)?;
    let list: Value = serde_json::from_str(releases_json).ok()?;
    let mut best: Option<([u32; 3], UpdateInfo)> = None;
    for r in list.as_array()? {
        if r.get("draft") == Some(&Value::Bool(true)) {
            continue;
        }
        let Some(tag) = r.get("tag_name").and_then(Value::as_str) else {
            continue;
        };
        let Some(v) = normalize(tag) else { continue };
        if best.as_ref().is_some_and(|(b, _)| v <= *b) {
            continue;
        }
        let page = r.get("html_url").and_then(Value::as_str).unwrap_or("").to_string();
        let body = r.get("body").and_then(Value::as_str).unwrap_or("");
        let (mut url, mut sha, mut size) = (None, None, 0i64);
        for a in r.get("assets").and_then(Value::as_array).into_iter().flatten() {
            let name = a.get("name").and_then(Value::as_str).unwrap_or("");
            if !name.starts_with("Unplug-") || !name.ends_with(".exe") {
                continue;
            }
            url = a.get("browser_download_url").and_then(Value::as_str).map(str::to_string);
            size = a.get("size").and_then(Value::as_i64).unwrap_or(0);
            sha = a
                .get("digest")
                .and_then(Value::as_str)
                .filter(|d| d.to_lowercase().starts_with("sha256:"))
                .map(|d| d[7..].to_lowercase());
            break;
        }
        // only this repository's own files
        if url
            .as_ref()
            .is_some_and(|u| !u.to_lowercase().starts_with(&DOWNLOAD_PREFIX.to_lowercase()))
        {
            url = None;
        }
        best = Some((
            v,
            UpdateInfo {
                tag: tag.to_string(),
                page_url: page,
                download_url: url,
                sha256: sha,
                size,
                notes: short_notes(body),
            },
        ));
    }
    best.filter(|(v, _)| *v > cur).map(|(_, i)| i)
}

/// The release notes for a message box: no markdown marks, at most ~700 characters.
pub fn short_notes(body: &str) -> String {
    let keep: Vec<String> = body
        .replace('\r', "")
        .split('\n')
        .map(|l| l.replace("**", "").replace('`', "").trim_start_matches(['#', ' ']).to_string())
        .filter(|l| !l.trim().is_empty())
        .collect();
    let s = keep.join("\n");
    if s.chars().count() <= 700 {
        s
    } else {
        format!("{}...", s.chars().take(700).collect::<String>().trim_end())
    }
}

/// What a look for an update found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    Newer(UpdateInfo),
    Current,
    /// GitHub could not be asked (offline, rate limit): "nothing new" was not an answer.
    Unreachable,
}

/// Asks GitHub: the manifest of the newest release first, then the API.
pub fn check(http: &dyn Http, current: &str) -> Check {
    let manifest = http
        .get_with(MANIFEST_URL, Some("application/vnd.github+json"))
        .ok()
        .filter(|r| r.is_success())
        .and_then(|r| r.text(1 << 20).ok())
        .filter(|t| {
            serde_json::from_str::<Value>(t)
                .ok()
                .and_then(|v| v.as_array().map(|a| !a.is_empty()))
                .unwrap_or(false)
        });
    let json = match manifest {
        Some(m) => m,
        None => match http
            .get_with(RELEASES_URL, Some("application/vnd.github+json"))
            .ok()
            .filter(|r| r.is_success())
            .and_then(|r| r.text(4 << 20).ok())
        {
            Some(j) => j,
            None => return Check::Unreachable,
        },
    };
    match newer(&json, current) {
        Some(i) => Check::Newer(i),
        None => Check::Current,
    }
}

fn update_dir() -> PathBuf {
    std::env::temp_dir().join("Unplug-update")
}

/// Downloads the exe into the temp folder and checks it; `Ok(path)` or why not.
pub fn download(info: &UpdateInfo, http: &dyn Http, progress: &mut dyn FnMut(f64), cancel: &AtomicBool) -> Result<PathBuf, String> {
    let (Some(url), Some(want)) = (&info.download_url, &info.sha256) else {
        return Err("this release has no exe file with a checksum".into());
    };
    let dir = update_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join(format!(
        "Unplug-{}.exe",
        info.tag
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '-')
            .collect::<String>()
    ));
    let result = (|| {
        let resp = http.get(url).map_err(|e| format!("the download failed ({e})"))?;
        if !resp.is_success() {
            return Err(format!("the download server answered {}", resp.status));
        }
        let total = resp.content_length.map(|n| n as i64).unwrap_or(info.size).max(1);
        let mut src = resp.body;
        let mut dst = fs::File::create(&file).map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let (mut buf, mut got) = (vec![0u8; 256 * 1024], 0i64);
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("cancelled".into());
            }
            let n = src.read(&mut buf).map_err(|e| format!("the download failed ({e})"))?;
            if n == 0 {
                break;
            }
            hash.update(&buf[..n]);
            dst.write_all(&buf[..n]).map_err(|e| e.to_string())?;
            got += n as i64;
            progress((got as f64 / total as f64).min(1.0));
        }
        let have: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        if !have.eq_ignore_ascii_case(want) {
            return Err("the downloaded file does not match its checksum".into());
        }
        Ok(file.clone())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&file);
    }
    result
}

/// Where the running exe waits after it was replaced.
fn old_copy(exe: &Path) -> PathBuf {
    exe.with_extension("exe.old")
}

/// Puts `downloaded` in the place of `exe`. A running exe can be renamed but not overwritten, so it is renamed first; if the new one cannot be put in
/// its place the old one is given back.
pub fn replace(exe: &Path, downloaded: &Path) -> Result<(), String> {
    let old = old_copy(exe);
    let _ = fs::remove_file(&old);
    fs::rename(exe, &old).map_err(|e| e.to_string())?;
    if let Err(e) = fs::copy(downloaded, exe) {
        let _ = fs::remove_file(exe);
        let _ = fs::rename(&old, exe);
        return Err(e.to_string());
    }
    let _ = fs::remove_file(downloaded);
    Ok(())
}

/// Replaces this exe with `downloaded` and starts the new one (`--relaunch`: it waits for this one to end). The caller ends this process.
pub fn install(downloaded: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    replace(&exe, downloaded)?;
    std::process::Command::new(&exe)
        .arg("--relaunch")
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// At start: removes what the last update left (the old exe and the download).
pub fn clean_up() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = fs::remove_file(old_copy(&exe));
    }
    let _ = fs::remove_dir_all(update_dir());
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::fake::FakeHttp;

    fn release(tag: &str, sha: &str, url_tag: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","html_url":"https://github.com/cmpnn-romain/Unplug/releases/tag/{tag}","body":"- A change\n- **Bold** `code`","assets":[{{"name":"Unplug-{tag}.exe","size":1000,"digest":"sha256:{sha}","browser_download_url":"https://github.com/cmpnn-romain/Unplug/releases/download/{url_tag}/Unplug-{tag}.exe"}}]}}"#
        )
    }

    #[test]
    fn versions_read_the_numbers_before_any_suffix() {
        assert_eq!(normalize("v0.3.38"), Some([0, 3, 38]));
        assert_eq!(normalize("0.3.38+abc123"), Some([0, 3, 38]));
        assert_eq!(normalize("0.3.38-beta"), Some([0, 3, 38]));
        assert!(normalize("nonsense").is_none() && normalize("").is_none() && normalize("1.2").is_none());
    }

    #[test]
    fn the_newest_release_newer_than_this_one_is_offered_with_its_exe() {
        let sha = "ab".repeat(32);
        let json = format!(
            "[{},{},{}]",
            release("0.3.9", &sha, "0.3.9"),
            release("0.3.40", &sha, "0.3.40"),
            release("0.3.38", &sha, "0.3.38")
        );
        let i = newer(&json, "0.3.38").unwrap();
        assert_eq!((i.tag.as_str(), i.size), ("0.3.40", 1000));
        assert_eq!(i.sha256.as_deref(), Some(sha.as_str()));
        assert!(i.download_url.unwrap().ends_with("0.3.40/Unplug-0.3.40.exe"));
        assert_eq!(i.notes, "- A change\n- Bold code");
        assert!(newer(&json, "0.3.40").is_none() && newer(&json, "0.4.0+x").is_none());
        assert!(newer(&json, "garbage").is_none() && newer("not json", "0.1.0").is_none());
    }

    #[test]
    fn an_exe_from_another_address_is_not_used() {
        let json = format!(
            "[{}]",
            release("0.5.0", &"cd".repeat(32), "0.5.0")
                .replace("https://github.com/cmpnn-romain/Unplug/releases/download/", "https://evil.test/")
        );
        assert!(newer(&json, "0.1.0").unwrap().download_url.is_none());
        assert!(newer(r#"[{"tag_name":"0.5.0","draft":true}]"#, "0.1.0").is_none());
    }

    #[test]
    fn a_check_uses_the_manifest_then_the_api_and_says_when_it_could_not_ask() {
        let sha = "ab".repeat(32);
        let h = FakeHttp::serving(MANIFEST_URL, format!("[{}]", release("0.9.0", &sha, "0.9.0")).as_bytes());
        assert!(matches!(check(&h, "0.3.38"), Check::Newer(i) if i.tag == "0.9.0"));
        assert_eq!(check(&h, "0.9.0"), Check::Current);
        let api = FakeHttp::serving(RELEASES_URL, format!("[{}]", release("0.8.0", &sha, "0.8.0")).as_bytes());
        assert!(matches!(check(&api, "0.3.38"), Check::Newer(i) if i.tag == "0.8.0"));
        let empty_manifest = FakeHttp::serving(MANIFEST_URL, b"[]");
        empty_manifest.put(RELEASES_URL, 200, format!("[{}]", release("0.8.0", &sha, "0.8.0")).as_bytes());
        assert!(matches!(check(&empty_manifest, "0.1.0"), Check::Newer(_)));
        assert_eq!(check(&FakeHttp::default(), "0.3.38"), Check::Unreachable);
    }

    #[test]
    fn the_exe_is_downloaded_checked_and_a_wrong_one_is_deleted() {
        let body = b"pretend exe".to_vec();
        let sha: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
        let url = "https://github.com/cmpnn-romain/Unplug/releases/download/9.9.9/Unplug-9.9.9.exe";
        let h = FakeHttp::serving(url, &body);
        let info = UpdateInfo {
            tag: "9.9.9".into(),
            page_url: String::new(),
            download_url: Some(url.into()),
            sha256: Some(sha),
            size: body.len() as i64,
            notes: String::new(),
        };
        let mut seen = 0.0;
        let path = download(&info, &h, &mut |p| seen = p, &AtomicBool::new(false)).unwrap();
        assert_eq!((fs::read(&path).unwrap(), seen), (body.clone(), 1.0));
        let _ = fs::remove_file(&path);
        let wrong = UpdateInfo {
            sha256: Some("0".repeat(64)),
            ..info.clone()
        };
        assert_eq!(
            download(&wrong, &h, &mut |_| {}, &AtomicBool::new(false)).unwrap_err(),
            "the downloaded file does not match its checksum"
        );
        assert!(!update_dir().join("Unplug-9.9.9.exe").exists());
        let no_sum = UpdateInfo {
            sha256: None,
            ..info.clone()
        };
        assert!(download(&no_sum, &h, &mut |_| {}, &AtomicBool::new(false)).is_err());
        assert_eq!(download(&info, &h, &mut |_| {}, &AtomicBool::new(true)).unwrap_err(), "cancelled");
        assert!(download(&info, &FakeHttp::default(), &mut |_| {}, &AtomicBool::new(false))
            .unwrap_err()
            .contains("404"));
    }

    #[test]
    fn the_new_exe_takes_the_place_of_the_old_one_which_is_kept_beside_it() {
        let dir = std::env::temp_dir().join(format!("unplug-replace-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let (exe, new) = (dir.join("Unplug.exe"), dir.join("download.exe"));
        fs::write(&exe, b"old").unwrap();
        fs::write(&new, b"new").unwrap();
        replace(&exe, &new).unwrap();
        assert_eq!(fs::read(&exe).unwrap(), b"new");
        assert_eq!(fs::read(old_copy(&exe)).unwrap(), b"old");
        assert!(!new.exists());
        // a download that is gone gives the old exe back
        assert!(replace(&exe, &dir.join("missing.exe")).is_err());
        assert_eq!(fs::read(&exe).unwrap(), b"new");
        let _ = fs::remove_dir_all(dir);
    }
}
