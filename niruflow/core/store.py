import sqlite3
from datetime import datetime,timezone
from .config import STATE

def connect():
    STATE.mkdir(parents=True,exist_ok=True)
    db=sqlite3.connect(STATE/"history.sqlite3",timeout=20)
    db.execute("PRAGMA busy_timeout=20000")
    db.execute("""CREATE TABLE IF NOT EXISTS actions(
       id INTEGER PRIMARY KEY, ts TEXT NOT NULL, rule TEXT NOT NULL,
       source TEXT NOT NULL, destination TEXT NOT NULL, status TEXT NOT NULL,
       bytes INTEGER NOT NULL DEFAULT 0, message TEXT NOT NULL DEFAULT '')""")
    db.commit()
    return db

def record(rule,source,destination,status,size=0,message=""):
    with connect() as db:
        db.execute("INSERT INTO actions(ts,rule,source,destination,status,bytes,message) VALUES(?,?,?,?,?,?,?)",
                   (datetime.now(timezone.utc).isoformat(),rule,str(source),str(destination),status,size,message))

def history(limit=100):
    with connect() as db:
        return db.execute("SELECT id,ts,rule,source,destination,status,bytes,message FROM actions ORDER BY id DESC LIMIT ?",(limit,)).fetchall()
