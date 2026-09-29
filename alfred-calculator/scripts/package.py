#!/usr/bin/env python3
from pathlib import Path
import plistlib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = "com.ariestwn.calculator-rust"

def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL,BUNDLE+"/"+name)).upper()

def metadata():
    objects,connections,positions=[],{},{}
    def add(name,kind,config,x,y,version=1):
        objects.append(dict(uid=uid(name),type="alfred.workflow."+kind,config=config,version=version))
        positions[uid(name)]=dict(xpos=x,ypos=y)
    def link(source,target,port=None,keep_open=False):
        # vitoclose vetoes closing Alfred; True means "Don't close".
        edge=dict(destinationuid=uid(target),modifiers=0,modifiersubtext="",vitoclose=keep_open)
        if port: edge['sourceoutputuid']=uid(port)
        connections.setdefault(uid(source),[]).append(edge)
    for name,keyword,command,title,description,y in [
        ("calculator","{var:CALC_KEYWORD}","filter","Calculator","Math, units, currencies, dates and time zones",40),
        ("currency","{var:CALC_FX_KEYWORD}","currency","Currency Converter","Type an amount, choose a source, then filter targets",300),
    ]:
        add(name,"input.scriptfilter",dict(keyword=keyword,title=title,subtext=description,
            argumenttype=1,argumenttreatemptyqueryasnil=False,argumenttrimmode=0,withspace=True,
            alfredfiltersresults=False,alfredfiltersresultsmatchmode=0,skipuniversalaction=True,
            queuedelaycustom=1,queuedelayimmediatelyinitially=True,queuedelaymode=0,queuemode=1,
            runningsubtext="Calculating…",script=f'exec ./calculator {command} -- "$1"',
            scriptargtype=1,scriptfile="",escaping=0,type=0),300,y,3)
        link(name,"route")
        add(name+"-universal","trigger.universalaction",dict(acceptsfiles=False,acceptsmulti=0,
            acceptstext=True,acceptsurls=False,name="Calculate Selected Text" if name=="calculator" else "Convert Currency"),40,y)
        link(name+"-universal",name,keep_open=True)
        add(name+"-external","trigger.external",dict(triggerid=name,availableviaurlhandler=False),40,y+120)
        link(name+"-external",name,keep_open=True)
    add("hotkey","trigger.hotkey",dict(action=0,argument=1,focusedappvariable=False,
        focusedappvariablename="",hotkey=0,hotmod=0,hotstring="",leftcursor=False,modsmode=0,relatedAppsMode=0),40,570,2)
    link("hotkey","calculator",keep_open=True)
    add("route","utility.conditional",dict(conditions=[
        dict(inputstring="{var:CALC_ACTION}",matchcasesensitive=True,matchmode=0,matchstring="paste",outputlabel="Paste",uid=uid("route-paste")),
        dict(inputstring="{var:CALC_ACTION}",matchcasesensitive=True,matchmode=0,matchstring="url",outputlabel="Rate provider",uid=uid("route-url")),
    ],elselabel="Copy",hideelse=False),570,170)
    link("route","copy")
    link("route","paste","route-paste")
    link("route","url","route-url")
    for name,y in [("copy",40),("paste",220)]:
        add(name,"output.clipboard",dict(autopaste=name=="paste",clipboardtext="{query}",
            ignoredynamicplaceholders=True,transient=False),820,y,3)
    add("url","action.openurl",dict(browser="",skipqueryencode=True,skipvarencode=False,spaces="",url="{query}"),820,400)
    fields=[
        dict(variable="CALC_KEYWORD",label="Calculator keyword",type="textfield",config=dict(default="calc",trim=True,required=False),description="Type calc followed by an expression."),
        dict(variable="CALC_FX_KEYWORD",label="Currency browser keyword",type="textfield",config=dict(default="fx",trim=True,required=False),description="Type fx 100, then choose source and target currencies."),
        dict(variable="CALC_DECIMAL",label="Number format",type="popupbutton",config=dict(default="auto",pairs=[["Local (follow macOS)","auto"],["United States — 1,234.56","dot"],["Indonesia — 1.234,56","comma"]]),description="Choose decimal and thousands separators for numeric input, results, and Shift-Return rounded copy. Local follows the macOS decimal style. Command-Return raw copy always uses a dot and no grouping."),
        dict(variable="CALC_GROUPING",label="Thousands separators",type="checkbox",config=dict(default=True,required=False,text="Group digits in formatted answers"),description="Raw answers never contain grouping separators."),
        dict(variable="CALC_PRECISION",label="Decimal precision",type="popupbutton",config=dict(default="12",pairs=[["0 (no decimals)" if n==0 else str(n),str(n)] for n in range(31)]),description="Maximum fractional digits (0–30). 0 rounds to a whole number; 2 shows up to two decimal places. Applies to formatted and raw numeric answers; integers retain their precision."),
        dict(variable="CALC_TIMEZONE",label="Local timezone",type="textfield",config=dict(default="auto",trim=True,required=False),description="auto follows macOS. Override with an IANA zone such as Asia/Jakarta."),
        dict(variable="CALC_WORK_HOURS",label="Hours per workday",type="popupbutton",config=dict(default="8",pairs=[[str(n),str(n)] for n in [4,6,7,8,9,10,12]]),description="Work weeks are Monday–Friday. Public holidays are included in annual counts."),
        dict(variable="CALC_PPI",label="Default pixels per inch",type="textfield",config=dict(default="96",trim=True,required=False),description="Used for px conversions. Override in an expression with at 72 ppi."),
        dict(variable="CALC_FAVORITES",label="Favorite currencies",type="textfield",config=dict(default="IDR,USD,EUR,GBP,JPY,SGD,AUD,BTC,ETH",trim=True,required=False),description="Comma-separated codes, shown first in the currency browser."),
        dict(variable="CALC_OFFLINE",label="Offline mode",type="checkbox",config=dict(default=False,required=False,text="Use cached exchange rates only"),description="Math and dates always work offline. Cached rates show their provider and timestamp."),
    ]
    return dict(bundleid=BUNDLE,name="Calculator Rust",description="Natural-language math, units, currencies, dates and time zones.",
        createdby="ariestwn",category="Tools",version="0.1.7",disabled=False,objects=objects,connections=connections,
        uidata=positions,userconfigurationconfig=fields,variables={},variablesdontexport=[],webaddress="",
        readme=(ROOT/"README.md").read_text())

def main():
    (ROOT/"workflow/info.plist").write_bytes(plistlib.dumps(metadata(),sort_keys=False))
    output=ROOT/"dist/Calculator Rust.alfredworkflow"
    output.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(output,"w",compression=zipfile.ZIP_DEFLATED) as z:
        for name in ["calculator","icon.png","info.plist"]:z.write(ROOT/"workflow"/name,name)
        for name in ["README.md","LICENSE","THIRD_PARTY_NOTICES.md"]:z.write(ROOT/name,name)
        for path in sorted((ROOT/"licenses").glob("*")):z.write(path,"licenses/"+path.name)
    print(output)

if __name__=="__main__":main()
