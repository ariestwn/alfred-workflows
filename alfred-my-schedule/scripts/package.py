#!/usr/bin/env python3
from pathlib import Path
import plistlib
import uuid
import zipfile
from hide_helpers import hide_helper

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = 'com.ariestwn.my-schedule-rust'
EXEC = './My Schedule.app/Contents/MacOS/my-schedule'
def uid(name): return str(uuid.uuid5(uuid.NAMESPACE_URL, BUNDLE+'/'+name)).upper()

def metadata():
    objects, connections, positions = [], {}, {}
    def add(name, kind, config, x, y, version=1):
        objects.append(dict(uid=uid(name),type='alfred.workflow.'+kind,config=config,version=version))
        positions[uid(name)] = dict(xpos=x,ypos=y)
    def link(a,b,port=None):
        edge = dict(destinationuid=uid(b),modifiers=0,modifiersubtext='',vitoclose=True)
        if port: edge['sourceoutputuid'] = uid(port)
        connections.setdefault(uid(a),[]).append(edge)
    for i,(name,keyword,title,subtitle) in enumerate([
        ('agenda','{var:SCHEDULE_KEYWORD}','My Schedule','Upcoming events, next meeting, and title search'),
        ('next','snext','Next Meeting','Open or join your next meeting'),
        ('calendars','scal','Choose Calendars','Choose which synced calendars appear in My Schedule'),
        ('new','snew','Create Event','Title | today 14:00 | 60 | calendar (optional)'),
        ('free','sfree','Copy Availability','Free time: today, tomorrow, week, or YYYY-MM-DD'),
        ('menu','','Event Actions','Join, copy details, or manage in Calendar'),
    ]):
        y = 40 + i*180
        add(name,'input.scriptfilter',dict(keyword=keyword,title=title,subtext=subtitle,argumenttype=1,
            argumenttreatemptyqueryasnil=False,argumenttrimmode=0,withspace=True,alfredfiltersresults=False,
            alfredfiltersresultsmatchmode=0,skipuniversalaction=True,queuedelaycustom=1,
            queuedelayimmediatelyinitially=True,queuedelaymode=0,queuemode=1,runningsubtext='Reading your schedule…',
            script='exec "'+EXEC+'" '+name+' -- "$1"',scriptargtype=1,scriptfile='',escaping=0,type=0),300,y,3)
        link(name,'route')
        if name != 'menu':
            add(name+'-external','trigger.external',dict(triggerid=name,availableviaurlhandler=False),40,y)
            link(name+'-external',name)
    for i,name in enumerate(['agenda','next']):
        key=name+'-hotkey'
        add(key,'trigger.hotkey',dict(action=0,argument=0,focusedappvariable=False,focusedappvariablename='',
            hotkey=0,hotmod=0,hotstring='',leftcursor=False,modsmode=0,relatedAppsMode=0),40,1030+i*120,2)
        link(key,name)
    conditions=[dict(inputstring='{var:SCHEDULE_ACTION}',matchcasesensitive=True,matchmode=0,matchstring=action,
        outputlabel=action.title(),uid=uid('route-'+action)) for action in ['copy','menu']]
    add('route','utility.conditional',dict(conditions=conditions,elselabel='Run action',hideelse=False),590,340)
    link('route','copy','route-copy')
    link('route','menu','route-menu')
    link('route','run')
    add('copy','output.clipboard',dict(autopaste=False,clipboardtext='{query}',ignoredynamicplaceholders=True,transient=False),880,160,3)
    add('run','action.script',dict(script='exec "'+EXEC+'" act -- "$1"',scriptargtype=1,scriptfile='',escaping=0,type=0,
        concurrently=False),880,480,2)
    fields=[
        dict(variable='SCHEDULE_KEYWORD',label='Schedule keyword',type='textfield',config=dict(default='schedule',trim=True,required=True)),
        dict(variable='SCHEDULE_DAYS',label='Look ahead (days)',type='popupbutton',config=dict(default='90',pairs=[[str(n),str(n)] for n in [7,14,30,60,90,180,365]])),
        dict(variable='SCHEDULE_SHOW_ALL_DAY',label='All-day events',type='checkbox',config=dict(default=True,text='Show all-day events')),
        dict(variable='SCHEDULE_SHOW_DECLINED',label='Declined invitations',type='checkbox',config=dict(default=False,text='Show declined events in the agenda')),
        dict(variable='SCHEDULE_MEET_BROWSER',label='Google Meet browser',type='popupbutton',config=dict(default='',pairs=[['Default browser',''],['Google Chrome','com.google.Chrome'],['Safari','com.apple.Safari'],['Arc','company.thebrowser.Browser'],['Brave','com.brave.Browser'],['Microsoft Edge','com.microsoft.edgemac'],['Firefox','org.mozilla.firefox']])),
        dict(variable='SCHEDULE_TIMEZONE',label='Display timezone',type='textfield',config=dict(default='auto',trim=True,required=False),description='auto follows macOS. Or use an IANA name such as Asia/Jakarta.'),
        dict(variable='SCHEDULE_WORK_START',label='Workday starts (hour)',type='textfield',config=dict(default='9',trim=True,required=True),description='0–23, in the display timezone. Used when copying availability.'),
        dict(variable='SCHEDULE_WORK_END',label='Workday ends (hour)',type='textfield',config=dict(default='17',trim=True,required=True),description='1–24. Must be later than the start hour.'),
        dict(variable='SCHEDULE_MINIMUM_FREE',label='Minimum free slot (minutes)',type='textfield',config=dict(default='30',trim=True,required=True)),
    ]
    return dict(bundleid=BUNDLE,name='My Schedule',createdby='ariestwn',category='Productivity',version='0.1.2',
        description='Your macOS Calendar agenda in Alfred. Powered by Rust and EventKit.',disabled=False,
        objects=objects,connections=connections,uidata=positions,userconfigurationconfig=fields,
        variables={},variablesdontexport=[],readme=(ROOT/'README.md').read_text(),webaddress='')

def main():
    hide_helper(ROOT/'workflow/My Schedule.app')
    (ROOT/'workflow/info.plist').write_bytes(plistlib.dumps(metadata(),sort_keys=False))
    with zipfile.ZipFile(ROOT/'dist/My Schedule.alfredworkflow','w',compression=zipfile.ZIP_DEFLATED) as archive:
        for file in sorted((ROOT/'workflow').rglob('*')):
            if file.is_file(): archive.write(file,file.relative_to(ROOT/'workflow'))
        for name in ['README.md','LICENSE','THIRD_PARTY_NOTICES.md']:
            archive.write(ROOT/name,name)
        for file in sorted((ROOT/'licenses').glob('*')):
            archive.write(file,'licenses/'+file.name)
    print(ROOT/'dist/My Schedule.alfredworkflow')

if __name__ == '__main__': main()
