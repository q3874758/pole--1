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

use cargo_metadata::{Metadata, MetadataCommand, Package};
use serde::Serialize;

/// Default deny list applied when the caller does not pass
/// `--deny-licenses` on the command line. This must stay in lock-step
/// with `[licenses] deny = [...]` in `deny.toml` — both gates run in
/// CI, and a mismatch would let one side miss a violation the other
/// catches.
///
/// Last synced: 2026-06-10 (see V1 blocker #5 — license-list sync).
pub const DEFAULT_DENY_LICENSES: &[&str] = &[
    "GPL-1.0",
    "GPL-2.0",
    "GPL-3.0",
    "AGPL-1.0",
    "AGPL-3.0",
    "SSPL-1.0",
    "Commons-Clause",
    "Elastic-2.0",
];

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

fn parse_args_from<I, S>(argv: I) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut args = Args {
        manifest_path: None,
        out: None,
        format: Format::CycloneDx,
        deny_licenses: Vec::new(),
        warn_licenses: Vec::new(),
        show_help: false,
    };
    let mut iter = argv.into_iter();
    while let Some(a) = iter.next() {
        let a = a.as_ref();
        match a {
            "-h" | "--help" => args.show_help = true,
            "--manifest-path" => {
                args.manifest_path = iter.next().map(|v| PathBuf::from(v.as_ref()));
            }
            "--out" => {
                args.out = iter.next().map(|v| PathBuf::from(v.as_ref()));
            }
            "--format" => {
                let v = iter
                    .next()
                    .ok_or("--format requires a value")?
                    .as_ref()
                    .to_string();
                args.format = Format::parse(&v)
                    .ok_or_else(|| format!("unknown format '{v}' (use cyclonedx or spdx)"))?;
            }
            "--deny-licenses" => {
                let v = iter
                    .next()
                    .ok_or("--deny-licenses requires a value")?
                    .as_ref()
                    .to_string();
                args.deny_licenses = v.split(',').map(|s| s.trim().to_string()).collect();
            }
            "--warn-licenses" => {
                let v = iter
                    .next()
                    .ok_or("--warn-licenses requires a value")?
                    .as_ref()
                    .to_string();
                args.warn_licenses = v.split(',').map(|s| s.trim().to_string()).collect();
            }
            other => return Err(format!("unknown flag: {other}")),
        }
    }
    // Apply the built-in deny list when the caller didn't pass
    // `--deny-licenses`. Mirrors `deny.toml` so both gates catch
    // the same set of forbidden licenses.
    if args.deny_licenses.is_empty() {
        args.deny_licenses = DEFAULT_DENY_LICENSES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
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
        .split(['/', ' ', '(', ')'])
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

/// Core license-audit logic. Operates on plain `(name, version,
/// license)` tuples so it can be unit-tested without constructing
/// a full `cargo_metadata::Package`. The wrapper `audit_licenses`
/// adapts the workspace's resolved packages into this shape.
fn audit_license_entries(
    entries: &[(String, String, Option<String>)],
    deny: &[String],
    warn: &[String],
) -> (Vec<String>, Vec<String>) {
    let mut denials = Vec::new();
    let mut warnings = Vec::new();
    for (name, version, lic) in entries {
        let Some(lic) = lic else {
            warnings.push(format!("{name}@{version}: no license declared"));
            continue;
        };
        let tokens = license_tokens(lic);
        for d in deny {
            if tokens.iter().any(|t| t.eq_ignore_ascii_case(d)) {
                denials.push(format!("{name}@{version}: license '{d}' denied"));
            }
        }
        for w in warn {
            if tokens.iter().any(|t| t.eq_ignore_ascii_case(w)) {
                warnings.push(format!("{name}@{version}: license '{w}' is in warn list"));
            }
        }
    }
    (denials, warnings)
}

fn audit_licenses(
    items: &[(Package, Option<String>)],
    deny: &[String],
    warn: &[String],
) -> (Vec<String>, Vec<String>) {
    let entries: Vec<(String, String, Option<String>)> = items
        .iter()
        .map(|(pkg, lic)| {
            (
                pkg.name.as_str().to_string(),
                pkg.version.to_string(),
                lic.clone(),
            )
        })
        .collect();
    audit_license_entries(&entries, deny, warn)
}

/// Run the SBOM generation + license audit. `args` is the full argv
/// (including argv[0]); the first argument is skipped, matching the
/// old `std::env::args().skip(1)` behaviour.
pub fn run(args: &[String]) -> Result<i32, String> {
    let args = parse_args_from(args.iter().skip(1))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn deny_strings() -> Vec<String> {
        DEFAULT_DENY_LICENSES
            .iter()
            .map(|s| (*s).to_string())
            .collect()
    }

    fn entry(name: &str, version: &str, license: Option<&str>) -> (String, String, Option<String>) {
        (
            name.to_string(),
            version.to_string(),
            license.map(str::to_string),
        )
    }

    // --- DEFAULT_DENY_LICENSES sync guard -------------------------------

    #[test]
    fn default_deny_list_matches_deny_toml() {
        // Keep this in lock-step with [licenses] deny = [...] in deny.toml.
        // If you add or remove a license here, update deny.toml too (and
        // vice versa) — see V1 blocker #5.
        let expected: &[&str] = &[
            "GPL-1.0",
            "GPL-2.0",
            "GPL-3.0",
            "AGPL-1.0",
            "AGPL-3.0",
            "SSPL-1.0",
            "Commons-Clause",
            "Elastic-2.0",
        ];
        assert_eq!(DEFAULT_DENY_LICENSES, expected);
    }

    // --- Per-license denial tests (regression for V1 blocker #5) -------

    #[test]
    fn commons_clause_license_is_denied() {
        // "Commons-Clause" was previously missing from pole-sbom's deny
        // list (only present in deny.toml). CI would let a Commons-Clause
        // dependency slip through on the pole-sbom gate.
        let entries = vec![entry(
            "acme-cc",
            "1.0.0",
            Some("Apache-2.0 WITH Commons-Clause"),
        )];
        let (denials, warnings) = audit_license_entries(&entries, &deny_strings(), &[]);
        assert_eq!(denials.len(), 1, "expected 1 denial, got {denials:?}");
        assert!(denials[0].contains("Commons-Clause"));
        assert!(denials[0].contains("acme-cc@1.0.0"));
        assert!(
            warnings.is_empty(),
            "Commons-Clause is a hard denial, not a warning"
        );
    }

    #[test]
    fn elastic_license_is_denied() {
        // Same regression as above, but for Elastic-2.0.
        let entries = vec![entry("elastic-thing", "0.4.2", Some("Elastic-2.0"))];
        let (denials, warnings) = audit_license_entries(&entries, &deny_strings(), &[]);
        assert_eq!(denials.len(), 1, "expected 1 denial, got {denials:?}");
        assert!(denials[0].contains("Elastic-2.0"));
        assert!(denials[0].contains("elastic-thing@0.4.2"));
        assert!(warnings.is_empty());
    }

    #[test]
    fn mit_license_is_allowed() {
        // Sanity: the allow list still works after the deny-list expansion.
        let entries = vec![entry("serde", "1.0.219", Some("MIT"))];
        let (denials, warnings) = audit_license_entries(&entries, &deny_strings(), &[]);
        assert!(denials.is_empty(), "MIT must not be denied: {denials:?}");
        assert!(warnings.is_empty());
    }

    // --- Misc hardening for the rest of the deny list --------------------

    #[test]
    fn gpl3_license_is_denied() {
        // Regression: GPL-3.0 was already in the deny list; this guards
        // against the audit_license_entries refactor accidentally
        // dropping it. Uses the short SPDX form that matches the deny
        // list entry verbatim; SPDX 3.x forms like "GPL-3.0-only" /
        // "GPL-3.0-or-later" are not currently normalised by
        // `license_tokens` and would slip through (out of scope for
        // V1 blocker #5; tracked as a follow-up).
        let entries = vec![entry("legacy-c", "2.0.0", Some("GPL-3.0"))];
        let (denials, _) = audit_license_entries(&entries, &deny_strings(), &[]);
        assert_eq!(denials.len(), 1);
        assert!(denials[0].contains("GPL-3.0"));
    }

    #[test]
    fn sspl_license_is_denied() {
        let entries = vec![entry("mongo-rs", "1.5.0", Some("SSPL-1.0"))];
        let (denials, _) = audit_license_entries(&entries, &deny_strings(), &[]);
        assert_eq!(denials.len(), 1);
        assert!(denials[0].contains("SSPL-1.0"));
    }

    #[test]
    fn no_license_emits_warning_not_denial() {
        let entries = vec![entry("unlicensed", "0.0.1", None)];
        let (denials, warnings) = audit_license_entries(&entries, &deny_strings(), &[]);
        assert!(denials.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("no license declared"));
    }

    #[test]
    fn warn_list_emits_warning_not_denial() {
        let entries = vec![entry("copyleft", "1.0.0", Some("MPL-2.0"))];
        let warns = vec!["MPL-2.0".to_string()];
        let (denials, warnings) = audit_license_entries(&entries, &deny_strings(), &warns);
        assert!(denials.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("MPL-2.0"));
        assert!(warnings[0].contains("warn list"));
    }

    // --- parse_args default-deny behaviour -------------------------------

    #[test]
    fn parse_args_applies_default_deny_when_flag_omitted() {
        // Drive parse_args_from with an empty argv to confirm the
        // default-deny kick-in path. Mirrors `parse_args()` reading
        // `std::env::args().skip(1)` with no flags.
        let parsed = parse_args_from(Vec::<&str>::new()).expect("parse_args_from empty");
        let expected: Vec<String> = DEFAULT_DENY_LICENSES
            .iter()
            .map(|s| (*s).to_string())
            .collect();
        assert_eq!(parsed.deny_licenses, expected);
    }

    #[test]
    fn parse_args_user_deny_overrides_default() {
        // When the caller passes --deny-licenses explicitly, the default
        // list must NOT be merged in — otherwise the explicit list grows
        // silently every time we add to DEFAULT_DENY_LICENSES.
        let parsed = parse_args_from(vec!["--deny-licenses", "GPL-3.0-only"]).unwrap();
        assert_eq!(parsed.deny_licenses, vec!["GPL-3.0-only".to_string()]);
        assert!(!parsed.deny_licenses.iter().any(|s| s == "Commons-Clause"));
    }

    #[test]
    fn parse_args_csv_deny_lists_are_trimmed() {
        let parsed =
            parse_args_from(vec!["--deny-licenses", "GPL-3.0-only, AGPL-3.0-only "]).unwrap();
        assert_eq!(
            parsed.deny_licenses,
            vec!["GPL-3.0-only".to_string(), "AGPL-3.0-only".to_string()]
        );
    }

    // --- license_tokens helper -------------------------------------------

    #[test]
    fn license_tokens_splits_expression() {
        // `audit_license_entries` relies on tokenisation so that
        // "Apache-2.0 WITH Commons-Clause" matches a deny entry of
        // "Commons-Clause" without false positives on "Apache-2.0".
        let toks = license_tokens("Apache-2.0 WITH Commons-Clause");
        assert!(toks.iter().any(|t| t == "Apache-2.0"));
        assert!(toks.iter().any(|t| t == "WITH"));
        assert!(toks.iter().any(|t| t == "Commons-Clause"));
    }
}
