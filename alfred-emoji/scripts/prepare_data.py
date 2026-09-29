#!/usr/bin/env python3
"""Build data/emoji.tsv from Unicode emoji-test.txt and CLDR English annotations."""
from pathlib import Path
import json

ROOT = Path(__file__).resolve().parent.parent
DATA = ROOT / "data"
TONES = {chr(c) for c in range(0x1F3FB, 0x1F400)}


def annotations():
    table = {}
    for name in ["annotations.json", "derived.json"]:
        doc = json.loads((DATA / name).read_text())
        root = doc.get("annotations") or doc.get("annotationsDerived")
        for key, value in root["annotations"].items():
            table.setdefault(key.replace("\ufe0f", ""), value)
    return table


def main():
    notes = annotations()
    rows, group, subgroup = [], "", ""
    for line in (DATA / "emoji-test.txt").read_text().splitlines():
        if line.startswith("# group:"):
            group = line.split(":", 1)[1].strip()
        elif line.startswith("# subgroup:"):
            subgroup = line.split(":", 1)[1].strip()
        elif line and not line.startswith("#") and "; fully-qualified" in line:
            if group == "Component":
                continue
            emoji, name = line.split("#", 1)[1].strip().split(" ", 1)
            if TONES & set(emoji):
                continue
            name = name.split(" ", 1)[1]
            note = notes.get(emoji.replace("\ufe0f", ""), {})
            words = [w for w in note.get("default", []) if w.lower() != name.lower()]
            fields = [emoji, name, group, subgroup, "|".join(words)]
            assert all("\t" not in f and "\n" not in f for f in fields)
            rows.append("\t".join(fields))
    (DATA / "emoji.tsv").write_text("\n".join(rows) + "\n")
    print(f"{len(rows)} emoji written to data/emoji.tsv")


if __name__ == "__main__":
    main()
