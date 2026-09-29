#!/usr/bin/env python3
import plistlib
import zipfile
from package import ROOT,BUNDLE,FILES,uid

with zipfile.ZipFile(ROOT/"dist/Screenshots Rust.alfredworkflow") as archive:
    assert archive.testzip() is None
    assert set(archive.namelist())==set(FILES+["README.md","LICENSE"])
    data=plistlib.loads(archive.read("info.plist"))
    assert data["bundleid"]==BUNDLE
    objects={o['uid']:o for o in data['objects']}
    def visit(node,ancestors):
        assert node not in ancestors,"Workflow contains a cycle"
        for edge in data['connections'].get(node,[]):
            assert edge['destinationuid'] in objects
            visit(edge['destinationuid'],ancestors|{node})
    for node in objects: visit(node,set())
    assert objects[uid('grid')]['config']['filterable'] is False
    for name in ['shots','grid']: assert archive.getinfo(name).external_attr>>16&0o111
    assert len(objects)==len(data['objects'])
    assert not data['variables']
print('Workflow package, paging graph, and executable permissions verified.')
