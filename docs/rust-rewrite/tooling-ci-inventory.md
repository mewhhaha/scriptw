# Tooling CI discovery

The Linux job in [tooling-contracts.yaml](../../.github/workflows/tooling-contracts.yaml)
discovers every `scripts/tests/test_*.py` module. The macOS job retains the portable
`test_tooling_*.py` and `test_limited_verification.py` controls. New Linux test modules
enter discovery automatically; update this index when adding modules. The main CI
workflow also owns both shell controls. These are tooling controls, not Test262 results.

The 2026-10-21 audit of the preceding 60-module tree found 24 explicitly named or
glob-selected in workflows and 36 without an explicit invocation. Linux discovery
closes that gap. OS-specific controls retain their declared platform requirements;
cross-filesystem storage controls require distinct devices for temporary and workspace
paths and otherwise report a skip. Skips are not passing evidence for those controls.

| Module | Linux | macOS |
| --- | --- | --- |
| [test_compiler_comparison.py](../../scripts/tests/test_compiler_comparison.py) | All-module discovery | — |
| [test_tooling_benchmark_boundary.py](../../scripts/tests/test_tooling_benchmark_boundary.py) | All-module discovery | Portable discovery |
| [test_audit_test262_replay.py](../../scripts/tests/test_audit_test262_replay.py) | All-module discovery | — |
| [test_ci_targets.py](../../scripts/tests/test_ci_targets.py) | All-module discovery | — |
| [test_collator_documentation_projection.py](../../scripts/tests/test_collator_documentation_projection.py) | All-module discovery | — |
| [test_collect_new_test262_failures.py](../../scripts/tests/test_collect_new_test262_failures.py) | All-module discovery | — |
| [test_conformance_owners.py](../../scripts/tests/test_conformance_owners.py) | All-module discovery | — |
| [test_conformance_source_pins.py](../../scripts/tests/test_conformance_source_pins.py) | All-module discovery | — |
| [test_engine_regression_inventory.py](../../scripts/tests/test_engine_regression_inventory.py) | All-module discovery | — |
| [test_evidence_metadata.py](../../scripts/tests/test_evidence_metadata.py) | All-module discovery | — |
| [test_generate_intl_datetime_profile.py](../../scripts/tests/test_generate_intl_datetime_profile.py) | All-module discovery | — |
| [test_generate_intl_displaynames_profile.py](../../scripts/tests/test_generate_intl_displaynames_profile.py) | All-module discovery | — |
| [test_generate_intl_keyword_aliases.py](../../scripts/tests/test_generate_intl_keyword_aliases.py) | All-module discovery | — |
| [test_generate_intl_locale_calendars_profile.py](../../scripts/tests/test_generate_intl_locale_calendars_profile.py) | All-module discovery | — |
| [test_generate_intl_locale_collations_identity.py](../../scripts/tests/test_generate_intl_locale_collations_identity.py) | All-module discovery | — |
| [test_generate_intl_locale_hour_cycles_profile.py](../../scripts/tests/test_generate_intl_locale_hour_cycles_profile.py) | All-module discovery | — |
| [test_generate_intl_locale_text_profile.py](../../scripts/tests/test_generate_intl_locale_text_profile.py) | All-module discovery | — |
| [test_generate_intl_locale_time_zones_profile.py](../../scripts/tests/test_generate_intl_locale_time_zones_profile.py) | All-module discovery | — |
| [test_generate_intl_locale_week_profile.py](../../scripts/tests/test_generate_intl_locale_week_profile.py) | All-module discovery | — |
| [test_generate_intl_named_time_zones.py](../../scripts/tests/test_generate_intl_named_time_zones.py) | All-module discovery | — |
| [test_generate_intl_time_zone_names.py](../../scripts/tests/test_generate_intl_time_zone_names.py) | All-module discovery | — |
| [test_independent_global_prelude.py](../../scripts/tests/test_independent_global_prelude.py) | All-module discovery | — |
| [test_intl_cldr_profile.py](../../scripts/tests/test_intl_cldr_profile.py) | All-module discovery | — |
| [test_intl_cross_host.py](../../scripts/tests/test_intl_cross_host.py) | All-module discovery | — |
| [test_intl_display_relative_identity.py](../../scripts/tests/test_intl_display_relative_identity.py) | All-module discovery | — |
| [test_intl_duration_identity.py](../../scripts/tests/test_intl_duration_identity.py) | All-module discovery | — |
| [test_intl_duration_profile.py](../../scripts/tests/test_intl_duration_profile.py) | All-module discovery | — |
| [test_intl_locale_numbering_systems_identity.py](../../scripts/tests/test_intl_locale_numbering_systems_identity.py) | All-module discovery | — |
| [test_intl_numberformat_memoization.py](../../scripts/tests/test_intl_numberformat_memoization.py) | All-module discovery | — |
| [test_intl_positional_numbering.py](../../scripts/tests/test_intl_positional_numbering.py) | All-module discovery | — |
| [test_intl_relative_time_profile.py](../../scripts/tests/test_intl_relative_time_profile.py) | All-module discovery | — |
| [test_intl_segmenter_identity.py](../../scripts/tests/test_intl_segmenter_identity.py) | All-module discovery | — |
| [test_intl_source_identities.py](../../scripts/tests/test_intl_source_identities.py) | All-module discovery | — |
| [test_limited_verification.py](../../scripts/tests/test_limited_verification.py) | All-module discovery | Portable discovery |
| [test_native_program_templates.py](../../scripts/tests/test_native_program_templates.py) | All-module discovery | — |
| [test_native_verification_helpers.py](../../scripts/tests/test_native_verification_helpers.py) | All-module discovery | — |
| [test_native_verification_worker.py](../../scripts/tests/test_native_verification_worker.py) | All-module discovery | — |
| [test_prepare_test262_errata.py](../../scripts/tests/test_prepare_test262_errata.py) | All-module discovery | — |
| [test_publication_progress.py](../../scripts/tests/test_publication_progress.py) | All-module discovery | — |
| [test_readme_test_graph.py](../../scripts/tests/test_readme_test_graph.py) | All-module discovery | — |
| [test_replay_test262_executions.py](../../scripts/tests/test_replay_test262_executions.py) | All-module discovery | — |
| [test_repository_paths.py](../../scripts/tests/test_repository_paths.py) | All-module discovery | — |
| [test_robustness_campaign_tier.py](../../scripts/tests/test_robustness_campaign_tier.py) | All-module discovery | — |
| [test_run_aot_unit_shard.py](../../scripts/tests/test_run_aot_unit_shard.py) | All-module discovery | — |
| [test_shortcut_status.py](../../scripts/tests/test_shortcut_status.py) | All-module discovery | — |
| [test_test262_execution_batches.py](../../scripts/tests/test_test262_execution_batches.py) | All-module discovery | — |
| [test_test262_shard_report.py](../../scripts/tests/test_test262_shard_report.py) | All-module discovery | — |
| [test_tooling_campaign_cloud.py](../../scripts/tests/test_tooling_campaign_cloud.py) | All-module discovery | Portable discovery |
| [test_tooling_capped.py](../../scripts/tests/test_tooling_capped.py) | All-module discovery | Portable discovery |
| [test_tooling_dev.py](../../scripts/tests/test_tooling_dev.py) | All-module discovery | Portable discovery |
| [test_tooling_feature_probe.py](../../scripts/tests/test_tooling_feature_probe.py) | All-module discovery | Portable discovery |
| [test_tooling_identity.py](../../scripts/tests/test_tooling_identity.py) | All-module discovery | Portable discovery |
| [test_tooling_product_graph.py](../../scripts/tests/test_tooling_product_graph.py) | All-module discovery | Portable discovery |
| [test_tooling_watched.py](../../scripts/tests/test_tooling_watched.py) | All-module discovery | Portable discovery |
| [test_validated_runtime_envelopes.py](../../scripts/tests/test_validated_runtime_envelopes.py) | All-module discovery | — |
| [test_workspace_artifact_retention.py](../../scripts/tests/test_workspace_artifact_retention.py) | All-module discovery | — |
| [test_workspace_build_cpus.py](../../scripts/tests/test_workspace_build_cpus.py) | All-module discovery | — |
| [test_workspace_cohorts.py](../../scripts/tests/test_workspace_cohorts.py) | All-module discovery | — |
| [test_workspace_feature_roots.py](../../scripts/tests/test_workspace_feature_roots.py) | All-module discovery | — |
| [test_workspace_partition_queue.py](../../scripts/tests/test_workspace_partition_queue.py) | All-module discovery | — |
| [test_workspace_runner_entry.py](../../scripts/tests/test_workspace_runner_entry.py) | All-module discovery | — |
| [test_workspace_rustc_cache.py](../../scripts/tests/test_workspace_rustc_cache.py) | All-module discovery | — |
| [test_workspace_test_artifacts.py](../../scripts/tests/test_workspace_test_artifacts.py) | All-module discovery | — |
| [test_workspace_test_partitions.py](../../scripts/tests/test_workspace_test_partitions.py) | All-module discovery | — |
| [test_workspace_test_schedule.py](../../scripts/tests/test_workspace_test_schedule.py) | All-module discovery | — |
| [test_workspace_test_storage.py](../../scripts/tests/test_workspace_test_storage.py) | All-module discovery | — |
| [test_benchmark_metrics.py](../../scripts/tests/test_benchmark_metrics.py) | All-module discovery | — |
| [test_generated_output_transaction.py](../../scripts/tests/test_generated_output_transaction.py) | All-module discovery | — |
| [test_intl_generation_publication.py](../../scripts/tests/test_intl_generation_publication.py) | All-module discovery | — |
| [test_maintenance_source_guards.py](../../scripts/tests/test_maintenance_source_guards.py) | All-module discovery | — |
| [test_status_publication.py](../../scripts/tests/test_status_publication.py) | All-module discovery | — |
| [test_test262_chart_evidence.py](../../scripts/tests/test_test262_chart_evidence.py) | All-module discovery | — |
