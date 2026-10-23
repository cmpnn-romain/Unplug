// Copyright (C) 2026 the Unplug contributors (AUTHORS.md). SPDX-License-Identifier: GPL-3.0-only
//! Reads the release number from the repository's `VERSION` file (the one source of truth) and puts the icon, the version and the request for
//! administrator rights into the Windows executable (turning a device off needs them).

fn main() {
    let version = std::fs::read_to_string("../../../VERSION")
        .map(|v| v.trim().to_string())
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let strict = version.split('.').count() == 3
        && version
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && (p == "0" || !p.starts_with('0')));
    assert!(
        strict,
        "VERSION must be strict MAJOR.MINOR.PATCH (e.g. 0.1.4), got '{version}'. See docs/VERSIONING.md"
    );
    println!("cargo:rerun-if-changed=../../../VERSION");
    println!("cargo:rustc-env=UNPLUG_VERSION={version}");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app.ico");
        res.set("ProductName", "Unplug");
        res.set("FileVersion", &version);
        res.set("ProductVersion", &version);
        res.set("FileDescription", "Unplug");
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>"#,
        );
        if let Err(e) = res.compile() {
            println!("cargo:warning=the icon could not be embedded: {e}");
        }
    }
}
