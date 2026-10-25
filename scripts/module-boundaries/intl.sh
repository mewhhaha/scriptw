require_file crates/scriptw-aot-wasm/src/builtins/intl_supported_values.rs
require_file crates/scriptw-aot-wasm/src/data/intl_supported_values.rs
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/mod.rs 'mod intl_supported_values;' 1 'actual supportedValuesOf emitter registration'
require_fixed_string_count crates/scriptw-aot-wasm/src/data.rs 'mod intl_supported_values;' 1 'checked static catalogue owner registration'
wasm_normalization_data="crates/scriptw-aot-wasm/src/data/normalization.rs"
require_file "$wasm_normalization_data"
require_exact_line_count crates/scriptw-aot-wasm/src/data.rs 'mod normalization;' 1 'private Unicode normalization table owner'
require_fixed_string_count "$wasm_normalization_data" "pub(super) fn tables() -> &'static NormalizationTables" 1 'shared normalization table authority'
require_fixed_string_count crates/scriptw-aot-wasm/src/data.rs 'normalization::tables()' 1 'data construction consumes the normalization authority'
check_no_inline_legacy_includes "$wasm_normalization_data"
check_raw_line_budget "$wasm_normalization_data" 170
for normalization_child in format construction tests; do
  require_module_decl "$wasm_normalization_data" "$normalization_child"
  normalization_owner="crates/scriptw-aot-wasm/src/data/normalization/$normalization_child.rs"
  require_file "$normalization_owner"
  check_no_inline_legacy_includes "$normalization_owner"
  check_raw_line_budget "$normalization_owner" 250
done
require_fixed_string_count "$wasm_normalization_data" 'NormalizationTables::from_image(EMBEDDED_IMAGE)' 1 'normalization consumes admitted immutable rows'
require_fixed_string_count crates/scriptw-aot-wasm/build.rs 'normalization_construction::build()' 1 'normalization construction belongs to Cargo build'
require_module_decl crates/scriptw-aot-wasm/src/data.rs unicode_case_tables
for case_data_owner in \
  crates/scriptw-aot-wasm/src/data/unicode_case_tables.rs \
  crates/scriptw-aot-wasm/src/data/unicode_case_tables/construction.rs \
  crates/scriptw-aot-wasm/src/data/unicode_case_tables/tests.rs; do
  require_file "$case_data_owner"
  check_no_inline_legacy_includes "$case_data_owner"
  check_raw_line_budget "$case_data_owner" 150
done
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/standard.rs 'self.emit_intl_supported_values_of(function)?;' 1 'actual supportedValuesOf dispatch'
require_tree_regex_count crates/scriptw-aot-wasm/src '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_intl_supported_values_of[[:space:]]*\(' 1 'supportedValuesOf sole emitter owner'
require_fixed_string_count crates/scriptw-aot-wasm/src/emit.rs 'let intl_selection = inputs.intl_selection(intl_profile)?;' 1 'one lazy selected Intl owner outside builtin retries'
require_fixed_string_count crates/scriptw-aot-wasm/src/lib.rs 'pub use emit::emit_with_intl_profile;' 1 'actual selected Intl compiler entry'
require_module_decl crates/scriptw-aot-wasm/src/emit/module_assembly.rs intl
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/module_assembly/intl.rs 'string_pool.check_intl_supported_values()?' 1 'consumed checked selected catalogue admission'
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/module_assembly/intl.rs 'selected.identity().artifact_identity()' 1 'sole selected artifact identity owner'
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/module_assembly/intl.rs 'for (name, data) in selected.component_sections()' 1 'same selected physical frame inventory consumed by export and emission'
require_fixed_string_count crates/scriptw-aot-wasm/src/emit/module_assembly/intl.rs 'if let Some(services) = selected.service_selection()' 1 'artifact carries its checked public service authority'
require_fixed_string_count crates/scriptw-aot-wasm/src/data/intl_supported_values.rs 'pub(crate) enum CompiledSupportedValuesTable {' 1 'closed per-key selected primitive catalogue owner'
require_exact_line_count crates/scriptw-aot-wasm/src/data/intl_supported_values.rs '    Available(Arc<SupportedValuesList>),' 1 'available keys retain their admitted catalogue'
require_exact_line_count crates/scriptw-aot-wasm/src/data/intl_supported_values.rs '    Unavailable,' 1 'omitted key data has explicit unavailable authority'
require_fixed_string_count crates/scriptw-aot-wasm/src/data/intl_supported_values.rs 'pub(crate) fn source(&self) -> Option<Arc<SupportedValuesList>>' 1 'catalogue access preserves physical absence'
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/string/unicode_operations.rs 'b.strings.intl_default_locale()?' 1 'selected Unicode case DefaultLocale consumer'
require_tree_regex_count crates/scriptw-aot-wasm/src/builtins 'embedded_intl_data_identity' 0 'no default identity escape from selected native consumers'

# Real filtered List data has one crate-private catalogue owner and a private
# exact-row exporter. Both production and admission consume this same closure;
# Duration resolves its retained List association through the selected owner.
intl_list_image="crates/scriptw-intl/src/list_image.rs"
intl_list_projection="crates/scriptw-intl/src/list_image/projection.rs"
intl_list_projection_export="crates/scriptw-intl/src/list_image/projection/export.rs"
require_file "$intl_list_projection"
require_file "$intl_list_projection_export"
require_exact_line_count "$intl_list_image" 'pub(crate) mod projection;' 1 'crate-private actual List projection owner attachment'
require_exact_line_count "$intl_list_projection" 'mod export;' 1 'private exact List row exporter attachment'
for list_projection_entry in produce admit; do
  require_fixed_string_count "$intl_list_projection" "pub(super) fn ${list_projection_entry}(" 1 'actual projected List production/admission owner'
  require_fixed_string_count "$intl_list_image" "projection::${list_projection_entry}(" 1 'actual selected List projection consumer'
done
require_fixed_string_count "$intl_list_projection_export" 'pub(super) fn export_rows(' 1 'actual exact List row export entry'
require_fixed_string_count "$intl_list_projection" 'export::export_rows(' 1 'physical selected List rows consume the exporter'
require_fixed_string_count crates/scriptw-intl/src/list_format/profiles.rs 'use crate::list_image::projection::ListCatalogue;' 1 'List admission consumes the projected catalogue'
require_fixed_string_count crates/scriptw-intl/src/list_format/profiles.rs 'pub(crate) fn resolve_duration_locale(' 1 'selected private Duration List association owner'
require_fixed_string_count crates/scriptw-intl/src/duration_format/profiles.rs 'lists.resolve_duration_locale(' 1 'Duration consumes the selected List association'
check_no_inline_legacy_includes "$intl_list_projection"
check_no_inline_legacy_includes "$intl_list_projection_export"
check_raw_line_budget "$intl_list_image" 250
check_raw_line_budget "$intl_list_projection" 435
check_raw_line_budget "$intl_list_projection_export" 60
check_raw_line_budget crates/scriptw-intl/src/list_format/profiles.rs 300
check_raw_line_budget crates/scriptw-intl/src/duration_format/profiles.rs 375

# RelativeTime's native projection owns actual complete selected rows. Byte
# admission recomputes the producer closure before minting public profiles;
# the complete Number foundation remains the numeric operation owner.
intl_relative_image="crates/scriptw-intl/src/relative_time_image.rs"
intl_relative_projection="crates/scriptw-intl/src/relative_time_image/projection.rs"
intl_selection="crates/scriptw-intl/src/selection.rs"
require_file "$intl_relative_projection"
require_exact_line_count "$intl_relative_image" 'pub(crate) mod projection;' 1 'private actual RelativeTime projection attachment'
for relative_projection_entry in produce admit; do
  require_fixed_string_count "$intl_relative_projection" "pub(super) fn ${relative_projection_entry}" 1 'actual selected RelativeTime production/admission owner'
  require_fixed_string_count "$intl_relative_image" "projection::${relative_projection_entry}(" 1 'consumed RelativeTime projection entry'
done
require_fixed_string_count "$intl_relative_projection" 'if payload != expected.as_slice()' 1 'reader requires the exact pinned projection derivation'
require_fixed_string_count "$intl_relative_projection" 'pub(crate) struct RelativeCatalogue {' 1 'private producer-minted selected RelativeTime catalogue'
require_fixed_string_count crates/scriptw-intl/src/relative_time_format/profiles.rs 'catalogue: &crate::relative_time_image::projection::RelativeCatalogue,' 1 'selected profiles require actual admitted row provenance'
require_fixed_string_count "$intl_relative_image" 'RelativeProfiles::from_projected_json(source, &catalogue, &numbers.profiles())' 1 'image publishes selected templates with the actual Number owner'
check_no_inline_legacy_includes "$intl_relative_projection"
# Exact native admission also validates its retained private Number domain.
check_raw_line_budget "$intl_relative_projection" 335
require_file crates/scriptw-intl/src/relative_time_image/projection/tests.rs

# DisplayNames projection removes unselected locale/currency data and
# unreachable typed pools. Exact rederivation mints the private native catalogue.
intl_display_image="crates/scriptw-intl/src/display_names_image.rs"
intl_display_projection="crates/scriptw-intl/src/display_names_image/projection.rs"
intl_display_profiles="crates/scriptw-intl/src/display_names/profiles.rs"
require_file "$intl_display_projection"
require_exact_line_count "$intl_display_image" 'mod projection;' 1 'private selected DisplayNames physical producer/admission'
require_exact_line_count "$intl_display_image" 'pub(crate) use projection::DisplayNamesCatalogue;' 1 'consumed private borrowed native catalogue'
for display_projection_entry in produce admit; do
  require_regex_count "$intl_display_projection" "^[[:space:]]*pub\\(super\\) fn ${display_projection_entry}[<(]" 1 'real selected DisplayNames factory/admission body, distinct from the currency producer'
  require_fixed_string_count "$intl_display_image" "projection::${display_projection_entry}(" 1 'actual image consumes selected DisplayNames body'
done
require_fixed_string_count "$intl_display_projection" "pub(crate) struct DisplayNamesCatalogue<'a>" 1 'only exact admission mints a borrowed complete-row catalogue'
require_fixed_string_count "$intl_display_projection" 'if payload != expected.as_slice()' 1 'actual reader requires complete source rederivation'
require_fixed_string_count "$intl_display_projection" 'raw["name_pool"] = serde_json::Value::Array(selected_pools);' 1 'locale producer physically publishes reachable typed pools'
require_fixed_string_count "$intl_display_projection" 'raw["name_pool"] = serde_json::Value::Array(pools);' 1 'currency producer physically publishes the filtered remapped pools'
require_fixed_string_count "$intl_display_image" 'projection::produce_data(' 1 'actual paired currency projection consumer'
require_fixed_string_count "$intl_display_image" 'pub(crate) fn currency_codes(&self)' 1 'same admitted currency domain is consumed by provider admission'
require_fixed_string_count "$intl_display_projection" 'source_pool_indices,' 1 'source association order is bound by descriptor'
require_fixed_string_count "$intl_display_profiles" "catalogue: &crate::display_names_image::DisplayNamesCatalogue<'_>," 1 'actual native decoder consumes private admitted catalogue'
require_fixed_string_count "$intl_display_profiles" 'serde_json::from_slice(catalogue.bytes())' 1 'selected native operation tables consume actual projected bytes'
check_no_inline_legacy_includes "$intl_display_projection"
# Currency and locale production share this catalogue; retain private authority
# checks instead of the old locale-only line ceilings.
intl_display_catalogue_fields="$(sed -n "/^pub(crate) struct DisplayNamesCatalogue<'a> {$/,/^}$/p" "$intl_display_projection")"
require_text_regex_count "$intl_display_catalogue_fields" '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+' 0 'no caller-manufactured sparse currency catalogue'
require_fixed_string_present "$intl_display_profiles" 'owner.currency_codes()' 'native sparse Currency admission consumes the same closed domain'
require_file crates/scriptw-intl/src/display_names_image/projection/tests.rs
require_fixed_string_count crates/scriptw-intl/src/display_names_image/projection/tests.rs 'fn selected_rows_keep_complete_reachable_pools_and_the_real_fallback_catalogue()' 1 'complete six-domain/three-width/fallback/pool closure control'
require_fixed_string_count crates/scriptw-intl/src/display_names_image/projection/tests.rs 'fn self_consistent_changed_rows_pools_and_provenance_cannot_claim_the_projection()' 1 'actual rederived payload damage refusal control'

# One checked Custom identity composes actual producers for eight independent
# locale selectors, data dimensions and a closed public service/frame owner.
require_exact_line_count "$intl_selection" '    CustomProjection(CustomIntlProfile),' 1 'closed combined compilation selection'
require_exact_line_count "$intl_selection" 'pub struct CustomIntlProfile {' 1 'sole combined Custom input owner'
intl_custom_fields="$(sed -n '/^pub struct CustomIntlProfile {$/,/^}$/p' "$intl_selection")"
require_text_regex_count "$intl_custom_fields" '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+' 0 'private validated combined input fields'
intl_custom_constructor="$(sed -n '/^impl CustomIntlProfile {$/,/^pub enum InvalidCustomIntlProfile {/p' "$intl_selection")"
require_text_regex_count "$intl_custom_constructor" '^[[:space:]]+pub fn new\(' 1 'single consumed combined Custom constructor'
require_fixed_string_count "$intl_selection" 'if list_locales.is_none()' 1 'no-filter admission checks the actual List field'
for intl_locale_field in relative_time display_names duration number date_time collator segmenter; do
  require_fixed_string_count "$intl_selection" "&& ${intl_locale_field}_locales.is_none()" 1 'no-filter admission checks every independent locale field'
  require_fixed_string_count "$intl_selection" "pub fn ${intl_locale_field}_locales(&self) -> Option<&[LocaleId]>" 1 'actual checked component locale getter'
  require_fixed_string_count crates/scriptw-engine/src/lib.rs "hash_optional_program_cache_locales(&mut hash, selection.${intl_locale_field}_locales())" 1 'cache consumes each independent locale selector'
done
for intl_locale_factory in ListDataImage RelativeTimeDataImage DisplayNamesDataImage DurationDataImage NumberProfilesDataImage DateTimeDataImage CollatorDataImage SegmenterDataImage; do
  require_fixed_string_present "$intl_selection" "${intl_locale_factory}::for_custom_projection(" 'selected bundle consumes the actual component producer'
done
require_fixed_string_count crates/scriptw-engine/src/lib.rs 'const PROGRAM_WASM_CACHE_KEY_DOMAIN: &[u8] = b"scriptw-program-wasm-cache-key-v20";' 1 'current component/data/service/language-policy cache framing domain'
require_fixed_string_count crates/scriptw-engine/src/lib.rs 'hash.update(PROGRAM_WASM_CACHE_KEY_DOMAIN);' 1 'actual cache consumes its single declared framing domain'
require_fixed_string_count crates/scriptw-engine/src/lib.rs 'hash.update(options.language_features.wire_code().to_le_bytes());' 1 'actual cache consumes the complete typed language policy'
require_fixed_string_count crates/scriptw-engine/src/lib.rs 'fn hash_optional_program_cache_locales(' 1 'presence/count/length-framed component locale owner'
require_fixed_string_count crates/scriptw-engine/src/lib.rs 'hash_optional_program_cache_locales(' 9 'one cache field owner consumed by all eight independent filters'
require_fixed_string_count crates/scriptw-engine/src/lib.rs '.map(CustomListProfile::requested_locales)' 1 'cache consumes the actual List filter'
for intl_data_dimension in currency_codes date_time_calendars numbering_systems named_time_zones service_selection; do
  require_fixed_string_count crates/scriptw-engine/src/lib.rs "match selection.${intl_data_dimension}()" 1 'cache independently frames each checked data/service dimension'
done
require_fixed_string_count crates/scriptw-engine/src/lib.rs 'hash.update(services.wire().to_le_bytes());' 1 'cache binds the actual public service set'
require_exact_line_count crates/scriptw-cli/src/main.rs 'enum IntlLocaleProjectionComponent {' 1 'private closed CLI component input authority'
require_fixed_string_count crates/scriptw-cli/src/main.rs 'fn parse(argument: &str) -> Option<Self>' 1 'sole open-string CLI component boundary'
require_fixed_string_count crates/scriptw-cli/src/main.rs 'IntlLocaleProjectionComponent::parse(argument)' 1 'single-pass argument cursor consumes the checked component token'
for cli_projection_route in 'List:list' 'RelativeTime:relative_time' 'DisplayNames:display_names' 'Duration:duration' 'Number:number' 'DateTime:date_time' 'Collator:collator' 'Segmenter:segmenter'; do
  cli_projection_component="${cli_projection_route%%:*}"
  cli_projection_field="${cli_projection_route#*:}"
  require_fixed_string_count crates/scriptw-cli/src/main.rs "IntlLocaleProjectionComponent::${cli_projection_component} => &mut ${cli_projection_field}_locales," 1 'exhaustive checked component chooses its own real field'
done
for cli_projection_flag in '--intl-list-locales:List' '--intl-relative-time-locales:RelativeTime' '--intl-displaynames-locales:DisplayNames' '--intl-duration-locales:Duration' '--intl-number-locales:Number' '--intl-datetime-locales:DateTime' '--intl-collator-locales:Collator' '--intl-segmenter-locales:Segmenter'; do
  cli_projection_spelling="${cli_projection_flag%%:*}"
  cli_projection_token="${cli_projection_flag#*:}"
  require_fixed_string_count crates/scriptw-cli/src/main.rs "\"${cli_projection_spelling}\" => Some(Self::${cli_projection_token})," 1 'actual locale flag spelling mints its closed token'
done
for cli_projection_argument in lists relative_times display_names durations numbers date_times collators segmenters; do
  require_fixed_string_present crates/scriptw-cli/src/main.rs "${cli_projection_argument}.as_deref()" 'CLI transports its checked locale field to the combined constructor'
done
require_fixed_string_present crates/scriptw-cli/src/main.rs 'CustomIntlProfile::from_manifest_json(json)' 'CLI consumes the same strict dimension/service manifest owner'
intl_service_owner="crates/scriptw-intl/src/service_selection.rs"
intl_bundle_export="crates/scriptw-intl/src/selection/export.rs"
intl_bundle_manifest="crates/scriptw-intl/src/selection/manifest.rs"
intl_selected_provider="crates/scriptw-intl/src/provider.rs"
for intl_service_path in "$intl_service_owner" "$intl_bundle_export" "$intl_bundle_manifest"; do
  require_file "$intl_service_path"
  check_no_inline_legacy_includes "$intl_service_path"
done
require_exact_line_count crates/scriptw-intl/src/lib.rs 'mod service_selection;' 1 'private closed service/dependency owner attachment'
require_exact_line_count "$intl_selection" 'mod export;' 1 'private actual sparse bundle framing owner'
require_exact_line_count "$intl_selection" 'mod manifest;' 1 'private checked dimension/service manifest owner'
require_exact_line_count "$intl_service_owner" 'pub struct IntlDataComponentSet(u16);' 1 'private closed component-set bits'
require_exact_line_count "$intl_service_owner" 'pub struct CheckedIntlServiceSelection {' 1 'single checked service-to-frame authority'
intl_service_fields="$(sed -n '/^pub struct CheckedIntlServiceSelection {$/,/^}$/p' "$intl_service_owner")"
require_text_regex_count "$intl_service_fields" '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+' 0 'no raw caller-branded service closure'
require_fixed_string_count "$intl_service_owner" 'let required: &[C] = match service {' 1 'every closed service derives its actual dependency frames'
require_fixed_string_count "$intl_service_owner" 'if bits == 0 || bits & !allowed != 0' 1 'wire admission refuses empty or unknown services'
require_fixed_string_count "$intl_selection" 'if checked.contains_component(IntlDataComponent::$kind)' 1 'only retained component factories execute'
require_fixed_string_count "$intl_selection" 'EmbeddedIntlProvider::with_selected_data_images(' 1 'bundle publishes through the checked native foundation gate'
require_fixed_string_count "$intl_selection" 'pub fn component_images(&self) -> Vec<(IntlDataComponent, Arc<[u8]>)>' 1 'typed physical frames are shared by export, emission and footprint accounting'
require_fixed_string_count "$intl_selection" '.map(|(component, bytes)| (component.section_name(), bytes))' 1 'section emission projects only the actual physical frame inventory'
require_fixed_string_count "$intl_selected_provider" 'pub fn with_selected_data_images(' 1 'sole sparse native publication gate'
require_fixed_string_count "$intl_selected_provider" 'services.contains_component(*component) != digest.is_some()' 1 'admission requires the exact retained foundation set'
require_fixed_string_count "$intl_selected_provider" 'if services.requested() == IntlServiceSet::ALL' 1 'complete selection retains original full-image admission'
require_fixed_string_count "$intl_selected_provider" 'let crate::IntlDataProfile::Custom(id) = locale.profile() else {' 1 'partial service/frame sets require an actual Custom foundation'
require_fixed_string_count "$intl_selected_provider" 'if !self.services.permits(service)' 1 'retained dependencies do not authorize absent public services'
require_fixed_string_present "$intl_selected_provider" 'data.ok_or(crate::UnavailableIntlService(service))' 'missing service data fails explicitly without embedded fallback'
for intl_public_service in Locale DateTimeFormat NumberFormat PluralRules ListFormat Collator DisplayNames DurationFormat Segmenter; do
  require_fixed_string_present "$intl_selected_provider" "self.service_data(IntlService::${intl_public_service}," 'actual public operation consumes its requested service authority'
done
intl_relative_public_operation="$(braced_rust_item_source "$intl_selected_provider" '^impl IntlOperationProvider<ResolveRelativeTimeLocale>')"
require_text_regex_count "$intl_relative_public_operation" 'self\.service_data\(' 1 'public RelativeTime resolution consumes service admission'
require_text_regex_count "$intl_relative_public_operation" 'IntlService::RelativeTimeFormat,' 2 'public RelativeTime admission and its retained Number foundation use the same service token'
require_fixed_string_count "$intl_bundle_export" 'if count != checked.components().len()' 1 'export-v2 reader requires exactly the derived frame count'
require_fixed_string_present "$intl_bundle_export" 'CheckedIntlServiceSelection::from_wire(wire)' 'sparse export re-admits the closed service owner'
require_fixed_string_count "$intl_bundle_export" '.eq(checked.components().iter())' 1 'export re-admission requires canonical exact component identities'
require_fixed_string_count "$intl_bundle_export" 'Self::from_component_sections(selection, frames)?' 1 'wire admission uses the actual native owner decoder'
require_fixed_string_present "$intl_bundle_manifest" 'if version.schema_version == 6' 'strict service manifest owns its current version'
require_fixed_string_present "$intl_selection" 'fn validate_service_dimensions(' 'dimensions cannot decorate omitted frames'
for intl_dimension_setter in with_currency_codes with_date_time_calendars with_numbering_systems with_named_time_zones with_services; do
  require_fixed_string_present "$intl_selection" "pub fn ${intl_dimension_setter}(" 'SDK consumes checked data/service selection'
  require_fixed_string_present "$intl_bundle_manifest" "$intl_dimension_setter" 'manifest consumes the same typed dimension owner'
done
for intl_dimensional_factory in \
  'NumberProfilesDataImage::for_custom_data_projection(' \
  'NumberProfilesDataImage::for_custom_numbering_projection(' \
  'DisplayNamesDataImage::for_custom_data_projection(' \
  'DateTimeDataImage::for_custom_data_projection(' \
  'DateTimeDataImage::for_custom_numbering_projection(' \
  'NamedTimeZoneDataImage::for_custom_projection('
do
  require_fixed_string_present "$intl_selection" "$intl_dimensional_factory" 'checked selector reaches a real physical native producer'
done
for intl_paired_domain in \
  'numbers.currency_codes() != display.currency_codes()' \
  'numbers.numbering_system_selection() != date_time.numbering_system_selection()' \
  'date_time.named_time_zone_selection() != named.named_time_zone_selection()' \
  'names.named_time_zone_selection() != named.named_time_zone_selection()'
do
  require_fixed_string_present "$intl_selected_provider" "$intl_paired_domain" 'native admission binds shared selected domains to their retained owners'
done
for intl_rederived_projection in number_image datetime_image; do
  intl_rederived_path="crates/scriptw-intl/src/${intl_rederived_projection}/projection.rs"
  require_file "$intl_rederived_path"
  require_fixed_string_count "$intl_rederived_path" 'if payload != expected.as_slice()' 1 'projected wire bytes require complete pinned source rederivation'
done
require_tree_regex_count crates/scriptw-engine/src 'CustomListProjection' 0 'no obsolete List-only product profile'
require_tree_regex_count crates/scriptw-cli/src 'CustomListProjection' 0 'no obsolete List-only CLI profile'
require_fixed_string_count scripts/generate-intl-display-relative-identity.py "rtf |= production_sources(repository, 'crates/scriptw-intl/src/relative_time_image')" 1 'RelativeTime recipe binds its actual projection child'
require_fixed_string_count scripts/generate-intl-display-relative-identity.py "dn |= production_sources(repository, 'crates/scriptw-intl/src/display_names_image')" 1 'DisplayNames recipe binds its actual consumed projection child'
for projection_recipe in scripts/generate-intl-list-identity.py scripts/generate-intl-display-relative-identity.py; do
  require_fixed_string_count "$projection_recipe" 'crates/scriptw-intl/src/selection.rs' 1 'actual combined SDK selection in each component source recipe'
done
for combined_projection_control in \
  crates/scriptw-engine/tests/aot_intl/aot_intl_compilation_profile.rs \
  crates/scriptw-cli/tests/cli/intl.rs \
  crates/scriptw-engine/tests/fixtures/intl_compilation_profile/projected_list.js \
  crates/scriptw-engine/tests/fixtures/intl_compilation_profile/projected_list_relative_time.js \
  crates/scriptw-engine/tests/fixtures/intl_compilation_profile/projected_display_names.js
do
  require_file "$combined_projection_control"
done
require_fixed_string_count crates/scriptw-engine/src/tests/cache.rs 'fn component_projection_cache_keys_bind_each_filter_presence_and_order_independently()' 1 'real artifact cache tuple control'
require_fixed_string_count crates/scriptw-cli/src/main.rs 'fn relative_and_combined_projection_arguments_are_checked_before_source_loading()' 1 'actual CLI admission and ordering control'
require_fixed_string_count crates/scriptw-engine/tests/aot_intl/aot_intl_compilation_profile.rs 'fn independent_relative_and_combined_projections_emit_exact_rows_and_remint_selected_wire_owners()' 1 'actual emitted-frame and wire-owner control'
require_fixed_string_count crates/scriptw-cli/tests/cli/intl.rs 'fn combined_projection_cli_build_matches_sdk_and_runs_both_selected_services()' 1 'actual CLI/SDK artifact equality and operation control'
require_fixed_string_count crates/scriptw-engine/src/tests/cache.rs 'fn display_names_cache_filters_are_independent_of_other_components_and_input_order()' 1 'actual independent third cache-field control'
require_fixed_string_count crates/scriptw-cli/src/main.rs 'fn display_names_projection_arguments_are_checked_before_source_loading()' 1 'actual three-component CLI admission ordering control'
require_fixed_string_count crates/scriptw-engine/tests/aot_intl/aot_intl_compilation_profile.rs 'fn display_names_and_three_component_projections_emit_selected_tables_and_run_real_consumers()' 1 'actual physical frames/foreign/wire/script/module consumer control'
require_fixed_string_count crates/scriptw-cli/tests/cli/intl.rs 'fn display_names_cli_projection_matches_sdk_and_composes_all_three_filters()' 1 'actual CLI/SDK three-component producer parity control'


# Exact exporter data has one narrow generated expansion boundary. The
# handwritten Intl kernel keeps the workspace ban on unsafe operations.
require_exact_line_count Cargo.toml 'unsafe_code = "forbid"' 1 'handwritten workspace unsafe ban'
require_exact_line_count crates/scriptw-intl/Cargo.toml 'workspace = true' 1 'Intl workspace lint inheritance'
collator_data=crates/scriptw-intl-collator-data
require_exact_line_count "$collator_data/Cargo.toml" 'unsafe_code = "deny"' 1 'data crate default unsafe denial'
require_exact_line_count "$collator_data/src/lib.rs" '#![deny(unsafe_code)]' 1 'data root unsafe denial'
require_exact_line_count "$collator_data/src/lib.rs" '#[allow(unsafe_code)]' 1 'private generated expansion allowance'
require_exact_line_count "$collator_data/src/lib.rs" 'mod baked;' 1 'private baked provider owner'
require_exact_line_count "$collator_data/src/lib.rs" 'pub use baked::PinnedCollatorProvider;' 1 'sole authored provider export'
require_exact_line_count "$collator_data/src/baked.rs" 'pub struct PinnedCollatorProvider;' 1 'immutable pinned provider'
require_fixed_string_count "$collator_data/src/baked.rs" 'make_provider!(PinnedCollatorProvider);' 1 'exact exporter provider marker'
require_regex_count "$collator_data/src/baked.rs" '^impl_(collation|normalizer)_[a-z_0-9]+!' 9 'exact consumed data marker expansions'
require_tree_regex_count "$collator_data/src" '(^|[^[:alnum:]_])unsafe[[:space:]]+(fn|impl|trait|extern|\{)' 0 'handwritten unsafe operations'

# Source ownership checks; runtime conformance is a separate batch gate.
wasm_collator=crates/scriptw-aot-wasm/src/builtins/intl_collator.rs
wasm_collator_compare=crates/scriptw-aot-wasm/src/builtins/intl_collator/compare.rs
require_file "$wasm_collator"
for collator_child in construction options compare resolved pool; do
  collator_child_path="crates/scriptw-aot-wasm/src/builtins/intl_collator/${collator_child}.rs"
  require_file "$collator_child_path"
  require_exact_line_count "$wasm_collator" "mod ${collator_child};" 1 'private Collator child attachment'
  check_no_inline_legacy_includes "$collator_child_path"
done
require_exact_line_count "$wasm_collator" "enum CollatorProviderRequest<'a> {" 1 'private closed Collator provider request domain'
require_exact_line_count "$wasm_collator" 'struct CollatorProviderResponse {' 1 'private Collator GC response owner'
require_exact_line_count "$wasm_collator_compare" \
  'pub(super) struct CompletedCollatorCompareStringsLocals {' 1 'completed ordered Collator strings owner'
collator_completed_fields="$(sed -n '/^pub(super) struct CompletedCollatorCompareStringsLocals {$/,/^}$/p' "$wasm_collator_compare")"
if grep -Eq '^[[:space:]]+pub(\([^)]*\))?[[:space:]]+' <<<"$collator_completed_fields"; then
  fail "$wasm_collator_compare must keep completed comparison string fields private"
fi
require_fixed_string_count "$wasm_collator_compare" \
  'BuiltinClosurePayload::IntlCollator(&record)' 1 'rooted Collator record in the cached bound closure'
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/intl_collator/resolved.rs 'self.emit_array_from_argument_list(&list, function)?;' 1 'Collator completed list Array publisher'
require_tree_regex_count crates/scriptw-aot-wasm/src '^[[:space:]]*pub\(in crate::builtins\)[[:space:]]+fn[[:space:]]+emit_intl_is_unicode_type_i32[[:space:]]*\(' 1 'shared real Unicode-type validator owner'
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/intl_number.rs 'mod unicode_type;' 1 'shared Unicode-type child'
for collator_method in emit_intl_collator_constructor emit_intl_collator_supported_locales_of emit_intl_collator_resolved_options emit_intl_collator_compare_getter emit_intl_collator_bound_compare; do
  require_tree_regex_count crates/scriptw-aot-wasm/src "^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+${collator_method}[[:space:]]*\(" 1 'actual Collator emitter owner'
  require_fixed_string_count "$wasm_standard_builtins" "self.${collator_method}(function)?;" 1 'fixed standard Collator entry'
done
# localeCompare is a String operation: its whole receiver/arguments enter the
# same Collator owner through string.rs, rather than a sixth Standard delegate.
require_regex_count "$wasm_collator_compare" \
  '^[[:space:]]*pub\(crate\)[[:space:]]+fn[[:space:]]+emit_intrinsic_string_locale_compare[[:space:]]*\(' 1 'String localeCompare Collator owner'
require_fixed_string_count crates/scriptw-aot-wasm/src/builtins/string.rs \
  'b.emit_intrinsic_string_locale_compare(' 1 'whole-value String localeCompare consumer'
require_fixed_string_count "$wasm_standard_builtins" \
  'self.emit_string_prototype_locale_compare_builtin(function)?' 1 'fixed standard String localeCompare entry'
require_file crates/scriptw-engine/tests/aot_intl/aot_intl_collator.rs
require_file crates/scriptw-engine/tests/fixtures/intl_collator/ordered_argument_conversion.js
require_file crates/scriptw-engine/tests/fixtures/intl_collator/brand_and_cache.js
require_file crates/scriptw-engine/tests/fixtures/intl_collator/called_function_realms.js

# The DurationFormat family has private production owners; its real GC record
# is declared by the central schema rather than a passive linear layout.
wasm_duration_format=crates/scriptw-aot-wasm/src/builtins/intl_durationformat.rs
require_file "$wasm_duration_format"
require_exact_line_count crates/scriptw-aot-wasm/src/builtins/mod.rs 'mod intl_durationformat;' 1 'private DurationFormat module'
if grep -Eq '^(pub(\([^)]*\))?[[:space:]]+)mod[[:space:]]+intl_durationformat;' crates/scriptw-aot-wasm/src/builtins/mod.rs; then
  fail 'DurationFormat family must remain a private module'
fi
for duration_format_child in construction inputs options pool render resolved temporal; do
  require_exact_line_count "$wasm_duration_format" "mod $duration_format_child;" 1 "private DurationFormat $duration_format_child child"
  require_file "crates/scriptw-aot-wasm/src/builtins/intl_durationformat/$duration_format_child.rs"
  check_no_inline_legacy_includes "crates/scriptw-aot-wasm/src/builtins/intl_durationformat/$duration_format_child.rs"
done
check_no_inline_legacy_includes "$wasm_duration_format"

# Locale information lists have a closed selector and one framed GC reader.
wasm_locale_information_list=crates/scriptw-aot-wasm/src/builtins/intl/locale_information_list.rs
for locale_list_child in calendars collations time_zones locale_information_list; do
  locale_list_path="crates/scriptw-aot-wasm/src/builtins/intl/${locale_list_child}.rs"
  require_file "$locale_list_path"
  require_exact_line_count crates/scriptw-aot-wasm/src/builtins/intl.rs "mod ${locale_list_child};" 1 'private Locale list child attachment'
  check_no_inline_legacy_includes "$locale_list_path"
  check_raw_line_budget "$locale_list_path" 400
done
require_exact_line_count "$wasm_locale_information_list" \
  'pub(super) enum LocaleInformationKind {' 1 'closed family-owned Locale list selector'
require_regex_count "$wasm_locale_information_list" \
  '^[[:space:]]*pub\(super\)[[:space:]]+fn[[:space:]]+emit_intl_locale_information_array[[:space:]]*\(' 1 'checked GC Locale list reader and Array publisher'
require_regex_count "$wasm_locale_information_list" \
  '^[[:space:]]*fn[[:space:]]+emit_validate_locale_information_name[[:space:]]*\(' 1 'private completed Locale list name validation'
require_fixed_string_count "$wasm_locale_information_list" \
  'IntlByteArrayReader::new(&response, schema, function)' 1 'GC ByteArray Locale response reader'
require_fixed_string_count "$wasm_locale_information_list" \
  'scriptw_intl::LOCALE_INFORMATION_WIRE_VERSION as i64' 1 'Locale response protocol version admission'
require_fixed_string_count "$wasm_locale_information_list" \
  'kind.operation().code()' 1 'closed Locale response operation admission'
require_fixed_string_count "$wasm_locale_information_list" \
  'self.emit_array_from_argument_list(&values, function)?' 1 'completed GC Locale List Array publication'
for locale_list_entry in calendars:Calendars collations:Collations time_zones:TimeZones; do
  locale_list_child="${locale_list_entry%%:*}"
  locale_list_kind="${locale_list_entry#*:}"
  locale_list_path="crates/scriptw-aot-wasm/src/builtins/intl/${locale_list_child}.rs"
  require_fixed_string_count "$locale_list_path" \
    'self.emit_intl_locale_information_array(' 1 'thin Locale information consumer'
  require_fixed_string_count "$locale_list_path" \
    "LocaleInformationKind::${locale_list_kind}," 1 'closed Locale information kind selection'
done
require_file crates/scriptw-intl/src/locale_information_wire.rs
require_module_decl crates/scriptw-intl/src/lib.rs locale_information_wire
require_file crates/scriptw-engine/tests/aot_intl/aot_intl_locale_information_lists.rs

# Chinese/Dangi retained data and emitted year models have private owners.
require_module_decl crates/scriptw-aot-wasm/src/data.rs temporal_east_asian_years
require_file crates/scriptw-aot-wasm/src/data/temporal_east_asian_years.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/data/temporal_east_asian_years.rs
for east_asian_data_child in construction format; do
  require_module_decl crates/scriptw-aot-wasm/src/data/temporal_east_asian_years.rs "$east_asian_data_child"
  require_file "crates/scriptw-aot-wasm/src/data/temporal_east_asian_years/$east_asian_data_child.rs"
  check_no_inline_legacy_includes "crates/scriptw-aot-wasm/src/data/temporal_east_asian_years/$east_asian_data_child.rs"
done
require_file crates/scriptw-aot-wasm/build.rs
require_module_decl crates/scriptw-aot-wasm/src/builtins/temporal_calendar_arithmetic.rs east_asian
require_file crates/scriptw-aot-wasm/src/builtins/temporal_calendar_arithmetic/east_asian.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/builtins/temporal_calendar_arithmetic/east_asian.rs
for east_asian_child in proleptic month_day; do
  require_module_decl crates/scriptw-aot-wasm/src/builtins/temporal_calendar_arithmetic/east_asian.rs "$east_asian_child"
  require_file "crates/scriptw-aot-wasm/src/builtins/temporal_calendar_arithmetic/east_asian/${east_asian_child}.rs"
  check_no_inline_legacy_includes "crates/scriptw-aot-wasm/src/builtins/temporal_calendar_arithmetic/east_asian/${east_asian_child}.rs"
done

# Calendar arithmetic and differences have consumed private owner modules.
require_module_decl crates/scriptw-aot-wasm/src/builtins/temporal_calendar_arithmetic.rs hebrew
require_file crates/scriptw-aot-wasm/src/builtins/temporal_calendar_arithmetic/hebrew.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/builtins/temporal_calendar_arithmetic/hebrew.rs
for calendar_difference_parent in temporal_plain_date_methods temporal_plain_year_month_methods; do
  calendar_difference_source="crates/scriptw-aot-wasm/src/builtins/${calendar_difference_parent}.rs"
  calendar_difference_leaf="crates/scriptw-aot-wasm/src/builtins/${calendar_difference_parent}/difference.rs"
  require_module_decl "$calendar_difference_source" difference
  require_file "$calendar_difference_leaf"
  check_no_inline_legacy_includes "$calendar_difference_leaf"
done

# String hooks consume one private observed-method owner.
require_module_decl crates/scriptw-aot-wasm/src/builtins/string.rs symbol_method
require_file crates/scriptw-aot-wasm/src/builtins/string/symbol_method.rs
check_no_inline_legacy_includes crates/scriptw-aot-wasm/src/builtins/string/symbol_method.rs

# The product and raw GC ABI fixtures consume the same agent operation handler.
require_module_decl crates/scriptw-engine/src/lib.rs wasm_agent_host
require_file crates/scriptw-engine/src/wasm_agent_host.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_agent_host.rs 200
check_no_inline_legacy_includes crates/scriptw-engine/src/wasm_agent_host.rs
require_fixed_string_count crates/scriptw-engine/src/lib.rs \
  'wasm_agent_host::agent_call,' 1 'shared native agent operation handler production owner'
require_fixed_string_count crates/scriptw-engine/src/tests.rs \
  'wasm_agent_host::agent_call,' 1 'shared native agent operation handler test control'
require_fixed_string_count crates/scriptw-engine/src/wasm_agent_host.rs \
  'pub(super) fn agent_call(' 1 'single actual agent dispatch owner'

# Private typed GC allocation reports only actual collector OOM as a nullable result.
require_module_decl crates/scriptw-engine/src/lib.rs wasm_gc_byte_array_host
require_file crates/scriptw-engine/src/wasm_gc_byte_array_host.rs
check_raw_line_budget crates/scriptw-engine/src/wasm_gc_byte_array_host.rs 180
check_no_inline_legacy_includes crates/scriptw-engine/src/wasm_gc_byte_array_host.rs
require_fixed_string_count crates/scriptw-engine/src/wasm_gc_host.rs \
  'wasm_gc_byte_array_host::link(linker, module)?;' 1 'current collector allocation linker'
