#!/usr/bin/env python3
from pathlib import Path
import copy
import json
import os
import plistlib
import subprocess
import tempfile
import time
import zipfile
from install_update import migrate_metadata

ROOT=Path(__file__).resolve().parent.parent

def main():
    with zipfile.ZipFile(ROOT/"dist/Calculator Rust.alfredworkflow") as z:
        assert z.testzip() is None
        assert {'calculator','info.plist','icon.png','README.md','LICENSE','THIRD_PARTY_NOTICES.md'}.issubset(z.namelist())
        assert not any(name.endswith(('fiat.json','crypto.json')) for name in z.namelist())
        assert z.getinfo('calculator').external_attr>>16 & 0o111
        assert z.read('calculator')==(ROOT/'workflow/calculator').read_bytes()
        config=plistlib.loads(z.read('info.plist'))
    objects={obj['uid']:obj for obj in config['objects']}
    assert len(objects)==len(config['objects'])
    for source,edges in config['connections'].items():
        assert source in objects
        ports={c['uid'] for c in objects[source]['config'].get('conditions',[])}
        for edge in edges:
            assert edge['destinationuid'] in objects
            if 'sourceoutputuid' in edge: assert edge['sourceoutputuid'] in ports
            destination = objects[edge['destinationuid']]
            assert edge['vitoclose'] == destination['type'].endswith('input.scriptfilter'), (source, edge)
    precision = next(field for field in config['userconfigurationconfig'] if field['variable'] == 'CALC_PRECISION')
    assert [value for _, value in precision['config']['pairs']] == [str(n) for n in range(31)]
    assert precision['config']['default'] == '12'
    number_format = next(field for field in config['userconfigurationconfig'] if field['variable'] == 'CALC_DECIMAL')
    assert number_format['label'] == 'Number format'
    assert [value for _, value in number_format['config']['pairs']] == ['auto', 'dot', 'comma']
    # An in-place update must fix old routes/options while retaining user settings.
    original = copy.deepcopy(config)
    original['version'] = '0.1.2'
    original['variables'] = {'CALC_KEYWORD': 'c', 'CALC_PRECISION': '6', 'CALC_DECIMAL': 'comma'}
    for edges in original['connections'].values():
        for edge in edges: edge['vitoclose'] = True
    old_precision = next(field for field in original['userconfigurationconfig'] if field['variable'] == 'CALC_PRECISION')
    old_precision['config'] = {'default': '6', 'pairs': [['6', '6'], ['12', '12']]}
    old_number_format = next(field for field in original['userconfigurationconfig'] if field['variable'] == 'CALC_DECIMAL')
    old_number_format['label'] = 'Decimal separator'
    old_number_format['config'] = {'default': 'comma', 'pairs': [['System setting', 'auto'], ['Dot', 'dot'], ['Comma', 'comma']]}
    custom = {'uid': 'custom', 'type': 'alfred.workflow.output.notification', 'config': {}}
    original['objects'].append(custom)
    original['connections']['custom'] = [dict(destinationuid=custom['uid'], vitoclose=True)]
    before = copy.deepcopy(original)
    updated = migrate_metadata(original, config)
    assert original == before
    assert updated['objects'] == original['objects']
    assert updated['variables'] == original['variables']
    assert updated['uidata'] == original['uidata']
    assert updated['connections'].pop('custom') == original['connections']['custom']
    assert updated['connections'] == config['connections']
    new_precision = next(field for field in updated['userconfigurationconfig'] if field['variable'] == 'CALC_PRECISION')
    assert new_precision['config']['default'] == '6'
    assert new_precision['config']['pairs'] == precision['config']['pairs']
    new_number_format = next(field for field in updated['userconfigurationconfig'] if field['variable'] == 'CALC_DECIMAL')
    assert new_number_format['label'] == 'Number format'
    assert new_number_format['config']['default'] == 'comma'
    assert new_number_format['config']['pairs'] == number_format['config']['pairs']
    for obj in objects.values():
        c=obj['config']
        if obj['type'].endswith('input.scriptfilter'):
            assert c['scriptargtype']==1 and '"$1"' in c['script'] and '{query}' not in c['script']
        if obj['type'].endswith('output.clipboard'): assert c['ignoredynamicplaceholders']
    with tempfile.TemporaryDirectory(prefix='alfred-calculator-verify-') as tmp:
        env=dict(os.environ,CALC_OFFLINE='1',CALC_DECIMAL='dot',CALC_TIMEZONE='Asia/Jakarta',CALC_CACHE_DIR=tmp,CALC_PRECISION='12',CALC_GROUPING='1')
        snapshot=dict(fetched=int(time.time()),as_of=int(time.time()),rates=dict(USD=1,GBP=0.8,EUR=0.9,IDR=16000))
        (Path(tmp)/'fiat.json').write_text(json.dumps(snapshot))
        crypto=dict(fetched=int(time.time()),as_of=int(time.time()),rates=dict(USD=1,BTC=0.00002,ETH=0.0005))
        (Path(tmp)/'crypto.json').write_text(json.dumps(crypto))
        def call(mode,q):return json.loads(subprocess.check_output([str(ROOT/'workflow/calculator'),mode,'--',q],env=env))
        for q,raw in [('52% of 900','468'),('2^100','1267650600228229401496703205376'),('10ft in m','3.048'),('100 usd in gbp','80'),('1 BTC in USD','50000'),('USD1K','1000'),('2 inches in px at 72 ppi','144'),('145 mins to timespan','8700'),('workhours in 2023','2080')]:
            output=call('filter',q)
            assert output['items'][0]['mods']['cmd']['arg']==raw,(q,output)
        result=call('currency','100 usd gbp')['items'][0]
        for q,raw in [('10000*10%','1000'),('1000000-10%','900000'),('10000+10%','11000'),('10000/10%','100000')]:
            output=call('filter',q)
            assert output['items'][0]['mods']['cmd']['arg']==raw,(q,output)
        assert result['title']=='80 GBP'
        assert 'cmd+shift' not in result['mods']
        assert result['text']['largetype']=='100 USD to GBP = 80 GBP'
        assert result['mods']['alt']['variables']['CALC_ACTION']=='paste'
        rounded = call('filter', '177904.72955 IDR')['items'][0]
        assert rounded['mods']['shift']['arg'] == '177,905 IDR'
        assert rounded['mods']['shift']['variables']['CALC_ACTION'] == 'copy'
        assert rounded['arg'] == '177,904.72955 IDR'
        assert call('currency', '1.234 USD GBP')['items'][0]['mods']['shift']['arg'] == '1 GBP'
        assert 'shift' not in call('filter', 'today')['items'][0]['mods']
        env['CALC_DECIMAL'] = 'comma'
        local_result = call('filter', '177904,72955 IDR')['items'][0]
        assert local_result['arg'] == '177.904,72955 IDR'
        assert local_result['mods']['shift']['arg'] == '177.905 IDR'
        assert local_result['mods']['cmd']['arg'] == '177904.72955'
        env['CALC_GROUPING'] = '0'
        assert call('filter', '177904,72955 IDR')['items'][0]['mods']['shift']['arg'] == '177905 IDR'
        env['CALC_DECIMAL'], env['CALC_GROUPING'] = 'dot', '1'
        assert call('filter','2 +')['items'][0]['valid'] is False
        assert call('filter','2024-02-30')['items'][0]['valid'] is False
        for query, expected in [('days since today', '0'), ('day since yesterday to today', '1')]:
            item = call('filter', query)['items'][0]
            assert item['mods']['cmd']['arg'] == expected, item
        assert call('filter','days since 31 Feb')['items'][0]['valid'] is False
        assert call('filter','2026-03-08 2:30am New York in London')['items'][0]['valid'] is False
        assert len(call('filter','2026-11-01 1:30am New York in London')['items'])==2
        assert len(call('currency','100')['items'])==184
        assert all(not row['valid'] for row in call('filter','')['items'])
        for digits, title, raw in [('0','≈ 1,235','1235'),('1','≈ 1,234.6','1234.6'),('2','≈ 1,234.57','1234.57'),('3','1,234.567','1234.567')]:
            env['CALC_PRECISION'] = digits
            item = call('filter','1234.567')['items'][0]
            assert item['title'] == title and item['arg'] == title, item
            assert item['mods']['cmd']['arg'] == raw, item
        env['CALC_PRECISION'] = '0'
        assert call('filter','145 mins to timespan')['items'][0]['mods']['cmd']['arg'] == '8700'
        assert call('currency','1.234 USD GBP')['items'][0]['title'] == '≈ 1 GBP'
        for digits, expected in [('invalid','12'),('','12'),('-1','12'),('99','30')]:
            env['CALC_PRECISION'] = digits
            fallback = call('filter','1/3')
            env['CALC_PRECISION'] = expected
            assert call('filter','1/3') == fallback
        assert not list(Path(tmp).glob('*.lock'))
    print(f"Verified archive, {len(objects)} Alfred objects, window closing, settings migration, decimal precision, clipboard routing, currency navigation, and packaged calculator.")

if __name__=='__main__':main()
