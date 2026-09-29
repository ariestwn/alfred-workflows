#!/usr/bin/env python3
"""Benchmark bounded gallery output; never print private filenames or image contents."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--folder',type=Path)
args = parser.parse_args()

with tempfile.TemporaryDirectory(prefix='screenshots-rust-benchmark-') as directory:
    temp = Path(directory)
    folder = args.folder.expanduser() if args.folder else temp/'images'
    if not args.folder:
        folder.mkdir()
        fixture = temp/'fixture.png'
        subprocess.run(['/usr/bin/swift',str(ROOT/'scripts/artwork.swift'),str(fixture),'--fixture'],
                       env=dict(os.environ,CLANG_MODULE_CACHE_PATH=str(temp/'swift-cache')),check=True)
        for n in range(25000):
            os.link(fixture,folder/f'screenshot-{n:05}.png')
    env = dict(os.environ,SHOTS_FOLDER=str(folder),alfred_workflow_cache=str(temp/'cache'),
               SHOTS_PAGE_SIZE='48',SHOTS_PIXELS='384',SHOTS_STATS='1',SHOTS_PAGE='0')
    def run(label, page='0', query=''):
        started=time.perf_counter()
        result=subprocess.run([str(ROOT/'workflow/shots'),'browse',query],env=dict(env,SHOTS_PAGE=page),
            capture_output=True,text=True,check=True,cwd=ROOT/'workflow',timeout=60)
        elapsed=time.perf_counter()-started
        data=json.loads(result.stdout)
        stats=json.loads(result.stderr.strip().splitlines()[-1])
        assert len(data['items'])<=51
        assert stats['page_images']<=48
        stats.update(label=label,wall_ms=round(elapsed*1000,2),json_bytes=len(result.stdout.encode()))
        print(json.dumps(stats))
        return stats
    cold=run('first page, cold cache')
    warm=run('first page, warm cache')
    assert warm['cached']+warm['failed_previews']==warm['page_images']
    run('last page',str(max(0,cold['pages']-1)))
    if not args.folder:
        search=run('global filename search',query='screenshot-00000')
        assert search['matched']==1
