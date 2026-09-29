#!/usr/bin/env python3
"""No access to real Handy data; creates and removes a synthetic 25,000-row DB."""
from pathlib import Path
import json
import os
import sqlite3
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix="handy-benchmark-") as tmp:
    folder = Path(tmp)
    conn = sqlite3.connect(folder/"history.db")
    conn.execute("CREATE TABLE transcription_history (id INTEGER PRIMARY KEY, file_name TEXT, timestamp INTEGER, saved INTEGER, title TEXT, transcription_text TEXT, post_processed_text TEXT)")
    conn.executemany("INSERT INTO transcription_history VALUES (?,?,?,0,?,?,NULL)",
        ((i,f"{i}.wav",1700000000+i,f"Meeting {i}",("A realistic sentence for a meeting transcript. "*50)+(f" unique{i}")) for i in range(25000)))
    conn.commit()
    conn.close()
    env = dict(os.environ,HOME=tmp,HANDY_DATA_DIR=tmp,HANDY_PAGE_SIZE="40",HANDY_PAGE="0",HANDY_PAGE_QUERY="")
    results=[]
    for query in ["history ","history unique1234", "history sentence transcript"]:
        start=time.perf_counter()
        output=subprocess.check_output([str(ROOT/"workflow/handy"),"filter","--",query],env=env)
        elapsed=time.perf_counter()-start
        result=json.loads(output)
        assert result["items"][0]["valid"], result
        results.append(dict(query=query,seconds=round(elapsed,3),rows=len(result["items"]),output_bytes=len(output)))
    print(json.dumps(dict(transcripts=25000,results=results),indent=2))
