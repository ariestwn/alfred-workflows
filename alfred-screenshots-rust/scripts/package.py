#!/usr/bin/env python3
from pathlib import Path
import plistlib
import uuid
import zipfile

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = "com.ariestwn.screenshots-rust"
FILES = ["shots", "grid", "info.plist", "icon.png", "previous.png", "next.png"]

def uid(name):
    return str(uuid.uuid5(uuid.NAMESPACE_URL, BUNDLE + "/" + name)).upper()

def metadata():
    objects, connections, positions = [], {}, {}
    def add(name, kind, config, x, y, version=1):
        objects.append(dict(uid=uid(name), type="alfred.workflow."+kind, config=config, version=version))
        positions[uid(name)] = dict(xpos=x,ypos=y)
    def link(source, target, modifier=0, title="", port=None, close=True):
        edge=dict(destinationuid=uid(target), modifiers=modifier, modifiersubtext=title,vitoclose=close)
        if port: edge['sourceoutputuid']=uid(port)
        connections.setdefault(uid(source),[]).append(edge)
    def condition(name, field, choices, x, y):
        add(name,"utility.conditional",dict(conditions=[dict(inputstring="{var:"+field+"}",
            matchcasesensitive=True,matchmode=0,matchstring=value,outputlabel=label,uid=uid(name+"-"+value))
            for value,label in choices],elselabel="File",hideelse=False),x,y)

    add("keyword","input.keyword",dict(keyword="{var:SHOTS_KEYWORD}",argumenttype=1,
        text="Browse Screenshots",subtext="Newest first · Type a filename or date to search the entire folder",
        withspace=True,skipuniversalaction=True),40,80)
    add("first-page","utility.argument",dict(argument="{query}",passthroughargument=False,
        variables={"SHOTS_PAGE":"0"}),240,80)
    link("keyword","first-page")
    link("first-page","grid")
    add("external","trigger.external",dict(triggerid="browse",availableviaurlhandler=False),240,260)
    link("external","grid")
    add("grid","userinterface.grid",dict(columncount=4,filterable=False,fixedorder=True,
        imageaspect=0,inputfile="grid",inputtype=1,loadingtext="Loading screenshot previews…",
        showsubtitles=True,showtitles=True,subtitlesinfooter=True,titlesinfooter=True),460,140)
    for name, intent, modifier, label, y in [
        ("preview","preview",0,"Preview image",40),
        ("copy","copy",1048576,"Copy image",160),
        ("reveal","reveal",524288,"Reveal in Finder",280),
        ("path","path",262144,"Copy file path",400)]:
        add(name+"-intent","utility.argument",dict(argument="{query}",passthroughargument=False,
            variables={"SHOTS_INTENT":intent}),680,y)
        link("grid",name+"-intent",modifier,label,close=name=="preview")
        link(name+"-intent","kind")
    condition("kind","SHOTS_KIND",[("page","Change page")],900,100)
    link("kind","reopen",port="kind-page")
    link("kind","intent")
    add("reopen","output.callexternaltrigger",dict(externaltriggerid="browse",passinputasargument=True,
        passvariables=True,workflowbundleid="self"),1120,40)
    condition("intent","SHOTS_INTENT",[("copy","Copy image"),("reveal","Reveal file"),("path","Copy path")],1120,220)
    link("intent","image")
    link("intent","clipboard-image",port="intent-copy",close=False)
    link("intent","finder",port="intent-reveal",close=False)
    link("intent","clipboard-path",port="intent-path",close=False)
    add("image","userinterface.image",dict(imageresizemode=0,stackview=True),1360,100)
    add("clipboard-image","automation.task",dict(tasksettings={},
        taskuid="com.alfredapp.automation.core/macOS/clipboard.set.image.data"),1360,240)
    add("finder","action.revealfile",dict(path=""),1360,380)
    add("clipboard-path","output.clipboard",dict(autopaste=False,clipboardtext="{query}",
        ignoredynamicplaceholders=True,transient=False),1360,520,3)
    link("image","clipboard-image",1048576,"Copy image",close=False)
    link("image","finder",524288,"Reveal in Finder",close=False)
    link("image","clipboard-path",262144,"Copy file path",close=False)

    fields=[dict(variable="SHOTS_KEYWORD",label="Keyword",description="Search the entire folder before opening the gallery.",
        type="textfield",config=dict(default="shots",required=False,trim=True)),
        dict(variable="SHOTS_FOLDER",label="Screenshots folder",description="Images directly inside this folder are shown, newest modification first.",
        type="filepicker",config=dict(default="~/Pictures/Screenshot",filtermode=1,required=True,placeholder="Choose screenshots folder")),
        dict(variable="SHOTS_PAGE_SIZE",label="Images per page",description="Smaller pages open faster. Use Next / Previous tiles to browse all images.",
        type="popupbutton",config=dict(default="48",pairs=[[str(n),str(n)] for n in [12,24,48,72,120]])),
        dict(variable="SHOTS_PIXELS",label="Preview size",description="Maximum thumbnail edge in pixels. Originals are used only when you preview or copy an image.",
        type="popupbutton",config=dict(default="384",pairs=[["Small (256 px)","256"],["Standard (384 px)","384"],["Large (512 px)","512"]]))]
    return dict(bundleid=BUNDLE,name="Screenshots Rust",description="Fast screenshot browsing with pages and cached previews.",
        createdby="ariestwn",category="Productivity",version="0.1.0",disabled=False,
        objects=objects,connections=connections,uidata=positions,userconfigurationconfig=fields,
        variables={},variablesdontexport=[],webaddress="",readme=(ROOT/"README.md").read_text())

def main():
    (ROOT/"workflow/info.plist").write_bytes(plistlib.dumps(metadata(),sort_keys=False))
    out=ROOT/"dist/Screenshots Rust.alfredworkflow"
    out.parent.mkdir(exist_ok=True)
    with zipfile.ZipFile(out,"w",compression=zipfile.ZIP_DEFLATED) as archive:
        for name in FILES: archive.write(ROOT/"workflow"/name,name)
        for name in ["README.md","LICENSE"]: archive.write(ROOT/name,name)
    print(out)

if __name__=="__main__": main()
