#!/usr/bin/env python3
"""Generate the immutable ECMA-402 catalogue from pinned IANA2026a inputs."""

import argparse
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile


DATA = Path("crates/scriptw-intl/data/iana-tzdb-2026a")
PROVIDER = Path("crates/scriptw-intl/src/provider/named_time_zones")
SOURCE_FILES = (
    "africa", "antarctica", "asia", "australasia", "europe", "northamerica",
    "southamerica", "etcetera", "backward", "factory",
)
ARCHIVES = {
    "tzdata2026a.tar.gz": "77b541725937bb53bd92bd484c0b43bec8545e2d3431ee01f04ef8f2203ba2b7",
    "jiff-tzdb-0.1.6.crate": "c900ef84826f1338a557697dc8fc601df9ca9af4ac137c7fb61d4c6f2dfd3076",
    "timezone_provider-0.1.2.crate": "df9ba0000e9e73862f3e7ca1ff159e2ddf915c9d8bb11e38a7874760f445d993",
}
COUNTRIES_SHA256 = "c0c34daadb59d2552252405b9be08894f83d2cd8145d0f6eb35e1557721dd55f"


def digest(content):
    return hashlib.sha256(content).hexdigest()


def archive_contents(path, expected):
    content = path.read_bytes()
    if digest(content) != expected:
        raise ValueError(f"pinned archive digest mismatch: {path}")
    with tarfile.open(fileobj=io.BytesIO(content), mode="r:gz") as archive:
        return {
            member.name: archive.extractfile(member).read()
            for member in archive.getmembers() if member.isfile()
        }


def zone_records(sources):
    zones, links, country_targets = set(), {}, {}
    for name in SOURCE_FILES:
        for line in sources[name].decode().splitlines():
            declaration, _, comment = line.partition("#")
            fields = declaration.split()
            if fields and comment.startswith("="):
                targets = comment[1:].split()
                if fields[0] != "Link" or len(fields) != 3 or len(targets) != 1:
                    raise ValueError(f"invalid unflattened Link annotation: {line}")
            if fields and fields[0] == "Zone":
                if fields[1] in zones or fields[1] in links:
                    raise ValueError(f"duplicate Zone: {fields[1]}")
                zones.add(fields[1])
            elif fields and fields[0] == "Link":
                if fields[2] in links or fields[2] in zones:
                    raise ValueError(f"duplicate Link: {fields[2]}")
                links[fields[2]] = fields[1]
            if fields and comment.startswith("="):
                country_targets[fields[2]] = targets[0]
    return zones, links, country_targets


def resolve_link(identifier, zones, links):
    seen = set()
    while identifier in links:
        if identifier in seen:
            raise ValueError(f"cyclic Link: {identifier}")
        seen.add(identifier)
        identifier = links[identifier]
    if identifier not in zones:
        raise ValueError(f"missing Link target: {identifier}")
    return identifier


def geographic_link_countries(zones, links, tab, countries, country_targets):
    """Recover explicit pre-flattening alias ownership from IANA's #= targets."""
    available = zones | links.keys()
    for identifier, target in country_targets.items():
        if identifier not in links or target not in available:
            raise ValueError(f"unknown unflattened Link target: {identifier} -> {target}")
        if resolve_link(identifier, zones, links) != resolve_link(target, zones, links):
            raise ValueError(f"unflattened Link changes terminal Zone: {identifier}")
        current, seen = identifier, set()
        while current in country_targets:
            if current in seen:
                raise ValueError(f"cyclic unflattened Link annotation: {identifier}")
            seen.add(current)
            current = country_targets[current]

    resolved = countries | tab
    for identifier in country_targets:
        # zone.tab names carry their own geography, even when linked across
        # country boundaries. Only absent legacy aliases need this authority.
        if identifier in tab:
            continue
        target = identifier
        while target in country_targets and target not in tab:
            target = country_targets[target]
        if target not in resolved:
            raise ValueError(f"missing unflattened Link country: {identifier} -> {target}")
        resolved[identifier] = resolved[target]
    return resolved


def country_records(source):
    """Read ICU fallback country arrays, never its transition or alias data."""
    columns = []
    for pattern in [r"\bNames\s*\{([^}]+)\}", r"\bRegions:array\s*\{([^}]+)\}"]:
        matches = re.findall(pattern, source, flags=re.S)
        if len(matches) != 1:
            raise ValueError("missing or duplicated ICU country array")
        # Remove indexed comments before reading quoted values.
        text = re.sub(r"//[^\n]*", "", matches[0])
        columns.append(re.findall(r'"([^"\\]+)"', text))
    names, countries = columns
    if len(names) != len(countries) or len(names) != len(set(names)):
        raise ValueError("ICU country arrays are not one-to-one")
    if any(not re.fullmatch(r"[A-Z]{2}|001", country) for country in countries):
        raise ValueError("invalid ICU territory code")
    return dict(zip(names, countries, strict=True))


def tab_records(source, countries):
    tab, by_country = {}, {}
    for line in source.splitlines():
        if not line or line.startswith("#"):
            continue
        country, _, identifier, *_ = line.split()
        if identifier in tab or not re.fullmatch("[A-Z]{2}", country):
            raise ValueError("invalid or duplicated zone.tab identifier")
        if identifier in countries and countries[identifier] != country:
            raise ValueError(f"country metadata disagrees with 2026a: {identifier}")
        tab[identifier] = country
        by_country.setdefault(country, []).append(identifier)
    return tab, by_country


def backzone_records(source):
    zones, links = set(), {}
    for line in source.splitlines():
        if line.startswith("#PACKRATLIST zone.tab "):
            line = line.removeprefix("#PACKRATLIST zone.tab ")
        fields = line.split("#", 1)[0].split()
        if fields and fields[0] == "Zone":
            if fields[1] in zones:
                raise ValueError(f"duplicate backzone Zone: {fields[1]}")
            zones.add(fields[1])
        elif fields and fields[0] == "Link":
            if fields[2] in links:
                raise ValueError(f"duplicate backzone Link: {fields[2]}")
            links[fields[2]] = fields[1]
    return zones, links


def primary_identifiers(zones, links, tab, by_country, countries, backzone_zones, backzone):
    available = zones | links.keys()
    if len({name.lower() for name in available}) != len(available):
        raise ValueError("case-insensitive identifier collision")
    countries = countries | tab
    result = {}
    for identifier in sorted(available):
        primary = identifier
        if identifier in links and identifier not in tab:
            zone = resolve_link(identifier, zones, links)
            if zone.startswith("Etc/"):
                primary = zone
            else:
                if identifier not in countries or zone not in countries:
                    raise ValueError(f"missing geographic country: {identifier} -> {zone}")
                country = countries[identifier]
                # World-region labels do not designate a geographic area
                # contained in one ISO3166 country.
                if country == "001" or country == countries[zone]:
                    primary = zone
                else:
                    choices = by_country.get(country, [])
                    if len(choices) == 1:
                        primary = choices[0]
                    else:
                        if identifier in backzone:
                            primary = backzone[identifier]
                        elif identifier in backzone_zones:
                            # A source-defined backzone Zone retains its own
                            # geography when no Link exists for the spec step.
                            primary = identifier
                        else:
                            raise ValueError(f"missing cross-country backzone Link: {identifier}")
                        if countries.get(primary) != country:
                            raise ValueError(f"backzone primary crosses country: {identifier}")
        if primary in {"Etc/UTC", "Etc/GMT", "GMT"}:
            primary = "UTC"
        if primary not in available:
            raise ValueError(f"primary identifier absent: {primary}")
        result[identifier] = primary
    if result.get("UTC") != "UTC":
        raise ValueError("UTC primary missing")
    for identifier, primary in result.items():
        if result[primary] != primary:
            raise ValueError(f"nonterminal primary: {identifier} -> {primary}")
    return result


def transition_records(crate):
    source = crate["jiff-tzdb-0.1.6/tzname.rs"].decode()
    if 'Some(r"2026a")' not in source:
        raise ValueError("transition archive version is not 2026a")
    archive = crate["jiff-tzdb-0.1.6/concatenated-zoneinfo.dat"]
    records = {}
    for name, start, end in re.findall(r'\(r"([^"]+)", (\d+)\.\.(\d+)\)', source):
        start, end = int(start), int(end)
        if name in records or not 0 <= start < end <= len(archive):
            raise ValueError("invalid transition archive range")
        content = archive[start:end]
        if not content.startswith(b"TZif"):
            raise ValueError("invalid transition archive signature")
        records[name] = content
    if not records:
        raise ValueError("empty transition archive")
    return records


def record(path, content):
    return {"path": str(path), "bytes": len(content), "sha256": digest(content)}


def outputs(root):
    data = root / DATA
    archives = {name: archive_contents(data / name, sha) for name, sha in ARCHIVES.items()}
    sources = archives["tzdata2026a.tar.gz"]
    if sources["version"] != b"2026a\n":
        raise ValueError("identifier source version is not 2026a")
    for path in sorted((data / "source").iterdir()):
        if path.read_bytes() != sources[path.name]:
            raise ValueError(f"source extract differs from archive: {path.name}")
    jiff = archives["jiff-tzdb-0.1.6.crate"]
    for name in ["COPYING", "LICENSE-MIT", "UNLICENSE"]:
        if (data / "jiff-license" / name).read_bytes() != jiff[f"jiff-tzdb-0.1.6/{name}"]:
            raise ValueError(f"Jiff license extract differs from archive: {name}")
    country_source = (data / "icu-77-1-zoneinfo64.icu").read_bytes()
    if digest(country_source) != COUNTRIES_SHA256:
        raise ValueError("country source digest mismatch")
    countries = country_records(country_source.decode())
    tab, by_country = tab_records(sources["zone.tab"].decode(), countries)
    zones, links, country_targets = zone_records(sources)
    geographic_countries = geographic_link_countries(
        zones, links, tab, countries, country_targets)
    backzone_zones, backzone_links = backzone_records(sources["backzone"].decode())
    primaries = primary_identifiers(zones, links, tab, by_country, geographic_countries,
                                    backzone_zones, backzone_links)
    transitions = transition_records(archives["jiff-tzdb-0.1.6.crate"])
    if primaries.keys() != transitions.keys():
        raise ValueError(f"catalogue/archive mismatch: {primaries.keys() ^ transitions.keys()}")
    catalogue = "".join(f"{name}\t{primary}\t{digest(transitions[name])}\n"
                        for name, primary in primaries.items()).encode()
    # The selector recipe includes the actual patched source and validation code.
    recipe_paths = [Path("scripts/generate-intl-named-time-zones.py"),
                    Path("crates/scriptw-intl/src/provider/named_time_zones.rs"),
                    Path("crates/scriptw-intl/src/time_zone/exact.rs"),
                    Path("crates/scriptw-intl/src/time_zone/named_query.rs")]
    recipe_paths += sorted(path.relative_to(root) for path in (root / PROVIDER).rglob("*.rs")
                           if path.name not in {"identity.rs", "tests.rs", "transition_tests.rs"})
    # Selected IANA/country/Names consumers are source inputs. Never capture the
    # downstream country kernel identity/manifest or this producer's own outputs.
    recipe_paths += [Path(path) for path in [
        'Cargo.toml',
        'Cargo.lock',
        'crates/scriptw-intl/Cargo.toml',
        'crates/scriptw-intl/build.rs',
        'crates/scriptw-intl/src/lib.rs',
        'crates/scriptw-intl/src/artifact_identity.rs',
        'crates/scriptw-intl/src/provider.rs', 'crates/scriptw-intl/src/selection.rs', 'crates/scriptw-intl/src/selection/minimal_admission.rs',
        'crates/scriptw-intl/src/provider/conformance.rs',
        'crates/scriptw-intl/src/service_selection.rs',
        'crates/scriptw-intl/src/selection/manifest.rs',
        'crates/scriptw-intl/src/selection/export.rs',
        'crates/scriptw-intl/src/protocol.rs',
        'crates/scriptw-intl/src/image.rs',
        'crates/scriptw-intl/src/locale_image.rs',
        'crates/scriptw-intl/src/image_build/locale.rs',
        'crates/scriptw-intl/src/image_build/keyword.rs',
        'crates/scriptw-intl/src/provider/language_domain.rs',
        'crates/scriptw-intl/src/provider/keyword_aliases.rs',
        'crates/scriptw-intl/src/provider/keyword_aliases/generated.rs', 'crates/scriptw-intl/src/provider/keyword_aliases/identity.rs',
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
        'crates/scriptw-intl/src/provider/time_zone_snapshot.rs',
        'crates/scriptw-intl/src/time_zone.rs',
        'crates/scriptw-intl/src/identifiers.rs',
    ]]
    # Bind actual selected transition/name admission helpers, retaining the
    # same complete pinned catalogue producer and excluding control modules.
    for owner in ("named_time_zone_image", "time_zone_names_image"):
        recipe_paths += sorted(path.relative_to(root) for path in
                               (root / "crates/scriptw-intl/src" / owner).rglob("*.rs")
                               if path.name != "tests.rs" and "tests" not in path.parts)
    vendor_paths = sorted(path.relative_to(root) for path in
                          (root / "vendor/timezone_provider-0.1.2").rglob("*") if path.is_file())
    inputs = [record(DATA / name, (data / name).read_bytes()) for name in sorted(ARCHIVES)]
    inputs += [record(DATA / "icu-77-1-zoneinfo64.icu", country_source)]
    inputs += [record(DATA / name, (data / name).read_bytes())
               for name in ["icu-country-source.json", "ICU-LICENSE"]]
    inputs += [record(path, (root / path).read_bytes()) for path in recipe_paths + vendor_paths]
    recipe = {"domain": "scriptw-intl-named-time-zones-v1", "iana": "2026a",
              "transitionFormat": "jiff rearguard slim TZif; exact selected is_dst",
              "identifierRules": "ECMA-402 AvailableNamedTimeZoneIdentifiers; no pending rename",
              "countryRole": "IANA2026a zone.tab and explicit unflattened Link targets; ICU77.1 Names/Regions fallback",
              "inputs": inputs, "catalogue": record(DATA / "catalogue.tsv", catalogue)}
    encoded = json.dumps(recipe, sort_keys=True, separators=(",", ":")).encode()
    provider_digest = hashlib.sha256(encoded).digest()
    constants = [("PROVIDER_DATA_SHA256", provider_digest),
                 ("CATALOGUE_SHA256", hashlib.sha256(catalogue).digest())]
    identity = "// Generated by scripts/generate-intl-named-time-zones.py.\n"
    for name, value in constants:
        identity += (f"pub(super) const {name}: [u8; 32] = [\n    "
                     + ", ".join(f"0x{byte:02x}" for byte in value[:16]) + ",\n    "
                     + ", ".join(f"0x{byte:02x}" for byte in value[16:]) + ",\n];\n")
    identity = identity.encode()
    manifest = {"ianaRelease": "2026a", "identifierCount": len(primaries),
                "primaryCount": sum(name == primary for name, primary in primaries.items()),
                "backzoneZonePrimaries": [name for name, primary in primaries.items()
                                         if name == primary and name in links
                                         and name not in tab and name in backzone_zones],
                "linkCountryTargets": dict(sorted(country_targets.items())),
                "annotatedCountryOverrides": [
                    {"identifier": name, "icuCountry": countries.get(name),
                     "ianaCountry": geographic_countries[name]}
                    for name in sorted(country_targets)
                    if countries.get(name) != geographic_countries[name]],
                "providerDataSha256": provider_digest.hex(), "recipe": recipe,
                "sourceExtracts": [record(path.relative_to(root), path.read_bytes())
                                   for path in sorted((data / "source").iterdir())],
                "jiffLicenses": [record(path.relative_to(root), path.read_bytes())
                                 for path in sorted((data / "jiff-license").iterdir())],
                "generated": [record(DATA / "catalogue.tsv", catalogue),
                              record(PROVIDER / "identity.rs", identity)]}
    return {DATA / "catalogue.tsv": catalogue, PROVIDER / "identity.rs": identity,
            DATA / "manifest.json": (json.dumps(manifest, indent=2) + "\n").encode()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    for relative, content in outputs(args.root).items():
        path = args.root / relative
        if args.check:
            if not path.is_file() or path.read_bytes() != content:
                raise SystemExit(f"stale generated named-zone file: {relative}")
        else:
            if path.exists() and path.read_bytes() == content:
                continue
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(content)


if __name__ == "__main__":
    main()
