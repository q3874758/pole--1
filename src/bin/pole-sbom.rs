//! pole-sbom — generate a CycloneDX 1.5 SBOM and run a basic
//! license-compliance audit against the workspace's dependency
//! tree.
//!
//! Usage:
//!     pole-sbom --out sbom.cdx.json
//!     pole-sbom --format spdx --out sbom.spdx.json
//!     pole-sbom --deny-licenses GPL-3.0-only,GPL-2.0-only
//!
//! The tool uses the `cargo_metadata` crate to obtain the resolved
//! dependency tree, then emits a deduplicated component list.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

use cargo_metadata::{Metadata, MetadataCommand, Package};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    CycloneDx,
    Spdx,
}

impl Format {
    fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "cyclonedx" | "cdx" => Some(Self::CycloneDx),
            "spdx" => Some(Self::Spdx),
            _ => None,
        }
    }
}

#[derive(Serialize)]
struct CycloneDxBom {
    #[serde(rename = "bomFormat")]
    bom_format: &'static str,
    #[serde(rename = "specVersion")]
    spec_version: &'static str,
    version: u32,
    #[serde(rename = "serialNumber")]
    serial_number: String,
    components: Vec<CycloneDxComponent>,
}

#[derive(Serialize)]
struct CycloneDxComponent {
    #[serde(rename = "type")]
    kind: &'static str,
    name: String,
    version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    licenses: Option<Vec<CycloneDxLicense>>,
    purl: String,
    #[serde(rename = "bom-ref")]
    bom_ref: String,
}

#[derive(Serialize)]
struct CycloneDxLicense {
    license: CycloneDxLicenseId,
}

#[derive(Serialize)]
struct CycloneDxLicenseId {
    id: String,
}

#[derive(Serialize)]
struct SpdxDocument {
    #[serde(rename = "spdxVersion")]
    spdx_version: &'static str,
    #[serde(rename = "dataLicense")]
    data_license: &'static str,
    #[serde(rename = "SPDXID")]
    spdx_id: &'static str,
    name: &'static str,
    packages: Vec<SpdxPackage>,
}

#[derive(Serialize)]
struct SpdxPackage {
    #[serde(rename = "SPDXID")]
    spdx_id: String,
    name: String,
    #[serde(rename = "versionInfo")]
    version_info: String,
    #[serde(rename = "downloadLocation")]
    download_location: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "licenseConcluded")]
    license_concluded: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename = "licenseDeclared")]
    license_declared: Option<String>,
}

struct Args {
    manifest_path: Option<PathBuf>,
    out: Option<PathBuf>,
    format: Format,
    deny_licenses: Vec<String>,
    warn_licenses: Vec<String>,
    show_help: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        manifest_path: None,
        out: None,
        format: Format::CycloneDx,
        deny_licenses: Vec::new(),
        warn_licenses: Vec::new(),
        show_help: false,
    };
    let mut iter = std::env::args().skip(1);
    while let Some(a) = iter.next() {
        match a.as_str() {
            "-h" | "--help" => args.show_help = true,
            "--manifest-path" => {
                args.manifest_path = iter.next().map(PathBuf::from);
            }
            "--out" => {
                args.out = iter.next().map(PathBuf::from);
            }
            "--format" => {
                let v = iter.next().ok_or("--format requires a value")?;
                args.format = Format::parse(&v)
                    .ok_or_else(|| format!("unknown format '{v}' (use cyclonedx or spdx)"))?;
            }
            "--deny-licenses" => {
                let v = iter.next().ok_or("--deny-licenses requires a value")?;
                args.deny_licenses = v.split(',').map(|s| s.trim().to_string()).collect();
            }
            "--warn-licenses" => {
                let v = iter.next().ok_or("--warn-licenses requires a value")?;
                args.warn_licenses = v.split(',').map(|s| s.trim().to_string()).collect();
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    Ok(args)
}

fn print_help() {
    eprintln!("pole-sbom — generate a SBOM (CycloneDX or SPDX) for the workspace");
    eprintln!();
    eprintln!("USAGE:");
    eprintln!("    pole-sbom [--out FILE] [--format cyclonedx|spdx] [--manifest-path PATH]");
    eprintln!("              [--deny-licenses A,B] [--warn-licenses A,B]");
    eprintln!();
    eprintln!("FLAGS:");
    eprintln!("    --out FILE             write SBOM to FILE (default: stdout)");
    eprintln!("    --format FMT           'cyclonedx' (default) or 'spdx'");
    eprintln!("    --manifest-path PATH   path to Cargo.toml (default: auto)");
    eprintln!("    --deny-licenses LIST   comma-separated SPDX IDs to deny");
    eprintln!("    --warn-licenses LIST   comma-separated SPDX IDs to flag");
}

fn load_metadata(manifest_path: Option<&PathBuf>) -> Result<Metadata, String> {
    let mut cmd = MetadataCommand::new();
    if let Some(p) = manifest_path {
        cmd.manifest_path(p);
    }
    cmd.exec()
        .map_err(|e| format!("cargo metadata failed: {e}"))
}

fn resolve_license(pkg: &Package) -> Option<String> {
    if let Some(s) = pkg.license.as_ref().filter(|s| !s.trim().is_empty()) {
        return Some(s.clone());
    }
    pkg.license_file.as_ref().map(|p| {
        p.file_name()
            .map(|n| n.to_string())
            .unwrap_or_else(|| p.to_string())
    })
}

fn license_tokens(license: &str) -> Vec<String> {
    license
        .split(|c: char| c == '/' || c == ' ' || c == '(' || c == ')')
        .filter_map(|tok| {
            let t = tok.trim().trim_end_matches(')').trim_start_matches('(');
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        })
        .collect()
}

fn build_components(metadata: &Metadata) -> Vec<(Package, Option<String>)> {
    let mut seen: BTreeMap<String, Package> = BTreeMap::new();
    for pkg in &metadata.packages {
        let key = format!("{}@{}", pkg.name, pkg.version);
        seen.entry(key).or_insert_with(|| pkg.clone());
    }
    seen.into_values()
        .map(|p| (p.clone(), resolve_license(&p)))
        .collect()
}

fn render_cyclonedx(items: &[(Package, Option<String>)]) -> String {
    let bom = CycloneDxBom {
        bom_format: "CycloneDX",
        spec_version: "1.5",
        version: 1,
        serial_number: format!("urn:uuid:{}", stable_serial()),
        components: items
            .iter()
            .map(|(pkg, lic)| {
                let version = pkg.version.to_string();
                let name = pkg.name.to_string();
                let bom_ref = format!("{}@{}", name, version);
                let licenses = lic.as_ref().map(|l| {
                    vec![CycloneDxLicense {
                        license: CycloneDxLicenseId { id: l.clone() },
                    }]
                });
                CycloneDxComponent {
                    kind: "library",
                    name,
                    version: version.clone(),
                    licenses,
                    purl: format!("pkg:cargo/{}@{}", pkg.name, version),
                    bom_ref,
                }
            })
            .collect(),
    };
    serde_json::to_string_pretty(&bom).unwrap_or_default()
}

fn render_spdx(items: &[(Package, Option<String>)]) -> String {
    let doc = SpdxDocument {
        spdx_version: "SPDX-2.3",
        data_license: "CC0-1.0",
        spdx_id: "SPDXRef-DOCUMENT",
        name: "pole-sbom",
        packages: items
            .iter()
            .map(|(pkg, lic)| {
                let version = pkg.version.to_string();
                let name = pkg.name.to_string();
                let spdx_id = format!("SPDXRef-Package-{}-{}", name, version).replace(' ', "-");
                let lic_str = lic.clone();
                SpdxPackage {
                    spdx_id,
                    name,
                    version_info: version,
                    download_location: "NOASSERTION",
                    license_concluded: lic_str.clone(),
                    license_declared: lic_str,
                }
            })
            .collect(),
    };
    serde_json::to_string_pretty(&doc).unwrap_or_default()
}

fn stable_serial() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let hex = format!("{nanos:032x}");
    // Lay out as 8-4-4-4-12 (UUID-ish).
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

fn audit_licenses(
    items: &[(Package, Option<String>)],
    deny: &[String],
    warn: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut denials = Vec::new();
    let mut warnings = Vec::new();
    for (pkg, lic) in items {
        let Some(lic) = lic else {
            warnings.push(format!("{}@{}: no license declared", pkg.name, pkg.version));
            continue;
        };
        let tokens = license_tokens(lic);
        for d in deny {
            if tokens.iter().any(|t| t.eq_ignore_ascii_case(d)) {
                denials.push(format!(
                    "{}@{}: license '{}' denied",
                    pkg.name, pkg.version, d
                ));
            }
        }
        for w in warn {
            if tokens.iter().any(|t| t.eq_ignore_ascii_case(w)) {
                warnings.push(format!(
                    "{}@{}: license '{}' is in warn list",
                    pkg.name, pkg.version, w
                ));
            }
        }
    }
    (denials, warnings)
}

fn run() -> Result<i32, String> {
    let args = parse_args()?;
    if args.show_help {
        print_help();
        return Ok(0);
    }
    let metadata = load_metadata(args.manifest_path.as_ref())?;
    let items = build_components(&metadata);

    let body = match args.format {
        Format::CycloneDx => render_cyclonedx(&items),
        Format::Spdx => render_spdx(&items),
    };

    match args.out.as_ref() {
        Some(p) => fs::write(p, &body).map_err(|e| format!("write {}: {e}", p.display()))?,
        None => println!("{body}"),
    }

    let (denials, warnings) = audit_licenses(&items, &args.deny_licenses, &args.warn_licenses);
    if !warnings.is_empty() {
        eprintln!("# License warnings ({}):", warnings.len());
        for w in &warnings {
            eprintln!("  - {w}");
        }
    }
    if !denials.is_empty() {
        eprintln!("# License denials ({}):", denials.len());
        for d in &denials {
            eprintln!("  - {d}");
        }
        return Ok(2);
    }
    Ok(0)
}

fn main() -> ExitCode {
    match run() {
        Ok(0) => ExitCode::SUCCESS,
        Ok(code) => ExitCode::from(code as u8),
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
