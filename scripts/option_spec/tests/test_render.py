import json
from pathlib import Path
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest


ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / "scripts" / "option_spec" / "render.py"
SPEC = ROOT / "scripts" / "option_spec" / "specs" / "tangle.toml"


class RenderOptionSpecTest(unittest.TestCase):
    def test_render_all_projections(self) -> None:
        with TemporaryDirectory() as tmpdir:
            out_dir = Path(tmpdir)
            subprocess.run(
                [sys.executable, str(SCRIPT), "--spec", str(SPEC), "--out", str(out_dir)],
                check=True,
                cwd=ROOT,
            )

            clap = (out_dir / "tangle_clap.rs.inc").read_text(encoding="utf-8")
            argparse_py = (out_dir / "tangle_argparse.py").read_text(encoding="utf-8")
            adoc = (out_dir / "tangle_options.adoc").read_text(encoding="utf-8")
            facts = json.loads((out_dir / "tangle_facts.json").read_text(encoding="utf-8"))

            self.assertIn('long = "config"', clap)
            self.assertIn("force_generated: bool", clap)
            self.assertIn('parser.add_argument("--force-generated"', argparse_py)
            self.assertIn("| `--config` / `-c` | `path` | `weaveback.toml`", adoc)
            self.assertEqual(facts["command"]["name"], "tangle")
            self.assertEqual(facts["options"][1]["long"], "force-generated")


if __name__ == "__main__":
    unittest.main()
