#!/usr/bin/env python3
"""Update an existing Calculator Rust installation, preserving its graph/settings."""
from pathlib import Path
import copy
import datetime
import os
import plistlib
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = "com.ariestwn.calculator-rust"

def migrate_metadata(original, source):
    updated = copy.deepcopy(original)
    updated["version"] = source["version"]
    updated["readme"] = source["readme"]
    # Patch the bundled result routes without replacing customized workflow objects.
    for origin, edges in source["connections"].items():
        closing_edges = {
            (edge["destinationuid"], edge.get("sourceoutputuid"))
            for edge in edges if edge.get("vitoclose") is False
        }
        for edge in updated.get("connections", {}).get(origin, []):
            if (edge["destinationuid"], edge.get("sourceoutputuid")) in closing_edges:
                edge["vitoclose"] = False
    fields = updated.setdefault("userconfigurationconfig", [])
    for variable in ["CALC_PRECISION", "CALC_DECIMAL"]:
        definition = next(field for field in source["userconfigurationconfig"]
                          if field["variable"] == variable)
        for index, field in enumerate(fields):
            if field["variable"] == variable:
                replacement = copy.deepcopy(definition)
                default = field.get("config", {}).get("default")
                if default in [pair[1] for pair in definition["config"]["pairs"]]:
                    replacement["config"]["default"] = default
                fields[index] = replacement
                break
        else:
            fields.append(copy.deepcopy(definition))
    return updated

def replace(path, content, mode):
    with tempfile.NamedTemporaryFile(dir=path.parent, prefix=".calculator-update-", delete=False) as f:
        temporary = Path(f.name)
        try:
            f.write(content)
            f.flush()
            os.fsync(f.fileno())
            os.chmod(temporary, mode)
            os.replace(temporary, path)
        finally:
            temporary.unlink(missing_ok=True)

def main():
    target = Path(sys.argv[1]).resolve()
    original = plistlib.loads((target/"info.plist").read_bytes())
    source = plistlib.loads((ROOT/"workflow/info.plist").read_bytes())
    if original.get("bundleid") != BUNDLE or source.get("bundleid") != BUNDLE:
        raise SystemExit("Target must be an existing Calculator Rust workflow")
    subprocess.run(["/usr/bin/codesign","--verify","--strict",str(ROOT/"workflow/calculator")],check=True)
    backup = ROOT/"dist/backups"/datetime.datetime.now().strftime("%Y%m%d-%H%M%S-%f")
    backup.mkdir(parents=True)
    names = ["calculator","info.plist","README.md"]
    for name in names:
        if (target/name).is_file(): shutil.copy2(target/name,backup/name)
    updated = migrate_metadata(original, source)
    replace(target/"calculator",(ROOT/"workflow/calculator").read_bytes(),0o755)
    replace(target/"info.plist",plistlib.dumps(updated,sort_keys=False),0o644)
    replace(target/"README.md",(ROOT/"README.md").read_bytes(),0o644)
    print("Updated:",target)
    print("Version:",source["version"])
    print("Backup:",backup)

if __name__ == "__main__":main()
