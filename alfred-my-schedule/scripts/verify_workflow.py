#!/usr/bin/env python3
from pathlib import Path
import datetime
import json
import os
import plistlib
import re
import subprocess
import tempfile
import time
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BINARY = 'My Schedule.app/Contents/MacOS/my-schedule'

def main():
    with zipfile.ZipFile(ROOT/'dist/My Schedule.alfredworkflow') as archive:
        assert archive.testzip() is None
        assert {BINARY,'My Schedule.app/Contents/Info.plist','info.plist','icon.png','README.md','LICENSE'}.issubset(archive.namelist())
        assert not any(Path(name).name in ('events.json','preferences.json') for name in archive.namelist())
        assert archive.getinfo(BINARY).external_attr >> 16 & 0o111
        assert archive.read(BINARY) == (ROOT/'workflow'/BINARY).read_bytes()
        config = plistlib.loads(archive.read('info.plist'))
        app = plistlib.loads(archive.read('My Schedule.app/Contents/Info.plist'))
        assert app['NSCalendarsFullAccessUsageDescription']
        assert app['NSAppleEventsUsageDescription']
    objects = {o['uid']:o for o in config['objects']}
    assert len(objects) == len(config['objects'])
    for source, edges in config['connections'].items():
        assert source in objects
        ports = {c['uid'] for c in objects[source]['config'].get('conditions',[])}
        for edge in edges:
            assert edge['destinationuid'] in objects
            if 'sourceoutputuid' in edge: assert edge['sourceoutputuid'] in ports
    for obj in objects.values():
        data = obj['config']
        if obj['type'].endswith('action.script'): assert obj['version'] == 2
        if obj['type'].endswith(('input.scriptfilter','action.script')):
            assert data['scriptargtype'] == 1 and '"$1"' in data['script'] and '{query}' not in data['script']
            assert '"./My Schedule.app/Contents/MacOS/my-schedule"' in data['script']
        if obj['type'].endswith('output.clipboard'): assert data['ignoredynamicplaceholders']
    subprocess.run(['/usr/bin/codesign','--verify','--deep','--strict',str(ROOT/'workflow/My Schedule.app')],check=True)
    with tempfile.TemporaryDirectory(prefix='my-schedule-verify-') as tmp:
        tmp = Path(tmp)
        env = dict(os.environ,alfred_workflow_cache=str(tmp/'cache'),alfred_workflow_data=str(tmp/'data'),
            SCHEDULE_TIMEZONE='Asia/Jakarta',SCHEDULE_DAYS='90',SCHEDULE_SHOW_DECLINED='0',SCHEDULE_SHOW_ALL_DAY='1')
        start = int(time.time())+86400
        event = dict(id='demo-recurring',calendar_id='work',title='Demo planning meeting',start=start,end=start+3600,
            recurring=True,self_status=2,url='https://meet.google.com/abc-defg-hij',attendees=[dict(name='Demo Guest',email='mailto:demo@example.invalid')])
        snapshot = dict(calendars=[dict(id='work',title='Demo Work',source='Fixture',writable=True)],events=[event],default_calendar='work')
        fixture = tmp/'fixture.json'
        fixture.write_text(json.dumps(snapshot))
        def call(command,query='',extra=None):
            return json.loads(subprocess.check_output([str(ROOT/'workflow'/BINARY),command,'--fixture',str(fixture),'--',query],env=dict(env,**(extra or {}))))
        agenda = call('agenda')
        assert agenda['skipknowledge'] and agenda['rerun'] == 5
        row = next(row for row in agenda['items'] if row['title']=='Demo planning meeting')
        assert row['mods']['cmd']['valid'] and row['mods']['alt']['variables']['SCHEDULE_ACTION']=='menu'
        assert json.loads(row['arg'])['op']=='open'
        assert json.loads(call('next')['items'][0]['arg'])['op']=='join'
        assert not any(r['title']=='Demo planning meeting' for r in call('agenda','missing-title')['items'])
        assert call('calendars')['items'][1]['title']=='✓ Demo Work'
        menu=call('menu','',{'SCHEDULE_EVENT':json.dumps(dict(id=event['id'],start=start))})
        assert any(r['title']=='Join Google Meet' for r in menu['items'])
        create=call('new','Demo focus | tomorrow 14:00 | 60')['items'][0]
        assert json.loads(create['arg'])['op']=='create'
        assert call('free','tomorrow')['items'][0]['variables']['SCHEDULE_ACTION']=='copy'
        assert not (tmp/'data').exists(), 'Read-only commands wrote workflow preferences'
        assert not (tmp/'cache').exists(), 'Fixture commands read or wrote live snapshots'
        # Compile navigation scripts without executing or granting automation access.
        scripts=re.findall(r'r#"(.*?)"#',(ROOT/'src/actions.rs').read_text(),flags=re.S)
        for i,script in enumerate(scripts):
            subprocess.run(['/usr/bin/osacompile','-o',str(tmp/('navigation-'+str(i)+'.scpt')),'-e',script],check=True,capture_output=True)
        snapshot['events']=[dict(event,id=str(i),title='Demo meeting '+str(i)) for i in range(10000)]
        fixture.write_text(json.dumps(snapshot))
        tick=time.perf_counter()
        large=call('agenda','9999')
        elapsed=(time.perf_counter()-tick)*1000
        assert any(r['title']=='Demo meeting 9999' for r in large['items'])
        print('10,000-event title search: %.1f ms' % elapsed)
    print('Verified archive, %d Alfred objects, native helper signature, action routing, previews, and navigation scripts.' % len(objects))

if __name__ == '__main__': main()
