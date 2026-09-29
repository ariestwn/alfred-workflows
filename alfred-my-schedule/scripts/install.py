#!/usr/bin/env python3
"""Install this new workflow atomically, refusing to overwrite existing workflows."""
from pathlib import Path
import os
import datetime
import plistlib
import shutil
import subprocess
import sys
import tempfile
import uuid
import zipfile
from hide_helpers import hide_helper

ROOT=Path(__file__).resolve().parent.parent
BUNDLE='com.ariestwn.my-schedule-rust'

def atomic(path,content,mode):
    path.parent.mkdir(parents=True,exist_ok=True)
    with tempfile.NamedTemporaryFile(dir=path.parent,prefix='.my-schedule-update-',delete=False) as f:
        temporary=Path(f.name)
        try:
            f.write(content)
            f.flush()
            os.fsync(f.fileno())
            temporary.chmod(mode)
            os.replace(temporary,path)
        finally:
            temporary.unlink(missing_ok=True)

def update(target):
    backup=ROOT/'dist/backups'/datetime.datetime.now().strftime('%Y%m%d-%H%M%S-%f')
    shutil.copytree(target,backup)
    hide_helper(backup/'My Schedule.app')
    with zipfile.ZipFile(ROOT/'dist/My Schedule.alfredworkflow') as archive:
        for entry in archive.infolist():
            name=Path(entry.filename)
            if name.is_absolute() or '..' in name.parts: raise ValueError('Invalid archive path')
            content=archive.read(entry)
            if entry.filename=='info.plist':
                original=plistlib.loads((target/name).read_bytes())
                new=plistlib.loads(content)
                assert original['bundleid']==new['bundleid']==BUNDLE
                original['version']=new['version']
                original['readme']=new['readme']
                content=plistlib.dumps(original,sort_keys=False)
            atomic(target/name,content,0o755 if entry.external_attr >> 16 & 0o111 else 0o644)
    subprocess.run(['/usr/bin/codesign','--verify','--deep','--strict',str(target/'My Schedule.app')],check=True)
    hide_helper(target/'My Schedule.app')
    print('Updated:',target)
    print('Backup:',backup)

def main():
    parent=Path(sys.argv[1]).resolve()
    if parent.name != 'workflows' or parent.parent.suffix != '.alfredpreferences':
        raise SystemExit('Choose the workflows folder inside Alfred.alfredpreferences')
    for info in parent.glob('user.workflow.*/info.plist'):
        try: existing=plistlib.loads(info.read_bytes())
        except Exception: continue
        if existing.get('bundleid') == BUNDLE:
            if '--update' in sys.argv[2:]:
                update(info.parent)
                return
            raise SystemExit('Already installed: '+str(info.parent)+'; pass --update to update it while keeping settings.')
    target=parent/('user.workflow.'+str(uuid.uuid4()).upper())
    temporary=Path(tempfile.mkdtemp(prefix='.my-schedule-install-',dir=parent))
    try:
        with zipfile.ZipFile(ROOT/'dist/My Schedule.alfredworkflow') as archive:
            for entry in archive.infolist():
                name=Path(entry.filename)
                if name.is_absolute() or '..' in name.parts: raise ValueError('Invalid archive path')
                path=temporary/name
                path.parent.mkdir(parents=True,exist_ok=True)
                path.write_bytes(archive.read(entry))
                path.chmod(0o755 if entry.external_attr >> 16 & 0o111 else 0o644)
        assert plistlib.loads((temporary/'info.plist').read_bytes())['bundleid']==BUNDLE
        subprocess.run(['/usr/bin/codesign','--verify','--deep','--strict',str(temporary/'My Schedule.app')],check=True)
        hide_helper(temporary/'My Schedule.app')
        os.rename(temporary,target)
    finally:
        if temporary.exists(): shutil.rmtree(temporary)
    (ROOT/'dist/installed-path.txt').write_text(str(target)+'\n')
    print('Installed:',target)

if __name__=='__main__': main()
