import argparse
import json
import sys
from .core.config import load,save
from .core.version import VERSION
from .core.locations import location_paths
from .core.store import history
from .modules.cleanup import scan,quarantine,restore
from .modules.projects import analyze
from .modules.docker import inspect

def main():
    p=argparse.ArgumentParser(prog="niruflow")
    sub=p.add_subparsers(dest="cmd")
    sub.add_parser("desktop")
    sub.add_parser("status")
    sub.add_parser("rules")
    sub.add_parser("history")
    sub.add_parser("docker")
    dcp=sub.add_parser("docker-cleanup-preview")
    dcp.add_argument("--days",type=int,default=30)
    dcp.add_argument("--include-unused",action="store_true")
    sub.add_parser("profile")
    dc=sub.add_parser("docker-containers")
    dc.add_argument("--short",action="store_true")
    sub.add_parser("locations")
    sub.add_parser("media")
    sub.add_parser("compose")
    sub.add_parser("large-files")
    old=sub.add_parser("forgotten")
    old.add_argument("--months",type=int,default=12)
    old.add_argument("--limit",type=int,default=100)
    sub.add_parser("doctor")
    pr=sub.add_parser("projects")
    pr.add_argument("--keep",type=int,default=2)
    sc=sub.add_parser("scan")
    sc.add_argument("rule")
    run=sub.add_parser("run")
    run.add_argument("rule")
    run.add_argument("--apply",action="store_true")
    run.add_argument("--allow-synced",action="store_true")
    rs=sub.add_parser("restore")
    rs.add_argument("id",type=int)
    sub.add_parser("run-enabled")
    sub.add_parser("scheduler-status")
    backup=sub.add_parser("backup-export")
    backup.add_argument("destination")
    restore_config=sub.add_parser("backup-import")
    restore_config.add_argument("source")
    args=p.parse_args()
    try:
        cfg=load()
        if args.cmd in (None,"desktop"):
            from .desktop.app import launch
            return launch()
        if args.cmd=="status":
            print(json.dumps({"version":VERSION,"rules":len(cfg["rules"]),
                              "enabled":sum(bool(x.get("enabled")) for x in cfg["rules"])},indent=2))
        elif args.cmd=="rules":
            print(json.dumps(cfg["rules"],indent=2))
        elif args.cmd=="history":
            print(json.dumps(history(),indent=2))
        elif args.cmd=="doctor":
            from .modules.doctor import check
            print(json.dumps(check(),indent=2))
        elif args.cmd=="forgotten":
            from pathlib import Path
            from .modules.forgotten import find_old_files
            print(json.dumps(find_old_files(Path.home(),months=args.months,limit=args.limit),indent=2))
        elif args.cmd=="media":
            from .modules.media import inventory,default_roots
            print(json.dumps(inventory(cfg.get("media_roots",default_roots())),indent=2))
        elif args.cmd=="locations":
            from .core.locations import locations
            print(json.dumps(locations(cfg),indent=2,ensure_ascii=False))
        elif args.cmd=="compose":
            from .core.locations import location_paths
            from .modules.compose import discover
            print(json.dumps(discover(location_paths(cfg,'docker')),indent=2,ensure_ascii=False))
        elif args.cmd=="large-files":
            from .core.locations import location_paths
            from .modules.storage import largest
            print(json.dumps(largest(location_paths(cfg,'storage')),indent=2,ensure_ascii=False))
        elif args.cmd=="docker-cleanup-preview":
            from .modules.docker_cleanup import preview
            print(json.dumps(preview(args.days,args.include_unused),indent=2))
        elif args.cmd=="docker":
            print(json.dumps(inspect(),indent=2))
        elif args.cmd=="docker-containers":
            data=inspect()
            if args.short:
                for x in data['containers']:
                    print(f"{x.get('Names','?')}\t{x.get('Status','?')}\t{x.get('Image','?')}")
            else:
                print(json.dumps(data['containers'],indent=2))
        elif args.cmd=="profile":
            print(json.dumps({'active':cfg['active_profile'],'profiles':list(cfg['profiles'])},indent=2))
        elif args.cmd=="projects":
            print(json.dumps([item for path in location_paths(cfg,"projects") for item in analyze(path,args.keep)],indent=2))
        elif args.cmd=="restore":
            print(restore(args.id))
        elif args.cmd in ("scan","run"):
            rule=next((x for x in cfg["rules"] if x["id"]==args.rule),None)
            if rule is None:
                raise ValueError("Unknown rule")
            candidates=scan(rule)
            if args.cmd=="run" and args.apply:
                if not rule.get("enabled"):
                    raise PermissionError("Rule is disabled")
                print(json.dumps({"moved":quarantine(rule,candidates,args.allow_synced)},indent=2))
            else:
                print(json.dumps({"dry_run":True,"candidates":candidates},indent=2))
        elif args.cmd=="scheduler-status":
            from .core.scheduler import status
            print(json.dumps(status(),indent=2))
        elif args.cmd=="backup-export":
            from .core.backup import export_config
            print(export_config(args.destination))
        elif args.cmd=="backup-import":
            from .core.backup import import_config
            import_config(args.source)
            print("Configuration imported. Restart GUI to reload.")
        elif args.cmd=="run-enabled":
            from .core.safety import is_nextcloud
            for rule in cfg["rules"]:
                if rule.get("enabled") and rule.get("automatic"):
                    try:
                        if is_nextcloud(rule["path"]):
                            print(f"{rule['id']}: skipped synced path; manual confirmation required")
                            continue
                        moved=quarantine(rule,scan(rule))
                        print(f"{rule['id']}: quarantined {len(moved)}")
                    except Exception as e:
                        print(f"{rule['id']}: {e}",file=sys.stderr)
        return 0
    except Exception as exc:
        print(f"NIRUFLOW: {exc}",file=sys.stderr)
        return 1

if __name__=="__main__":
    raise SystemExit(main())
