#!/usr/bin/env python3
"""Build-time metadata only; installed actions run the Rust executable."""
from pathlib import Path
import plistlib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = "com.ariestwn.handy-rust"

def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, BUNDLE + "/" + name)).upper()

def metadata():
    objects, connections, positions = [], {}, {}
    def add(name, kind, config, x, y, version=1):
        objects.append(dict(uid=uid(name), type="alfred.workflow." + kind, config=config, version=version))
        positions[uid(name)] = dict(xpos=x, ypos=y)
    def link(source, target, port=None, close=True):
        edge = dict(destinationuid=uid(target), modifiers=0, modifiersubtext="", vitoclose=close)
        if port: edge["sourceoutputuid"] = uid(port)
        connections.setdefault(uid(source), []).append(edge)
    def script(code):
        return dict(script=code, scriptfile="", scriptargtype=1, escaping=0, type=0, concurrently=False)
    def sf(name, keyword, prefix, title, y):
        config = script('exec ./handy filter ' + (prefix + ' ' if prefix else '') + '-- "$1"')
        config.update(keyword=keyword, title=title, subtext="Handy speech-to-text", argumenttype=1,
                      withspace=True, alfredfiltersresults=False, alfredfiltersresultsmatchmode=0,
                      argumenttreatemptyqueryasnil=False, argumenttrimmode=0, queuedelaycustom=1,
                      queuedelayimmediatelyinitially=True, queuedelaymode=0, queuemode=1,
                      runningsubtext="Reading Handy…", skipuniversalaction=True)
        add(name,"input.scriptfilter",config,40,y,3)
        link(name,"action")
    sf("main","{var:HANDY_KEYWORD}","","Handy",40)
    for n,(keyword,prefix,title) in enumerate([
        ("hhistory","history","Search Handy Transcripts"),
        ("hsaved","saved","Saved Handy Transcripts"),
        ("hmodel","models","Select Handy Model"),
        ("hlang","languages","Select Handy Language"),
        ("hdict","dictionary","Manage Handy Dictionary"),
        ("hword","add","Add Handy Dictionary Word"),
    ]): sf(prefix,keyword,prefix,title,200+n*140)
    for n,(name,keyword,op,title) in enumerate([
        ("record","hrec","toggle","Toggle Handy Recording"),
        ("last-copy","hcopy","copy","Copy Last Handy Transcript"),
        ("last-paste","hpaste","paste","Paste Last Handy Transcript"),
        ("recordings","hfolder","recordings","Open Handy Recordings"),
        ("cancel","hcancel","cancel","Cancel Handy Recording"),
    ]):
        add(name,"input.keyword",dict(keyword=keyword,argumenttype=2,text=title,
            subtext="Handy",withspace=False,skipuniversalaction=True),350,480+n*140)
        add(name+"-arg","utility.argument",dict(argument='{"op":"'+op+'"}',passthroughargument=False),560,480+n*140)
        link(name,name+"-arg")
        link(name+"-arg","action")
    add("record-hotkey","trigger.hotkey",dict(action=0,argument=1,focusedappvariable=False,
        focusedappvariablename="",hotkey=0,hotmod=0,hotstring="",leftcursor=False,modsmode=0,relatedAppsMode=0),350,1200,2)
    link("record-hotkey","record-arg")
    add("external","trigger.external",dict(triggerid="browse",availableviaurlhandler=False),350,40)
    link("external","main")
    add("action","action.script",script('exec ./handy action -- "$1"'),800,320,2)
    link("action","route")
    routes = ["browse","copy","paste","view","reveal","open"]
    add("route","utility.conditional",dict(conditions=[dict(inputstring="{var:HANDY_ROUTE}",
        matchcasesensitive=True,matchmode=0,matchstring=route,outputlabel=route.title(),uid=uid("route-"+route))
        for route in routes],elselabel="Status or error",hideelse=False),1020,320)
    for route in routes: link("route",route,port="route-"+route)
    link("route","notification")
    add("browse","output.callexternaltrigger",dict(externaltriggerid="browse",passinputasargument=True,
        passvariables=True,workflowbundleid="self"),1260,40)
    for n,name in enumerate(["copy","paste"]):
        add(name,"output.clipboard",dict(autopaste=name=="paste",clipboardtext="{query}",
            ignoredynamicplaceholders=True,transient=False),1260,180+n*140,3)
    add("view","userinterface.text",dict(behaviour=1,fontmode=0,fontsizing=0,
        footertext="↩ Copy this view · Esc to close",inputtype=0,inputfile="",loadingtext="Opening transcript…",
        outputmode=0,scriptinput=0,spellchecking=0,stackview=True),1260,460)
    link("view","copy")
    add("reveal","action.revealfile",dict(path=""),1260,600)
    add("open","action.openfile",dict(openwith="",sourcefile=""),1260,740)
    add("notification","output.notification",dict(title="Handy",text="{var:HANDY_MESSAGE}",
        onlyshowifquerypopulated=False,removeextension=False,lastpathcomponent=False),1260,880)
    fields = [
        dict(variable="HANDY_KEYWORD",label="Main keyword",type="textfield",description="Open all Handy commands.",config=dict(default="handy",trim=True,required=False)),
        dict(variable="HANDY_BINARY",label="Handy binary",type="textfield",description="Executable inside Handy.app. Finish Handy setup before using this workflow.",config=dict(default="/Applications/Handy.app/Contents/MacOS/handy",trim=True,required=True)),
        dict(variable="HANDY_DATA_DIR",label="Handy data folder",type="filepicker",description="Contains history.db, settings_store.json, models, and recordings.",config=dict(default="~/Library/Application Support/com.pais.handy",filtermode=1,required=True)),
        dict(variable="HANDY_HF_CACHE",label="Hugging Face cache (optional)",type="textfield",description="Leave empty to use HF_HUB_CACHE, HF_HOME/hub, or ~/.cache/huggingface/hub.",config=dict(default="",trim=True,required=False)),
        dict(variable="HANDY_PAGE_SIZE",label="Transcripts per page",type="popupbutton",description="Next and Previous rows browse every matching transcript.",config=dict(default="40",pairs=[[str(n),str(n)] for n in [20,40,60,100]])),
    ]
    return dict(bundleid=BUNDLE,name="Handy Rust",description="Recording, transcripts, dictionary, models, and languages for Handy.",
        createdby="ariestwn",category="Productivity",version="0.1.0",disabled=False,objects=objects,
        connections=connections,uidata=positions,userconfigurationconfig=fields,variables={},variablesdontexport=[],
        webaddress="https://handy.computer",readme=(ROOT/"README.md").read_text())

def main():
    (ROOT/"workflow/info.plist").write_bytes(plistlib.dumps(metadata(),sort_keys=False))
    path = ROOT/"dist/Handy Rust.alfredworkflow"
    path.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(path,"w",compression=zipfile.ZIP_DEFLATED) as z:
        for name in ["handy","info.plist","icon.png"]: z.write(ROOT/"workflow"/name,name)
        for name in ["README.md","LICENSE","THIRD_PARTY_NOTICES.md"]: z.write(ROOT/name,name)
    print(path)

if __name__ == "__main__": main()
