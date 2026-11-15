"""Resource launcher controls: absent or ineffective caps never start payloads."""

from contextlib import ExitStack
import ast
import importlib.util
import io
from pathlib import Path
import unittest
from unittest.mock import patch


import sys
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

SOURCE = Path(__file__).resolve().parents[1] / "limited_verification.py"
spec = importlib.util.spec_from_file_location("limited_verification", SOURCE)
launcher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(launcher)


class PayloadWouldStart(Exception):
    pass


class KernelBudgetTests(unittest.TestCase):
    def scope(self, memory="4294967296", swap="0", group="1"):
        files = {"/proc/self/cgroup": "0::/test.scope\n",
                 "/sys/fs/cgroup/test.scope/memory.max": memory,
                 "/sys/fs/cgroup/test.scope/memory.swap.max": swap,
                 "/sys/fs/cgroup/test.scope/memory.oom.group": group}
        stack = ExitStack()
        stack.enter_context(patch.object(launcher.sys, "platform", "linux"))
        stack.enter_context(patch.object(launcher.Path, "read_text",
                                        lambda path: files[str(path)]))
        affinity = [{2, 4, 6}]
        stack.enter_context(patch.object(launcher.os, "sched_getaffinity",
                                        lambda pid: affinity[0], create=True))
        stack.enter_context(patch.object(launcher.os, "sched_setaffinity",
                                        lambda pid, cpus: affinity.__setitem__(0, cpus), create=True))
        return stack

    def test_ineffective_budget_never_executes_payload(self):
        for settings in ({"memory": "max"}, {"memory": "4294967297"},
                         {"memory": "0"}, {"swap": "1"}, {"group": "0"}):
            with self.subTest(settings=settings), self.scope(**settings), \
                    patch.object(launcher.os, "execvpe") as execute:
                self.assertEqual(launcher.main(["--in-scope", "--", "payload"]), 2)
                execute.assert_not_called()

    def test_confirmed_budget_preserves_argv_and_serializes_descendants(self):
        argv = ["payload", "two words", "$HOME", "*"]
        with self.scope(), patch.object(launcher.os, "execvpe",
                                        side_effect=PayloadWouldStart) as execute:
            with self.assertRaises(PayloadWouldStart):
                launcher.main(["--in-scope", "--", *argv])
            command, arguments, environment = execute.call_args.args
            self.assertEqual(command, "payload")
            self.assertEqual(arguments, argv)
            self.assertEqual(launcher.os.sched_getaffinity(0), {2})
            for setting in ("CARGO_BUILD_JOBS", "SCRIPTW_JOBS", "RUST_TEST_THREADS", "RAYON_NUM_THREADS"):
                self.assertEqual(environment[setting], "1")
            self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES"], "1")
            self.assertLessEqual(int(environment["SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES"]), 64 * 1024 * 1024)

    def test_large_inherited_caches_are_capped_and_stricter_image_limits_survive(self):
        for inherited, expected in (("536870912", "67108864"), ("1048576", "1048576")):
            with self.subTest(inherited=inherited), self.scope(), \
                    patch.dict(launcher.os.environ, {
                        "SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES": "64",
                        "SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES": inherited,
                    }), patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
                with self.assertRaises(PayloadWouldStart):
                    launcher.main(["--in-scope", "--", "payload"])
                environment = execute.call_args.args[2]
                self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES"], "1")
                self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES"], expected)

    def test_invalid_cache_limits_never_reach_engine_fallback_defaults(self):
        for setting in ("SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES", "SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES"):
            for invalid in ("0", "-1", "unlimited", "1.5", "", "9" * 21):
                with self.subTest(setting=setting, invalid=invalid), \
                        patch.dict(launcher.os.environ, {setting: invalid}), \
                        patch.object(launcher.shutil, "which") as manager, \
                        patch.object(launcher.os, "execvpe") as execute:
                    self.assertEqual(launcher.main(["--", "payload"]), 2)
                    manager.assert_not_called()
                    execute.assert_not_called()

    def test_missing_scope_manager_has_no_uncapped_fallback(self):
        with patch.object(launcher.sys, "platform", "linux"), \
                patch.object(launcher.shutil, "which", return_value=None), \
                patch.object(launcher.os, "execvpe") as execute:
            self.assertEqual(launcher.main(["--", "payload"]), 2)
            execute.assert_not_called()

    def test_budget_override_above_four_gib_never_launches_a_scope(self):
        for amount in ("4097", "65536"):
            with self.subTest(amount=amount), \
                    patch.object(launcher.shutil, "which") as manager, \
                    patch.object(launcher.os, "execvpe") as execute:
                with self.assertRaises(SystemExit) as failure:
                    launcher.main(["--memory-mib", amount, "--", "payload"])
                self.assertEqual(failure.exception.code, 2)
                manager.assert_not_called()
                execute.assert_not_called()

    def test_scope_launch_requires_aggregate_kernel_properties(self):
        with patch.object(launcher.sys, "platform", "linux"), \
                patch.object(launcher.shutil, "which", return_value="/usr/bin/systemd-run"), \
                patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
            with self.assertRaises(PayloadWouldStart):
                launcher.main(["--", "payload", "two words", "$HOME", "*"])
            command, argv, _ = execute.call_args.args
            self.assertEqual(command, "/usr/bin/systemd-run")
            for setting in ("--scope", "--expand-environment=no", "--property=MemoryMax=4294967296",
                            "--property=MemorySwapMax=0", "--property=OOMPolicy=kill"):
                self.assertIn(setting, argv)
            self.assertEqual(argv[-5:], ["--", "payload", "two words", "$HOME", "*"])


class CloudBudgetTests(unittest.TestCase):
    def setUp(self):
        # These tests supply their own inherited environment. The real outer
        # limiter remains active, including its kernel memory/CPU bounds.
        environment = patch.dict(launcher.os.environ, {}, clear=True)
        environment.start()
        self.addCleanup(environment.stop)

    def test_host_root_without_controller_files_keeps_the_inherited_scope_limits(self):
        files = {
            "/proc/self/cgroup": "0::/scope\n",
            "/sys/fs/cgroup/scope/memory.max": "4294967296",
            "/sys/fs/cgroup/scope/cpu.max": "100000 100000",
        }
        def read(path):
            try:
                return files[str(path)]
            except KeyError:
                raise FileNotFoundError(str(path)) from None
        affinity = [{0, 1, 2}]
        with patch.object(launcher.Path, "read_text", read), \
                patch.object(launcher.os, "sched_getaffinity", lambda pid: affinity[0]), \
                patch.object(launcher.os, "sched_setaffinity", lambda pid, cpus: affinity.__setitem__(0, cpus)):
            self.assertEqual(launcher.require_cloud_budget(), 4294967296)
            self.assertEqual(affinity[0], {0})
            self.assertEqual(launcher.cloud_native_affinity(), 1)
            files["/sys/fs/cgroup/scope/memory.max"] = "max"
            with self.assertRaisesRegex(ValueError, "finite inherited"):
                launcher.require_cloud_budget()
            del files["/sys/fs/cgroup/scope/memory.max"]
            with self.assertRaises(FileNotFoundError):
                launcher.require_cloud_budget()

    def cloud(self, membership="/", limits=None, cpu_limits=None, allowed_cpus=None):
        files = {"/proc/self/cgroup": f"0::{membership}\n"}
        for group, limit in (limits or {"/": "34359738368"}).items():
            files[str(Path("/sys/fs/cgroup") / group.lstrip("/") / "memory.max")] = limit
        for group, limit in (cpu_limits or {}).items():
            files[str(Path("/sys/fs/cgroup") / group.lstrip("/") / "cpu.max")] = limit
        stack = ExitStack()
        stack.enter_context(patch.object(launcher.sys, "platform", "linux"))
        stack.enter_context(patch.object(launcher.Path, "read_text",
                                        lambda path: files[str(path)]))
        affinity = [{2, 4, 6} if allowed_cpus is None else set(allowed_cpus)]
        stack.enter_context(patch.object(launcher.os, "sched_getaffinity",
                                        lambda pid: affinity[0], create=True))
        stack.enter_context(patch.object(launcher.os, "sched_setaffinity",
                                        lambda pid, cpus: affinity.__setitem__(0, cpus), create=True))
        return stack

    def hybrid(self, memory, cpu=None, allowed_cpus=(0, 1, 2, 3)):
        """A cgroup-v1 memory and CPU hierarchy beside an empty unified root."""
        files = {"/proc/self/cgroup":
                 "4:memory:/api/task\n1:cpu,cpuacct:/api\n0::/\n"}
        for group, limit in memory.items():
            files[str(Path("/sys/fs/cgroup/memory") / group.lstrip("/")
                      / "memory.limit_in_bytes")] = limit
        for group, (quota, period) in (cpu or {}).items():
            base = Path("/sys/fs/cgroup/cpu") / group.lstrip("/")
            files[str(base / "cpu.cfs_quota_us")] = quota
            files[str(base / "cpu.cfs_period_us")] = period
        def read(path):
            try:
                return files[str(path)]
            except KeyError:
                raise FileNotFoundError(str(path)) from None
        stack = ExitStack()
        stack.enter_context(patch.object(launcher.sys, "platform", "linux"))
        stack.enter_context(patch.object(launcher.Path, "read_text", read))
        stack.enter_context(patch.object(launcher.os, "sysconf", lambda name: 4096))
        affinity = [set(allowed_cpus)]
        stack.enter_context(patch.object(launcher.os, "sched_getaffinity",
                                        lambda pid: affinity[0], create=True))
        stack.enter_context(patch.object(launcher.os, "sched_setaffinity",
                                        lambda pid, cpus: affinity.__setitem__(0, cpus), create=True))
        return stack

    def test_hybrid_machine_uses_the_tightest_legacy_memory_limit(self):
        unlimited = "9223372036854771712"
        with self.hybrid({"/api/task": "14338772992", "/api": unlimited, "/": unlimited}):
            self.assertEqual(launcher.require_cloud_budget(), 14338772992)
        with self.hybrid({"/api/task": unlimited, "/api": "1073741824", "/": unlimited}):
            self.assertEqual(launcher.require_cloud_budget(), 1073741824)
        # The controller root alone may lack its limit file.
        with self.hybrid({"/api/task": "14338772992", "/api": unlimited}):
            self.assertEqual(launcher.require_cloud_budget(), 14338772992)

    def test_hybrid_machine_without_a_finite_legacy_limit_never_executes(self):
        unlimited = "9223372036854771712"
        for memory in (
            {"/api/task": unlimited, "/api": unlimited, "/": unlimited},
            {"/api/task": "0", "/api": unlimited, "/": unlimited},
            {"/api/task": "garbage", "/api": unlimited, "/": unlimited},
            {"/api": "1073741824", "/": unlimited},
        ):
            with self.subTest(memory=memory), self.hybrid(memory), \
                    patch.object(launcher.os, "execvpe") as execute:
                self.assertEqual(launcher.main(["--cloud", "--", "payload"]), 2)
                execute.assert_not_called()

    def test_hybrid_machine_respects_the_legacy_cpu_quota(self):
        memory = {"/api/task": "14338772992", "/api": "9223372036854771712"}
        with self.hybrid(memory, {"/api": ("-1", "100000"), "/": ("-1", "100000")}):
            self.assertEqual(launcher.cloud_native_affinity(), 4)
        with self.hybrid(memory, {"/api": ("250000", "100000"), "/": ("-1", "100000")}):
            self.assertEqual(launcher.cloud_native_affinity(), 2)
        with self.hybrid(memory, {"/api": ("-1", "100000")}):
            self.assertEqual(launcher.cloud_native_affinity(), 4)
        for cpu in ({"/api": ("0", "100000")}, {"/api": ("-2", "100000")},
                    {"/api": ("100000", "0")}, {"/": ("-1", "100000")}):
            with self.subTest(cpu=cpu), self.hybrid(memory, cpu), \
                    patch.object(launcher.os, "execvpe") as execute:
                self.assertEqual(
                    launcher.main(["--cloud", "--cloud-cpus", "auto", "--", "payload"]), 2)
                execute.assert_not_called()

    def test_cloud_uses_finite_machine_cap_without_scope_manager(self):
        argv = ["payload", "two words", "$HOME", "*"]
        with self.cloud(), patch.object(launcher.shutil, "which") as manager, \
                patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
            with self.assertRaises(PayloadWouldStart):
                launcher.main(["--cloud", "--", *argv])
            manager.assert_not_called()
            command, arguments, environment = execute.call_args.args
            self.assertEqual(command, "payload")
            self.assertEqual(arguments, argv)
            self.assertEqual(launcher.os.sched_getaffinity(0), {2})
            for name in ("CARGO_BUILD_JOBS", "SCRIPTW_JOBS", "RUST_TEST_THREADS", "RAYON_NUM_THREADS"):
                self.assertEqual(environment[name], "1")
            self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES"], "512")
            self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES"], "2147483648")

    def test_cloud_cache_ceilings_preserve_stricter_explicit_limits(self):
        for entries, image_bytes, expected in (
            ("64", "536870912", ("64", "536870912")),
            ("1", "1048576", ("1", "1048576")),
        ):
            with self.subTest(entries=entries, image_bytes=image_bytes), self.cloud(), \
                    patch.dict(launcher.os.environ, {
                        "SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES": entries,
                        "SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES": image_bytes,
                    }), patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
                with self.assertRaises(PayloadWouldStart):
                    launcher.main(["--cloud", "--", "payload"])
                environment = execute.call_args.args[2]
                self.assertEqual((environment["SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES"],
                                  environment["SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES"]), expected)

    def test_cloud_cache_image_ceiling_tracks_the_tightest_machine_budget(self):
        with self.cloud("/machine/task", {
            "/machine/task": "max", "/machine": "536870912", "/": "34359738368",
        }), patch.dict(launcher.os.environ, {
            "SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES": "64",
            "SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES": "536870912",
        }), patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
            with self.assertRaises(PayloadWouldStart):
                launcher.main(["--cloud", "--", "payload"])
            environment = execute.call_args.args[2]
            self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES"], "8")
            self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES"], "67108864")

    def test_cloud_cache_ceilings_scale_with_the_machine_budget(self):
        for budget, expected in (
            (512 * launcher.MIB, (8, 64 * launcher.MIB)),
            (4 * 1024 * launcher.MIB, (64, 512 * launcher.MIB)),
            (8 * 1024 * launcher.MIB, (128, 1024 * launcher.MIB)),
            (32 * 1024 * launcher.MIB, (512, 2 * 1024 * launcher.MIB)),
        ):
            with self.subTest(budget=budget):
                self.assertEqual(launcher.cloud_module_cache_limits(budget), expected)

    def test_cloud_uses_tightest_visible_ancestor(self):
        with self.cloud("/machine/task", {
            "/machine/task": "max", "/machine": "8589934592", "/": "34359738368",
        }):
            self.assertEqual(launcher.require_cloud_budget(), 8589934592)
        with self.cloud("/machine/task", {
            "/machine/task": "1073741824", "/machine": "8589934592", "/": "34359738368",
        }):
            self.assertEqual(launcher.require_cloud_budget(), 1073741824)

    def test_invalid_or_unbounded_cloud_caps_never_execute(self):
        for limit in ("max", "0", "-1", "garbage", "1.5"):
            with self.subTest(limit=limit), self.cloud(limits={"/": limit}), \
                    patch.object(launcher.os, "execvpe") as execute:
                self.assertEqual(launcher.main(["--cloud", "--", "payload"]), 2)
                execute.assert_not_called()

    def test_invalid_cloud_membership_never_executes(self):
        for membership in ("/../escape", "relative"):
            with self.subTest(membership=membership), self.cloud(membership), \
                    patch.object(launcher.os, "execvpe") as execute:
                self.assertEqual(launcher.main(["--cloud", "--", "payload"]), 2)
                execute.assert_not_called()

    def test_unreadable_cloud_controller_never_executes(self):
        with patch.object(launcher.sys, "platform", "linux"), \
                patch.object(launcher.Path, "read_text", side_effect=PermissionError), \
                patch.object(launcher.os, "execvpe") as execute:
            self.assertEqual(launcher.main(["--cloud", "--", "payload"]), 2)
            execute.assert_not_called()

    def test_cloud_cannot_be_combined_with_local_scope_options(self):
        for options in (["--in-scope"], ["--memory-mib", "4096"]):
            with self.subTest(options=options), patch.object(launcher.os, "execvpe") as execute:
                with self.assertRaises(SystemExit) as failure:
                    launcher.main(["--cloud", *options, "--", "payload"])
                self.assertEqual(failure.exception.code, 2)
                execute.assert_not_called()


class CloudCpuBudgetTests(unittest.TestCase):
    cloud = CloudBudgetTests.cloud

    def setUp(self):
        environment = patch.dict(launcher.os.environ, {}, clear=True)
        environment.start()
        self.addCleanup(environment.stop)

    def test_native_parallelism_uses_quota_while_tests_and_cargo_stay_serial(self):
        argv = ["payload", "two words", "$HOME", "*"]
        with self.cloud(cpu_limits={"/": "400000 100000"}, allowed_cpus={0, 1, 2, 3, 4}), \
                patch.object(launcher.shutil, "which") as manager, \
                patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
            with self.assertRaises(PayloadWouldStart):
                launcher.main(["--cloud", "--cloud-cpus", "auto", "--", *argv])
            manager.assert_not_called()
            self.assertEqual(execute.call_args.args[:2], ("payload", argv))
            self.assertEqual(launcher.os.sched_getaffinity(0), {0, 1, 2, 3})
            environment = execute.call_args.args[2]
            for name in ("CARGO_BUILD_JOBS", "RUST_TEST_THREADS"):
                self.assertEqual(environment[name], "1")
            for name in ("SCRIPTW_JOBS", "RAYON_NUM_THREADS"):
                self.assertEqual(environment[name], "4")
            self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES"], "512")
            self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES"], "2147483648")

    def test_native_parallelism_avoids_nested_default_halving_and_preserves_explicit_share(self):
        for supplied, expected in (({}, "100"), ({"SCRIPTW_CPU_PERCENT": "37"}, "37")):
            with self.subTest(supplied=supplied), \
                    self.cloud(cpu_limits={"/": "400000 100000"}, allowed_cpus={0, 1, 2, 3, 4}), \
                    patch.dict(launcher.os.environ, supplied), \
                    patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
                with self.assertRaises(PayloadWouldStart):
                    launcher.main(["--cloud", "--cloud-cpus", "auto", "--", "payload"])
                self.assertEqual(execute.call_args.args[2]["SCRIPTW_CPU_PERCENT"], expected)

    def test_native_parallelism_respects_tightest_ancestor_quota(self):
        with self.cloud("/machine/task", {
            "/machine/task": "max", "/machine": "8589934592", "/": "34359738368",
        }, cpu_limits={
            "/machine/task": "800000 100000", "/machine": "100000 50000", "/": "400000 100000",
        }, allowed_cpus={0, 1, 2, 3, 4}):
            self.assertEqual(launcher.require_cloud_budget(serial=False), 8589934592)
            self.assertEqual(launcher.cloud_native_affinity(), 2)
            self.assertEqual(launcher.os.sched_getaffinity(0), {0, 1})

    def test_unbounded_cpu_quota_uses_only_inherited_affinity(self):
        with self.cloud(cpu_limits={"/": "max 100000"}, allowed_cpus={2, 4, 6}):
            self.assertEqual(launcher.cloud_native_affinity(), 3)
            self.assertEqual(launcher.os.sched_getaffinity(0), {2, 4, 6})

    def test_fractional_cpu_quota_keeps_at_least_one_worker(self):
        for quota in ("50000 100000", "150000 100000"):
            with self.subTest(quota=quota), self.cloud(cpu_limits={"/": quota}):
                self.assertEqual(launcher.cloud_native_affinity(), 1)
                self.assertEqual(launcher.os.sched_getaffinity(0), {2})

    def test_native_parallelism_cannot_expand_inherited_affinity(self):
        for allowed in ({4}, {2, 6}):
            with self.subTest(allowed=allowed), \
                    self.cloud(cpu_limits={"/": "400000 100000"}, allowed_cpus=allowed):
                self.assertEqual(launcher.cloud_native_affinity(), len(allowed))
                self.assertEqual(launcher.os.sched_getaffinity(0), allowed)

    def test_native_parallelism_preserves_stricter_inherited_worker_limits(self):
        for supplied in ({"SCRIPTW_JOBS": "1"}, {"RAYON_NUM_THREADS": "2"},
                         {"SCRIPTW_JOBS": "16", "RAYON_NUM_THREADS": "16"}):
            expected = min(map(int, supplied.values()), default=4)
            expected = min(expected, 4)
            with self.subTest(supplied=supplied), \
                    self.cloud(cpu_limits={"/": "400000 100000"}, allowed_cpus={0, 1, 2, 3, 4}), \
                    patch.dict(launcher.os.environ, supplied), \
                    patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
                with self.assertRaises(PayloadWouldStart):
                    launcher.main(["--cloud", "--cloud-cpus", "auto", "--", "payload"])
                self.assertEqual(len(launcher.os.sched_getaffinity(0)), expected)
                environment = execute.call_args.args[2]
                self.assertEqual(environment["SCRIPTW_JOBS"], str(expected))
                self.assertEqual(environment["RAYON_NUM_THREADS"], str(expected))

    def test_invalid_cpu_controller_never_executes(self):
        for quota in ("", "max", "0 100000", "-1 100000", "1.5 100000", "max 0",
                      "max invalid", "400000 100000 extra", "１２ 100000"):
            with self.subTest(quota=quota), self.cloud(cpu_limits={"/": quota}), \
                    patch.object(launcher.os, "execvpe") as execute:
                self.assertEqual(launcher.main(["--cloud", "--cloud-cpus", "auto", "--", "payload"]), 2)
                execute.assert_not_called()

    def test_unreadable_cpu_controller_never_executes(self):
        with self.cloud(cpu_limits={"/": "400000 100000"}), \
                patch.object(launcher.os, "execvpe") as execute:
            original_read = launcher.Path.read_text
            def read(path):
                if path.name == "cpu.max":
                    raise PermissionError("unreadable CPU controller")
                return original_read(path)
            with patch.object(launcher.Path, "read_text", read):
                self.assertEqual(launcher.main(["--cloud", "--cloud-cpus", "auto", "--", "payload"]), 2)
                execute.assert_not_called()

    def test_empty_affinity_never_executes(self):
        with self.cloud(cpu_limits={"/": "400000 100000"}, allowed_cpus=set()), \
                patch.object(launcher.os, "execvpe") as execute:
            self.assertEqual(launcher.main(["--cloud", "--cloud-cpus", "auto", "--", "payload"]), 2)
            execute.assert_not_called()

    def test_ineffective_affinity_never_executes(self):
        with self.cloud(cpu_limits={"/": "100000 100000"}), \
                patch.object(launcher.os, "sched_setaffinity"), \
                patch.object(launcher.os, "execvpe") as execute:
            self.assertEqual(launcher.main(["--cloud", "--cloud-cpus", "auto", "--", "payload"]), 2)
            execute.assert_not_called()

    def test_invalid_inherited_native_limits_never_executes(self):
        for name in ("SCRIPTW_JOBS", "RAYON_NUM_THREADS"):
            for invalid in ("0", "-1", "unlimited", "1.5", "", "9" * 21):
                with self.subTest(name=name, invalid=invalid), \
                        self.cloud(cpu_limits={"/": "400000 100000"}), \
                        patch.dict(launcher.os.environ, {name: invalid}), \
                        patch.object(launcher.os, "execvpe") as execute:
                    self.assertEqual(launcher.main(["--cloud", "--cloud-cpus", "auto", "--", "payload"]), 2)
                    execute.assert_not_called()

    def test_native_cpu_option_requires_cloud_mode(self):
        with patch.object(launcher.os, "execvpe") as execute:
            with self.assertRaises(SystemExit) as failure:
                launcher.main(["--cloud-cpus", "auto", "--", "payload"])
            self.assertEqual(failure.exception.code, 2)
            execute.assert_not_called()


class CloudWorkerCpuBudgetTests(unittest.TestCase):
    cloud = CloudBudgetTests.cloud

    def setUp(self):
        environment = patch.dict(launcher.os.environ, {}, clear=True)
        environment.start()
        self.addCleanup(environment.stop)

    def test_worker_affinity_is_independent_of_serial_compilation_and_preserves_payload(self):
        argv = ["payload", "--threads", "3", "--jobs", "1", "two words", "$HOME", "*"]
        supplied = {"SCRIPTW_JOBS": "1", "RAYON_NUM_THREADS": "1",
                    "SCRIPTW_ENABLE_SPEC_EXEC": "0", "FEATURE_SENTINEL": "unchanged"}
        for share, expected_share in (({}, "100"), ({"SCRIPTW_CPU_PERCENT": "37"}, "37")):
            with self.subTest(share=share), \
                    self.cloud(cpu_limits={"/": "400000 100000"}, allowed_cpus={0, 1, 2}), \
                    patch.dict(launcher.os.environ, {**supplied, **share}), \
                    patch.object(launcher.shutil, "which") as manager, \
                    patch.object(launcher.sys, "stderr", new_callable=io.StringIO) as output, \
                    patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
                before = dict(launcher.os.environ)
                with self.assertRaises(PayloadWouldStart):
                    launcher.main(["--cloud", "--cloud-worker-cpus", "auto", "--", *argv])
                manager.assert_not_called()
                self.assertEqual(execute.call_args.args[:2], ("payload", argv))
                self.assertEqual(launcher.os.sched_getaffinity(0), {0, 1, 2})
                environment = execute.call_args.args[2]
                for name in ("CARGO_BUILD_JOBS", "RUST_TEST_THREADS", "SCRIPTW_JOBS", "RAYON_NUM_THREADS"):
                    self.assertEqual(environment[name], "1")
                for name, value in supplied.items():
                    self.assertEqual(environment[name], value)
                self.assertEqual(environment["SCRIPTW_CPU_PERCENT"], expected_share)
                self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_ENTRIES"], "512")
                self.assertEqual(environment["SCRIPTW_MODULE_MEMORY_CACHE_LIMIT_BYTES"], "2147483648")
                self.assertEqual(dict(launcher.os.environ), before)
                self.assertIn("3 CPU(s)", output.getvalue())
                self.assertIn("native compilation defaults up to 1 jobs", output.getvalue())

    def test_workspace_partition_launch_preserves_each_assigned_cpu(self):
        runner = SOURCE.with_name("run_workspace_tests.py")
        tree = ast.parse(runner.read_text())
        branches = [node for node in ast.walk(tree) if isinstance(node, ast.If)
                    and isinstance(node.test, ast.Name) and node.test.id == "partitioned"]
        launches = [node for branch in branches for node in branch.body
                    if isinstance(node, ast.Assign)
                    and any(isinstance(target, ast.Name) and target.id == "launch"
                            for target in node.targets)]
        self.assertEqual(len(launches), 1)
        actual = compile(ast.Module(body=launches, type_ignores=[]), str(runner), "exec")
        for assigned in ({0}, {0, 1, 2, 3}, {2, 4, 6}):
            for qualified in ("scriptw-engine:scriptw_engine", "scriptw-engine:aot_async",
                              "scriptw-engine:aot_intl", "scriptw-cli:cli"):
                package = qualified.split(":", 1)[0]
                payload = ["python3", "-B", "scripts/workspace_test_partitions.py",
                           "--cpus", *map(str, sorted(assigned)),
                           "--qualified-target", qualified, "--include-ignored"]
                scope = {"watch_label": "partition-proof", "case": {"package": package},
                         "command": payload,
                         "native_stall_seconds": lambda name: 3600 if name == "scriptw-cli" else 900}
                exec(actual, scope)
                invocation = scope["launch"]
                options = invocation[invocation.index("scripts/limited_verification.py") + 1:]
                with self.subTest(assigned=assigned, target=qualified), \
                        self.cloud(cpu_limits={"/": "400000 100000"}, allowed_cpus=assigned), \
                        patch.dict(launcher.os.environ, {"SCRIPTW_JOBS": "1", "RAYON_NUM_THREADS": "1"}), \
                        patch.object(launcher.sys, "stderr", new_callable=io.StringIO), \
                        patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
                    with self.assertRaises(PayloadWouldStart):
                        launcher.main(options)
                    self.assertEqual(launcher.os.sched_getaffinity(0), assigned)
                    self.assertEqual(execute.call_args.args[:2], (payload[0], payload))
                    for setting in ("CARGO_BUILD_JOBS", "RUST_TEST_THREADS", "SCRIPTW_JOBS", "RAYON_NUM_THREADS"):
                        self.assertEqual(execute.call_args.args[2][setting], "1")

    def test_worker_affinity_uses_tightest_quota_without_native_job_caps(self):
        with self.cloud("/machine/task", {
            "/machine/task": "max", "/machine": "8589934592", "/": "34359738368",
        }, cpu_limits={
            "/machine/task": "800000 100000", "/machine": "250000 100000", "/": "400000 100000",
        }, allowed_cpus={0, 1, 2, 3, 4}), \
                patch.dict(launcher.os.environ, {"SCRIPTW_JOBS": "1", "RAYON_NUM_THREADS": "1"}):
            self.assertEqual(launcher.require_cloud_budget(serial=False), 8589934592)
            self.assertEqual(launcher.cloud_worker_affinity(), 2)
            self.assertEqual(launcher.os.sched_getaffinity(0), {0, 1})

    def test_worker_affinity_never_expands_inherited_or_fractional_capacity(self):
        for quota, allowed, expected in (
            ("max 100000", {2, 4, 6}, {2, 4, 6}),
            ("400000 100000", {4}, {4}),
            ("400000 100000", {2, 6}, {2, 6}),
            ("50000 100000", {2, 4, 6}, {2}),
            ("150000 100000", {2, 4, 6}, {2}),
        ):
            with self.subTest(quota=quota, allowed=allowed), \
                    self.cloud(cpu_limits={"/": quota}, allowed_cpus=allowed):
                self.assertEqual(launcher.cloud_worker_affinity(), len(expected))
                self.assertEqual(launcher.os.sched_getaffinity(0), expected)

    def test_worker_mode_keeps_native_defaults_serial_without_explicit_limits(self):
        for supplied in ({}, {"SCRIPTW_JOBS": "16", "RAYON_NUM_THREADS": "8"}):
            with self.subTest(supplied=supplied), \
                    self.cloud(cpu_limits={"/": "400000 100000"}, allowed_cpus={0, 1, 2, 3, 4}), \
                    patch.dict(launcher.os.environ, supplied), \
                    patch.object(launcher.os, "execvpe", side_effect=PayloadWouldStart) as execute:
                with self.assertRaises(PayloadWouldStart):
                    launcher.main(["--cloud", "--cloud-worker-cpus", "auto", "--", "payload"])
                self.assertEqual(launcher.os.sched_getaffinity(0), {0, 1, 2, 3})
                for name in ("CARGO_BUILD_JOBS", "RUST_TEST_THREADS", "SCRIPTW_JOBS", "RAYON_NUM_THREADS"):
                    self.assertEqual(execute.call_args.args[2][name], "1")

    def test_invalid_worker_mode_selection_never_executes(self):
        for options in (
            ["--cloud-worker-cpus", "auto"],
            ["--cloud", "--cloud-worker-cpus", "3"],
            ["--cloud", "--cloud-cpus", "auto", "--cloud-worker-cpus", "auto"],
            ["--cloud", "--cloud-worker-cpus", "auto", "--memory-mib", "4096"],
        ):
            with self.subTest(options=options), patch.object(launcher.os, "execvpe") as execute:
                with self.assertRaises(SystemExit) as failure:
                    launcher.main([*options, "--", "payload"])
                self.assertEqual(failure.exception.code, 2)
                execute.assert_not_called()

    def test_invalid_worker_cpu_controller_and_affinity_never_execute(self):
        for quota, allowed in (("max 0", {2, 4, 6}), ("bad 100000", {2, 4, 6}),
                               ("400000 100000", set())):
            with self.subTest(quota=quota, allowed=allowed), \
                    self.cloud(cpu_limits={"/": quota}, allowed_cpus=allowed), \
                    patch.object(launcher.os, "execvpe") as execute:
                self.assertEqual(launcher.main(["--cloud", "--cloud-worker-cpus", "auto", "--", "payload"]), 2)
                execute.assert_not_called()
        with self.cloud(cpu_limits={"/": "100000 100000"}), \
                patch.object(launcher.os, "sched_setaffinity"), \
                patch.object(launcher.os, "execvpe") as execute:
            self.assertEqual(launcher.main(["--cloud", "--cloud-worker-cpus", "auto", "--", "payload"]), 2)
            execute.assert_not_called()

    def test_worker_mode_rejects_invalid_native_job_limits(self):
        for name in ("SCRIPTW_JOBS", "RAYON_NUM_THREADS"):
            with self.subTest(name=name), self.cloud(cpu_limits={"/": "400000 100000"}), \
                    patch.dict(launcher.os.environ, {name: "invalid"}), \
                    patch.object(launcher.os, "execvpe") as execute:
                self.assertEqual(launcher.main(["--cloud", "--cloud-worker-cpus", "auto", "--", "payload"]), 2)
                execute.assert_not_called()


if __name__ == "__main__":
    unittest.main()
