"""Keep plugin dependency rules effective for aliases and platform dependencies."""
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from scripts.ci import check_architecture


class PluginDependencyTests(unittest.TestCase):
    def check_dependencies(self, sdk, host, overrides=None):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            (root / "Cargo.toml").write_text(
                '[workspace.dependencies]\nhidden-transport = { package = "zzclawterm-transport", version = "1" }\n',
                encoding="utf-8",
            )
            crates = (
                "zzclawterm-files", "zzclawterm-recording", "zzclawterm-remote-ops",
                "zzclawterm-core", "zzclawterm-transport", "zzclawterm-terminal", "zzclawterm-store", "zzclawterm-sync",
                "zzclawterm-remote-desktop", "zzclawterm-desktop", "zzclawterm-plugin-api", "zzclawterm-plugin-host",
            )
            for crate in crates:
                directory = root / "crates" / crate
                directory.mkdir(parents=True)
                document = sdk if crate == "zzclawterm-plugin-api" else host if crate == "zzclawterm-plugin-host" else ""
                document = (overrides or {}).get(crate, document)
                (directory / "Cargo.toml").write_text(document, encoding="utf-8")
            with patch.object(check_architecture, "ROOT", root):
                return check_architecture.dependency_errors()

    def test_guest_and_host_keep_their_allowed_dependencies(self):
        self.assertEqual(self.check_dependencies(
            '[dependencies]\nwit-bindgen = "0.49"\n',
            '[dependencies]\nzzclawterm-core = "1"\nzzclawterm-store = "1"\nwasmtime = "39"\n',
        ), [])

    def test_workspace_alias_cannot_hide_sdk_transport_dependency(self):
        errors = self.check_dependencies('[dependencies]\nhidden-transport.workspace = true\n', "")
        self.assertEqual(errors, ["crate_boundary: zzclawterm-plugin-api must not depend on zzclawterm-transport"])

    def test_target_and_build_aliases_cannot_hide_presentation_dependencies(self):
        errors = self.check_dependencies(
            '[target.\'cfg(windows)\'.dev-dependencies]\npresentation = { package = "gpui-base", version = "1" }\n',
            '[build-dependencies]\nshell = { package = "zzclawterm-desktop", version = "1" }\n',
        )
        self.assertCountEqual(errors, [
            "crate_boundary: zzclawterm-plugin-api must not depend on gpui-base",
            "crate_boundary: zzclawterm-plugin-host must not depend on zzclawterm-desktop",
        ])

    def test_transport_cannot_depend_on_business_crates_even_through_aliases(self):
        for crate in ["zzclawterm-files", "zzclawterm-recording", "zzclawterm-remote-ops"]:
            with self.subTest(crate=crate):
                errors = self.check_dependencies("", "", {"zzclawterm-transport":
                    f'[target.\'cfg(windows)\'.dev-dependencies]\nhidden = {{ package = "{crate}", version = "1" }}\n'})
                self.assertEqual(errors, [f"crate_boundary: zzclawterm-transport must not depend on {crate}"])

    def test_business_crates_remain_independent_and_recording_needs_no_transport(self):
        for source, target in [("zzclawterm-files", "zzclawterm-remote-ops"),
                               ("zzclawterm-remote-ops", "zzclawterm-recording"),
                               ("zzclawterm-recording", "zzclawterm-transport")]:
            with self.subTest(source=source, target=target):
                errors = self.check_dependencies("", "", {source:
                    f'[dependencies]\nhidden = {{ package = "{target}", version = "1" }}\n'})
                self.assertEqual(errors, [f"crate_boundary: {source} must not depend on {target}"])


if __name__ == "__main__":
    unittest.main()
