#!/usr/bin/env python3
"""Bind checked date/time profiles and their calendar/formatting algorithms."""

import argparse
import hashlib
import json
from pathlib import Path
import tomllib


REPOSITORY = Path(__file__).resolve().parents[1]
PROVIDER = Path("crates/scriptw-intl/src/provider/datetime")
PROFILE = Path("crates/scriptw-intl/data/datetime-cldr-47")
PACKAGES = {
    "calendrical_calculations": "0.2.4",
    "icu_calendar": "2.0.6",
    "icu_calendar_data": "2.0.0",
    "icu_locale": "2.0.0",
    "serde": "1.0.228",
    "serde_json": "1.0.149",
}
PRODUCERS = (
    "generate-intl-datetime-profile.py", "generate-intl-datetime-identity.py",
    "intl_cldr_profile.py", "intl_ldml_schema.py", "intl_datetime_patterns.py",
    "intl_datetime_names.py", "intl_datetime_zones.py", "intl_rbnf_fields.py",
    "generate-intl-time-zone-names.py", "intl_positional_numbering.py",
    "intl_datetime_pool.py", "intl_calendar_eras.py", "generate-intl-keyword-aliases.py",
)


def generate(repository, calendar_root, lock_path):
    paths = {
        PROFILE / "manifest.json", PROFILE / "selector.json",
        Path("crates/scriptw-intl/data/zone-names-cldr-47/manifest.json"),
        Path("crates/scriptw-intl/src/datetime.rs"), Path("crates/scriptw-intl/src/datetime/input.rs"),
        Path("crates/scriptw-intl/src/datetime_protocol.rs"),
        Path("crates/scriptw-intl/src/provider/keyword_aliases.rs"),
        Path("crates/scriptw-intl/src/provider/keyword_aliases/generated.rs"),
        Path("crates/scriptw-intl/src/provider/keyword_aliases/identity.rs"),
        Path("crates/scriptw-intl/src/provider/datetime.rs"), PROVIDER / "generated/profile.json",
        *(Path("scripts") / name for name in PRODUCERS),
    }
    # Locale query identities depend on this DateTime authority. Only genuine
    # upstream IANA metadata and actual image consumers enter this input set.
    paths.update(Path(path) for path in [
        'crates/scriptw-intl/Cargo.toml',
        'crates/scriptw-intl/build.rs',
        'crates/scriptw-intl/src/lib.rs',
        'crates/scriptw-intl/src/artifact_identity.rs',
        'crates/scriptw-intl/src/provider.rs', 'crates/scriptw-intl/src/provider/conformance.rs', 'crates/scriptw-intl/src/selection.rs', 'crates/scriptw-intl/src/selection/minimal_admission.rs',
        'crates/scriptw-intl/src/service_selection.rs',
        'crates/scriptw-intl/src/selection/manifest.rs',
        'crates/scriptw-intl/src/selection/export.rs',
        'crates/scriptw-intl/src/protocol.rs',
        'crates/scriptw-intl/src/image.rs',
        'crates/scriptw-intl/src/locale_image.rs',
        'crates/scriptw-intl/src/image_build/locale.rs',
        'crates/scriptw-intl/src/image_build/keyword.rs',
        'crates/scriptw-intl/src/provider/language_domain.rs',
        'scripts/generate-intl-keyword-aliases.py',
        'crates/scriptw-engine/src/intl_data_images.rs',
        'crates/scriptw-engine/src/wasm_gc_intl_host.rs',
        'crates/scriptw-aot-wasm/src/emit.rs',
        'crates/scriptw-aot-wasm/src/emit/module_assembly.rs',
        'crates/scriptw-intl/src/named_time_zone_image.rs',
        'crates/scriptw-intl/src/image_build/named_time_zones.rs',
        'crates/scriptw-intl/src/provider/locale_time_zones.rs',
        'crates/scriptw-intl/data/locale-time-zones-iana2026a/zone.tab',
        'crates/scriptw-intl/data/locale-time-zones-iana2026a/regions.tsv',
        'crates/scriptw-intl/src/time_zone_names_image.rs',
        'crates/scriptw-intl/src/provider/time_zone_names.rs',
        'crates/scriptw-intl/src/provider/time_zone_names/raw.rs',
        'crates/scriptw-intl/data/zone-names-cldr-47/native-profile.json',
        'crates/scriptw-intl/data/zone-names-cldr-47/native-profile-manifest.json',
        'crates/scriptw-intl/src/datetime_image.rs',
        'crates/scriptw-intl/src/image_build/calendar.rs',
        'crates/scriptw-engine/src/intl_datetime_host.rs',
        'crates/scriptw-intl/src/provider/time_zone_snapshot.rs',
        'crates/scriptw-intl/src/provider/named_time_zones/identity.rs',
        'crates/scriptw-intl/data/iana-tzdb-2026a/manifest.json',
        'crates/scriptw-intl/data/iana-tzdb-2026a/catalogue.tsv',
    ])
    paths.update(path.relative_to(repository) for path in
                 (repository / "crates/scriptw-intl/data/numbering-tols-cldr-48").rglob("*") if path.is_file())
    paths.update(path.relative_to(repository) for path in
                 (repository / "crates/scriptw-intl/data/cldr-47-bcp47").rglob("*") if path.is_file())
    era_sources = repository / "crates/scriptw-intl/data/calendar-eras-cldr-48"
    if era_sources.is_dir():
        paths.update(path.relative_to(repository) for path in era_sources.rglob("*") if path.is_file())
    # Consume actual native image admission/projection helpers alongside the
    # existing DateTime provider; fixture and test modules stay outside recipes.
    for owner in ("datetime_image", "named_time_zone_image", "time_zone_names_image"):
        for path in (repository / "crates/scriptw-intl/src" / owner).rglob("*.rs"):
            relative = path.relative_to(repository)
            if relative.name != "tests.rs" and "tests" not in relative.parts:
                paths.add(relative)
    for path in (repository / PROVIDER).rglob("*.rs"):
        relative = path.relative_to(repository)
        if relative.name not in ("identity.rs", "tests.rs") and "tests" not in relative.parts:
            paths.add(relative)
    records = []
    for path in sorted(paths):
        contents = (repository / path).read_bytes()
        records.append({"path": path.as_posix(), "sha256": hashlib.sha256(contents).hexdigest(), "bytes": len(contents)})
    vendor = Path("vendor/icu_calendar-2.0.6")
    calendar_paths = [vendor / "Cargo.toml", *(path.relative_to(calendar_root) for path in (calendar_root / vendor / "src").rglob("*.rs"))]
    if len(calendar_paths) < 30:
        raise ValueError("the canonical calendar source inventory is incomplete")
    for path in sorted(calendar_paths):
        contents = (calendar_root / path).read_bytes()
        records.append({"path": path.as_posix(), "sha256": hashlib.sha256(contents).hexdigest(), "bytes": len(contents)})
    lock = tomllib.loads(lock_path.read_text())
    packages = []
    for name, version in sorted(PACKAGES.items()):
        candidates = [package for package in lock["package"] if package["name"] == name]
        if len(candidates) != 1 or candidates[0]["version"] != version:
            raise ValueError(f"unexpected canonical kernel dependency: {name}")
        package = candidates[0]
        if name == "icu_calendar":
            if "source" in package or "checksum" in package:
                raise ValueError("the checked calendar vendor patch is absent")
        elif package.get("source") != "registry+https://github.com/rust-lang/crates.io-index" or len(package.get("checksum", "")) != 64:
            raise ValueError(f"dependency lacks a registry identity: {name}")
        packages.append({key: package[key] for key in ("name", "version", "source", "checksum") if key in package})
    recipe = {"schema_version": 1, "purpose": "DateTimeFormat inherited CLDR profile and exact calendar/parts kernel", "packages": packages, "files": records}
    canonical = json.dumps(recipe, sort_keys=True, separators=(",", ":")).encode()
    digest = hashlib.sha256(canonical).digest()
    rust = "// Generated by scripts/generate-intl-datetime-identity.py; do not edit.\n"
    rust += "pub(super) const PROVIDER_DATA_SHA256: [u8; 32] = [\n"
    for start in range(0, 32, 16):
        rust += "    " + ", ".join(f"0x{byte:02x}" for byte in digest[start:start + 16]) + ",\n"
    rust += "];\n"
    receipt = {**recipe, "provider_data_sha256": digest.hex()}
    return rust, json.dumps(receipt, indent=2, sort_keys=True) + "\n"


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, default=REPOSITORY)
    parser.add_argument("--calendar-root", type=Path)
    parser.add_argument("--lock", type=Path)
    parser.add_argument("--check", action="store_true")
    options = parser.parse_args()
    calendar_root = options.calendar_root or options.repository
    lock_path = options.lock or options.repository / "Cargo.lock"
    rust, receipt = generate(options.repository, calendar_root, lock_path)
    for relative, contents in [(PROVIDER / "identity.rs", rust), (PROFILE / "kernel-identity.json", receipt)]:
        path = options.repository / relative
        if options.check:
            if path.read_text() != contents:
                raise ValueError(f"stale date/time identity: {relative}")
        else:
            if path.exists() and path.read_text() == contents:
                continue
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(contents)
    print(json.dumps({"identity": json.loads(receipt)["provider_data_sha256"], "source_files": len(json.loads(receipt)["files"])}))


if __name__ == "__main__":
    main()
