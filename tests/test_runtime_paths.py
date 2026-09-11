from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from GalTransl.AppSettings import load_app_settings, save_app_settings
from GalTransl.RuntimePaths import (
    get_active_dict_dir,
    get_app_settings_path,
    get_dict_dir,
    get_plugins_dir,
    get_resource_root,
    get_translation_guidelines_dir,
    resolve_dict_dir,
)


class RuntimePathTests(unittest.TestCase):
    def test_resource_root_honours_environment_override(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with patch.dict(os.environ, {"GALTRANSL_RESOURCE_DIR": str(root)}):
                self.assertEqual(get_resource_root(), root.resolve())
                self.assertEqual(get_plugins_dir(), root.resolve() / "plugins")
                self.assertEqual(get_dict_dir(), root.resolve() / "Dict")
                self.assertEqual(
                    get_translation_guidelines_dir(),
                    root.resolve() / "translation_guidelines",
                )

    def test_active_dict_dir_seeds_from_bundled_resources(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            resource_dir = root / "resource"
            data_dir = root / "data"
            bundled_dict = resource_dir / "Dict"
            bundled_dict.mkdir(parents=True)
            (bundled_dict / "common.txt").write_text("bundled", encoding="utf-8")

            env = {
                "GALTRANSL_RESOURCE_DIR": str(resource_dir),
                "GALTRANSL_DATA_DIR": str(data_dir),
            }
            with patch.dict(os.environ, env):
                active = get_active_dict_dir()
                self.assertEqual(active, data_dir / "Dict")
                self.assertEqual(resolve_dict_dir("Dict"), active)
                self.assertEqual((active / "common.txt").read_text(encoding="utf-8"), "bundled")

                (active / "common.txt").write_text("user", encoding="utf-8")
                self.assertEqual(get_active_dict_dir(), active)
                self.assertEqual((active / "common.txt").read_text(encoding="utf-8"), "user")

                (active / "common.txt").unlink()
                get_active_dict_dir()
                self.assertFalse((active / "common.txt").exists())

    @unittest.skipIf(os.name == "nt", "XDG paths are Linux/Unix-specific")
    def test_save_app_settings_uses_xdg_config_home(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            xdg = Path(tmp) / "config"
            env = {
                "XDG_CONFIG_HOME": str(xdg),
                "GALTRANSL_APP_SETTINGS_PATH": "",
            }
            with patch.dict(os.environ, env):
                settings_path = get_app_settings_path()
                saved = save_app_settings(
                    {
                        "printTranslationLogInTerminal": False,
                        "maxConcurrentJobs": 7,
                    }
                )
                self.assertEqual(settings_path, xdg / "GalTransl" / "app_settings.json")
                self.assertTrue(settings_path.is_file())
                self.assertEqual(
                    json.loads(settings_path.read_text(encoding="utf-8")),
                    saved,
                )
                self.assertEqual(load_app_settings()["maxConcurrentJobs"], 7)


if __name__ == "__main__":
    unittest.main()
