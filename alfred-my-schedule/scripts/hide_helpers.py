#!/usr/bin/env python3
"""Keep My Schedule's technical app bundles out of Alfred's app search."""
from pathlib import Path
import plistlib
import subprocess
import sys

TAG = 'alfred:ignore'
ATTRIBUTE = 'com.apple.metadata:_kMDItemUserTags'


def hide_helper(path):
    path = Path(path).resolve()
    info = plistlib.loads((path/'Contents/Info.plist').read_bytes())
    if info.get('CFBundleIdentifier') != 'com.ariestwn.my-schedule-rust.helper':
        raise ValueError('Expected a My Schedule helper: '+str(path))
    attributes = subprocess.check_output(['/usr/bin/xattr', str(path)], text=True).splitlines()
    tags = []
    if ATTRIBUTE in attributes:
        encoded = subprocess.check_output(['/usr/bin/xattr', '-px', ATTRIBUTE, str(path)], text=True)
        tags = plistlib.loads(bytes.fromhex(encoded))
    if not isinstance(tags, list) or not all(isinstance(tag, str) for tag in tags):
        raise ValueError('Unexpected Finder tags on '+str(path))
    if TAG not in tags:
        tags.append(TAG)
        encoded = plistlib.dumps(tags, fmt=plistlib.FMT_BINARY).hex()
        subprocess.run(['/usr/bin/xattr', '-wx', ATTRIBUTE, encoded, str(path)], check=True)


if __name__ == '__main__':
    for path in sys.argv[1:]:
        hide_helper(path)
    print('Hidden from Alfred app search:', len(sys.argv)-1, 'helper bundles')
