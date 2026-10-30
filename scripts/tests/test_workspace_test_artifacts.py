"""Cargo identity controls for same-name workspace native test selectors."""

import ast
from scripts.tests.workspace_runner_support import runner_body
import importlib.util
from pathlib import Path
from types import SimpleNamespace
import unittest

SOURCE = Path(__file__).resolve().parents[1] / "workspace_test_artifacts.py"
spec = importlib.util.spec_from_file_location("workspace_test_artifacts", SOURCE)
artifacts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(artifacts)


def message(package, name="differential_corpus", kind="test", *, test=True):
    return {"reason": "compiler-artifact", "package_id": package,
            "target": {"name": name, "kind": [kind]},
            "profile": {"test": test}, "executable": f"/tmp/{package}/{name}"}


def producer(package="runtime-id", name="build-script-build", kind="custom-build"):
    row = message(package, name, kind, test=False)
    row.update(executable=None, filenames=[f"/tmp/{package}/producer"])
    row["profile"]["opt_level"] = "0"
    return row


class WorkspaceTestArtifactTests(unittest.TestCase):
    def test_intl_dev_roots_are_isolated_without_a_caller_flag(self):
        packages = [{'name': 'scriptw-engine'}, {'name': 'scriptw-intl'}, {'name': 'extra'}]
        isolated = artifacts.isolated_test_packages(packages)
        self.assertEqual(artifacts.package_selection(excluded=isolated),
                         ['--workspace', '--exclude', 'scriptw-intl'])
        self.assertEqual(artifacts.isolated_test_packages(packages, ['extra']),
                         ['extra', 'scriptw-intl'])
        with self.assertRaisesRegex(ValueError, 'selected workspace'):
            artifacts.isolated_test_packages(packages, ['missing'])

    def test_other_workspaces_keep_their_explicit_isolation_policy(self):
        packages = [{'name': 'normal'}, {'name': 'separate'}]
        self.assertEqual(artifacts.isolated_test_packages(packages), [])
        self.assertEqual(artifacts.isolated_test_packages(packages, ['separate']), ['separate'])

    @staticmethod
    def case(package, kind, target):
        return {'package': package, 'kind': kind, 'target': target,
                'selector': ['--' + kind] if kind in ('lib', 'doc') else ['--' + kind, target]}

    def test_combined_build_retains_unselected_same_selector_package_siblings(self):
        selected = [self.case('a', 'lib', 'a'), self.case('b', 'test', 'shared')]
        inventory = [*selected, self.case('b', 'lib', 'b'),
                     self.case('a', 'test', 'shared'), self.case('a', 'test', 'other')]
        selectors = artifacts.selected_cohort_selectors(selected)
        self.assertEqual(selectors, [['--lib'], ['--test', 'shared']])
        expected = {(c['package'], c['target'], c['kind']) for c in inventory
                    if artifacts.selector_key(c, (), selectors) == ('selected-native', '*')}
        self.assertEqual(expected, {('a', 'a', 'lib'), ('b', 'b', 'lib'),
                                    ('a', 'shared', 'test'), ('b', 'shared', 'test')})
        found = artifacts.test_artifacts([message(p, t, k) for p, t, k in expected], expected)
        self.assertEqual(set(found), expected)
        self.assertEqual(len(selected), 2)
        self.assertEqual(artifacts.selector_key(inventory[-1], (), selectors), ('test', 'other'))

    def test_combined_build_keeps_isolated_native_and_doc_cohorts_separate(self):
        normal = self.case('a', 'lib', 'a')
        isolated = self.case('intl', 'test', 'shared')
        docs = self.case('a', 'doc', 'doc')
        selected = [normal, isolated, docs]
        self.assertEqual(artifacts.selected_cohort_selectors(selected, ['intl']), [['--lib']])
        self.assertEqual(artifacts.selected_cohort_selectors([isolated, docs], ['intl']), [])
        selectors = [['--lib'], ['--test', 'shared']]
        self.assertEqual(artifacts.selector_key(isolated, ['intl'], selectors), ('test', 'intl::shared'))
        self.assertEqual(artifacts.selector_key(docs, ['intl'], selectors), ('doc', 'doc'))

    def test_retry_members_detect_removed_sibling_with_the_same_cargo_selector(self):
        first = self.case('a', 'test', 'shared')
        second = self.case('b', 'test', 'shared')
        both = [first, second]
        self.assertEqual(artifacts.selected_cohort_selectors(both),
                         artifacts.selected_cohort_selectors([first]))
        self.assertNotEqual(artifacts.selected_target_members(both),
                            artifacts.selected_target_members([first]))
        self.assertEqual(artifacts.selected_target_members(both),
                         artifacts.selected_target_members(list(reversed(both))))

    def test_combined_runner_commands_keep_feature_profile_and_package_boundaries(self):
        source = SOURCE.with_name('run_workspace_tests.py')
        nodes = [node for node in runner_body(source)
                 if isinstance(node, ast.FunctionDef) and node.name in ['group_key', 'cargo_test_base']]
        namespace = {'selector_key': artifacts.selector_key,
                     'package_selection': artifacts.package_selection,
                     'combined_selectors': [['--lib'], ['--test', 'shared']],
                     'isolated_packages': ['intl'], 'profile_flags': ['--release'],
                     'args': SimpleNamespace(all_features=True, packages=None, selected_feature_roots=False)}
        exec(compile(ast.Module(body=nodes, type_ignores=[]), str(source), 'exec'), namespace)
        command = namespace['cargo_test_base']
        self.assertEqual(command(self.case('a', 'lib', 'a')), [
            'cargo', 'test', '--locked', '--offline', '--release', '--workspace',
            '--exclude', 'intl', '--all-features', '--lib', '--test', 'shared'])
        self.assertEqual(command(self.case('intl', 'lib', 'intl')), [
            'cargo', 'test', '--locked', '--offline', '--release', '-p', 'intl', '--all-features', '--lib'])
        self.assertEqual(command(self.case('a', 'doc', 'doc')), [
            'cargo', 'test', '--locked', '--offline', '--release', '-p', 'a', '--all-features', '--doc'])

    def test_custom_build_uses_single_filename_with_null_executable(self):
        rows = [producer(), producer("unrelated-id"), {"reason": "build-finished", "success": True}]
        selected = artifacts.build_script_artifact(rows, "runtime-id", "build-script-build")
        self.assertEqual(selected, Path("/tmp/runtime-id/producer"))
        self.assertEqual(rows[0]["profile"]["opt_level"], "0")

    def test_custom_build_keeps_exact_package_target_and_kind(self):
        for row in [producer("other-id"), producer(name="other-name"), producer(kind="bin")]:
            with self.subTest(row=row), self.assertRaisesRegex(ValueError, "exact"):
                artifacts.build_script_artifact(
                    [row, {"reason": "build-finished", "success": True}],
                    "runtime-id", "build-script-build")

    def test_custom_build_rejects_missing_or_duplicate_cargo_finish(self):
        for finish in [[], [{"reason": "build-finished", "success": False}],
                       [{"reason": "build-finished", "success": True}] * 2]:
            with self.subTest(finish=finish), self.assertRaisesRegex(ValueError, "finish"):
                artifacts.build_script_artifact([producer(), *finish], "runtime-id", "build-script-build")

    def test_custom_build_rejects_duplicate_and_missing_producers(self):
        for rows in [[], [producer(), producer()]]:
            with self.subTest(rows=rows), self.assertRaisesRegex(ValueError, "duplicate"):
                artifacts.build_script_artifact(
                    [*rows, {"reason": "build-finished", "success": True}],
                    "runtime-id", "build-script-build")

    def test_custom_build_rejects_ambiguous_or_invalid_filename_framing(self):
        for files in [[], ["/tmp/a", "/tmp/b"], [None], [""]]:
            row = producer()
            row["filenames"] = files
            with self.subTest(files=files), self.assertRaisesRegex(ValueError, "framing"):
                artifacts.build_script_artifact(
                    [row, {"reason": "build-finished", "success": True}],
                    "runtime-id", "build-script-build")

    def test_custom_build_cannot_admit_a_test_or_executable_claim(self):
        for key, value in [("executable", "/tmp/wrong"), ("profile", {"test": True})]:
            row = producer()
            row[key] = value
            with self.subTest(key=key), self.assertRaisesRegex(ValueError, "framing"):
                artifacts.build_script_artifact(
                    [row, {"reason": "build-finished", "success": True}],
                    "runtime-id", "build-script-build")

    def test_same_name_targets_keep_exact_package_identity(self):
        keys = [(package, "differential_corpus", "test") for package in ["cli-id", "test262-id"]]
        found = artifacts.test_artifacts([message("test262-id"), message("cli-id")], keys)
        self.assertEqual(found[keys[0]], Path("/tmp/cli-id/differential_corpus"))
        self.assertEqual(found[keys[1]], Path("/tmp/test262-id/differential_corpus"))

    def test_missing_sibling_is_not_silently_replaced_by_same_name(self):
        with self.assertRaisesRegex(ValueError, "missing"):
            artifacts.test_artifacts([message("cli-id")], [
                ("cli-id", "differential_corpus", "test"),
                ("test262-id", "differential_corpus", "test")])

    def test_duplicate_and_unexpected_test_images_fail_closed(self):
        key = [("cli-id", "differential_corpus", "test")]
        for rows in [[message("cli-id"), message("cli-id")], [message("other-id")]]:
            with self.subTest(rows=rows), self.assertRaises(ValueError):
                artifacts.test_artifacts(rows, key)

    def test_normal_binary_and_nonexecutables_do_not_enter_test_inventory(self):
        found = artifacts.test_artifacts([
            message("cli-id", test=False), {"reason": "build-finished", "success": True},
            message("test262-id")], [("test262-id", "differential_corpus", "test")])
        self.assertEqual(len(found), 1)

    def test_target_kind_cannot_be_substituted(self):
        with self.assertRaisesRegex(ValueError, "unexpected"):
            artifacts.test_artifacts([message("cli-id", kind="bin")],
                                    [("cli-id", "differential_corpus", "test")])

    def test_native_artifact_directory_is_bound_to_profile_and_target_kind(self):
        root = Path("/tmp/workspace-artifact-profile/release")
        for kind, directory in [("lib", "deps"), ("test", "deps"),
                                ("bin", "deps"), ("example", "examples")]:
            key = ("cli-id", "native", kind)
            row = message(*key)
            row["executable"] = str(root / directory / "native-hash")
            self.assertEqual(artifacts.test_artifacts(
                [row], [key], profile_root=root)[key], Path(row["executable"]))
            for wrong in [root / ("deps" if kind == "example" else "examples"),
                          root.parent / "debug" / directory,
                          root / directory / "nested",
                          root.parent.parent / "other-target" / "release" / directory]:
                row["executable"] = str(wrong / "native-hash")
                with self.subTest(kind=kind, wrong=wrong), self.assertRaisesRegex(
                        AssertionError, "another profile or target directory"):
                    artifacts.test_artifacts([row], [key], profile_root=root)

    def test_package_selection_is_stable_for_every_selector(self):
        self.assertEqual(artifacts.package_selection(), ["--workspace"])
        self.assertEqual(artifacts.package_selection(["scriptw-cli", "scriptw-test262"]),
                         ["-p", "scriptw-cli", "-p", "scriptw-test262"])

    def test_library_cohort_and_duplicate_selectors_share_build_keys(self):
        self.assertEqual(artifacts.selector_key({"kind": "lib", "target": "scriptw_cli"}),
                         artifacts.selector_key({"kind": "lib", "target": "scriptw_test262"}))
        self.assertNotEqual(artifacts.selector_key({"kind": "bin", "target": "scriptw"}),
                            artifacts.selector_key({"kind": "test", "target": "scriptw"}))


    def test_excluded_test_roots_are_absent_from_normal_cargo_selection(self):
        self.assertEqual(artifacts.package_selection(excluded=["scriptw-intl"]),
                         ["--workspace", "--exclude", "scriptw-intl"])
        self.assertEqual(artifacts.package_selection(
            ["scriptw-cli", "scriptw-intl", "scriptw-test262"], excluded=["scriptw-intl"]),
            ["-p", "scriptw-cli", "-p", "scriptw-test262"])

    def test_exclusion_cannot_implicitly_select_the_default_package(self):
        with self.assertRaisesRegex(ValueError, "nonempty"):
            artifacts.package_selection(["scriptw-intl"], excluded=["scriptw-intl"])

    def test_isolated_library_and_named_targets_cannot_widen_normal_cohorts(self):
        for kind in ["lib", "test"]:
            normal = {"package": "scriptw-cli", "kind": kind, "target": "native_contract"}
            isolated = {"package": "scriptw-intl", "kind": kind, "target": "native_contract"}
            sibling = {"package": "scriptw-test262", "kind": kind, "target": "native_contract"}
            self.assertNotEqual(artifacts.selector_key(normal, ["scriptw-intl"]),
                                artifacts.selector_key(isolated, ["scriptw-intl"]))
            self.assertEqual(artifacts.selector_key(normal, ["scriptw-intl"]),
                             artifacts.selector_key(sibling, ["scriptw-intl"]))

    def test_distinct_isolated_packages_keep_distinct_cargo_cohorts(self):
        cases = [{"package": p, "kind": "test", "target": "same"}
                 for p in ["scriptw-intl", "scriptw-cli"]]
        self.assertNotEqual(*[artifacts.selector_key(c, ["scriptw-intl", "scriptw-cli"])
                              for c in cases])

    def test_docs_default_to_one_complete_workspace_cohort(self):
        docs = [{"package": p, "kind": "doc"} for p in ["scriptw-cli", "scriptw-intl"]]
        self.assertEqual(artifacts.doc_cohorts(docs),
                         [("doc:*", ["--workspace"], {"scriptw-cli", "scriptw-intl"})])

    def test_docs_keep_intl_test_data_out_of_normal_roots(self):
        docs = [{"package": p, "kind": "doc"} for p in ["scriptw-cli", "scriptw-intl"]]
        self.assertEqual(artifacts.doc_cohorts(docs, isolated_packages=["scriptw-intl"]), [
            ("doc:*", ["--workspace", "--exclude", "scriptw-intl"], {"scriptw-cli"}),
            ("doc:scriptw-intl::*", ["-p", "scriptw-intl"], {"scriptw-intl"})])

    def test_docs_subset_and_all_isolated_selection_remain_exact(self):
        docs = [{"package": p, "kind": "doc"} for p in ["scriptw-intl", "scriptw-cli"]]
        self.assertEqual(artifacts.doc_cohorts(
            docs, ["scriptw-intl", "scriptw-cli"], ["scriptw-intl", "scriptw-cli"]), [
            ("doc:scriptw-cli::*", ["-p", "scriptw-cli"], {"scriptw-cli"}),
            ("doc:scriptw-intl::*", ["-p", "scriptw-intl"], {"scriptw-intl"})])
        self.assertEqual(artifacts.doc_cohorts(docs, ["scriptw-intl", "scriptw-cli"], ["scriptw-intl"])[0],
                         ("doc:*", ["-p", "scriptw-cli"], {"scriptw-cli"}))

    def test_docs_without_doctests_remain_in_cargo_selection(self):
        inventory = [{"package": "normal", "kind": "doc"},
                     {"package": "no-doc", "kind": "lib"},
                     {"package": "isolated-no-doc", "kind": "lib"}]
        self.assertEqual(artifacts.doc_cohorts(
            inventory, ["normal", "no-doc", "isolated-no-doc"], ["isolated-no-doc"]),
            [("doc:*", ["-p", "normal", "-p", "no-doc"], {"normal"})])

    def test_docs_empty_inventory_never_selects_implicit_default_root(self):
        self.assertEqual(artifacts.doc_cohorts([], ["scriptw-intl"], ["scriptw-intl"]), [])

    def test_docs_duplicate_package_inventory_fails_closed(self):
        docs = [{"package": "scriptw-cli", "kind": "doc"}] * 2
        with self.assertRaisesRegex(ValueError, "duplicate"):
            artifacts.doc_cohorts(docs)

    def test_doc_cohort_union_covers_every_package_exactly_once(self):
        names = {"a", "b", "c", "d"}
        inventory = [{"package": p, "kind": "doc"} for p in sorted(names)]
        for isolated in [[], ["b"], ["a", "c"], sorted(names)]:
            groups = artifacts.doc_cohorts(inventory, isolated_packages=isolated)
            covered = [p for _, _, docs in groups for p in docs]
            self.assertEqual(set(covered), names)
            self.assertEqual(len(covered), len(names))

if __name__ == "__main__":
    unittest.main()
