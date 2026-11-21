#!/usr/bin/env python3
"""Multi-turn agent evaluation. Records observable events, never semantic scores."""
import argparse
import json
from pathlib import Path
import shutil
import sys
import qualitative_eval as qe


def make_trace(directory, root, private):
    script = directory / 'work'
    code = '''import hashlib,json,pathlib,subprocess,sys,time,uuid
root=pathlib.Path(ROOT)
audit=pathlib.Path(PRIVATE)/'work-audit';audit.mkdir(exist_ok=True)
def snapshot():
    out={}
    for p in sorted(root.rglob('*')):
        if not p.is_file() or p.is_symlink() or '__pycache__' in p.parts:continue
        rel=p.relative_to(root).as_posix()
        if rel.startswith('.amh/') or rel=='.amh.lock':continue
        b=p.read_bytes();out[rel]={'sha256':hashlib.sha256(b).hexdigest()}
        if len(b)<=262144:
            try:out[rel]['text']=b.decode('utf-8')
            except UnicodeDecodeError:pass
    return out
if len(sys.argv)<3 or sys.argv[1]!='exec':raise SystemExit('usage: work exec COMMAND [ARG...]')
id=uuid.uuid4().hex;start=time.time_ns();before=snapshot()
data=sys.stdin.buffer.read()
r=subprocess.run(sys.argv[2:],cwd=root,input=data,capture_output=True,timeout=300)
after=snapshot()
for name,content in [('stdin',data),('stdout',r.stdout),('stderr',r.stderr)]:
    (audit/(id+'.'+name)).write_bytes(content)
record={'id':id,'argv':sys.argv[2:],'started_ns':start,'finished_ns':time.time_ns(),'exit_code':r.returncode,'before':before,'after':after}
with (audit/'events.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(record,ensure_ascii=False)+'\\n')
sys.stdout.buffer.write(r.stdout);sys.stderr.buffer.write(r.stderr);raise SystemExit(r.returncode)
'''.replace('ROOT', repr(str(root))).replace('PRIVATE', repr(str(private)))
    script.write_text('#!' + sys.executable + '\n' + code, encoding='utf-8')
    script.chmod(0o755)
    return script


def prepare(amh, suite, out):
    c = qe.prepare(amh, suite, out)
    for item in c['cases'].values():
        root, private = Path(item['workspace']), Path(item['private'])
        # Optional extra fixture files are legitimate project state, not hints
        # about the methods or responses that should be selected.
        for name in item['case'].get('extra_fixtures', []):
            shutil.copytree(qe.REPO / 'evals/fixtures' / name, root, dirs_exist_ok=True)
        for task in item['case'].get('saved_tasks', []):
            qe.write_json(root / 'saved-tasks' / (task['id'] + '.json'), task)
        if item['case'].get('saved_tasks'):
            with (root / 'README.md').open('a', encoding='utf-8') as f:
                f.write('\nRecorded work requests are in saved-tasks/. Their existence does not select the current request.\n')
        initial = qe.read_json(private / 'initial.json')
        initial['files'] = qe.product_files(root)
        initial['user_messages'] = item['case']['turns']
        qe.write_json(private / 'initial.json', initial)
        trace = make_trace(Path(item['tool']).parent, root, private)
        prompt = Path(item['actor_prompt'])
        prompt.write_text(prompt.read_text() + '\n\nEnvironment observation: use ' + str(trace) + ' exec COMMAND [ARG...] for workspace reads, edits and checks; stdin is supported. Use the supplied AMH directly for its calls. Do not inspect the wrapper source or observation storage. This records observable actions; it does not specify which methods to use. Wait after handling this user request; further requests may follow in the same conversation.', encoding='utf-8')
    qe.write_json(Path(out) / 'controller.json', c)
    return c


def seal(folder):
    qe.write_json(folder / 'evidence-manifest.json', {p.relative_to(folder).as_posix(): qe.sha(p.read_bytes()) for p in sorted(folder.rglob('*')) if p.is_file() and p != folder / 'evidence-manifest.json'})


def collect_turn(run, case, turn, actor_note=None):
    run = Path(run).resolve()
    item = qe.read_json(run / 'controller.json')['cases'][case]
    private = Path(item['private'])
    if not 1 <= turn <= len(item['case']['turns']): raise ValueError('turn index out of range')
    if turn > 1 and not (private / 'turns' / str(turn-1) / 'observation.json').exists(): raise ValueError('freeze prior turn first')
    destination = private / 'turns' / str(turn)
    if destination.exists(): raise FileExistsError('turn already frozen')
    observation = qe.collect(run, case, actor_note)
    observation['user_message'] = item['case']['turns'][turn-1]
    observation['turn'] = turn
    observation['work_trace_scope'] = 'Commands voluntarily passed through work; other session events are not fully observed or sandboxed.'
    qe.write_json(private / 'result/observation.json', observation)
    if (private / 'work-audit').exists(): shutil.copytree(private / 'work-audit', private / 'result/work-audit')
    seal(private / 'result')
    destination.parent.mkdir(exist_ok=True)
    (private / 'result').rename(destination)
    return observation


def finalize(run, case):
    run = Path(run).resolve()
    item = qe.read_json(run / 'controller.json')['cases'][case]
    private = Path(item['private'])
    count = len(item['case']['turns'])
    for i in range(1, count+1):
        if not (private / 'turns' / str(i) / 'observation.json').is_file(): raise ValueError('all turns must be frozen')
    shutil.copytree(private / 'turns' / str(count), private / 'result')
    shutil.copytree(private / 'turns', private / 'result/turns')
    event = private / 'controller-event.json'
    if event.exists(): shutil.copyfile(event, private / 'result/controller-event.json')
    seal(private / 'result')
    return private / 'result'

def export(run, out):
    run, out = Path(run).resolve(), Path(out).resolve()
    provenance = qe.export_evidence(run, out)
    controller = qe.read_json(run / 'controller.json')
    def portable(value):
        if isinstance(value, str): return value.replace(str(run), '{RUN_ROOT}').replace(str(qe.REPO), '{REPOSITORY}')
        if isinstance(value, list): return [portable(v) for v in value]
        if isinstance(value, dict): return {k: portable(v) for k,v in value.items()}
        return value
    for case, item in controller['cases'].items():
        private, folder = Path(item['private']), out / case
        qe.write_json(folder / 'case.json', portable(item['case']))
        for turn in sorted((private / 'turns').iterdir(), key=lambda p: int(p.name)):
            target = folder / 'turns' / turn.name
            qe.write_json(target / 'observation.json', portable(qe.read_json(turn / 'observation.json')))
            workspace = turn / 'workspace'
            inventory = {p.relative_to(workspace).as_posix(): {'sha256': qe.sha(p.read_bytes()), 'bytes': p.stat().st_size} for p in sorted(workspace.rglob('*')) if p.is_file() and '__pycache__' not in p.parts}
            qe.write_json(target / 'file-inventory.json', inventory)
        audit = private / 'result/work-audit'
        if (audit / 'events.jsonl').exists():
            for line in (audit / 'events.jsonl').read_text().splitlines():
                record = __import__('json').loads(line)
                for channel in ['stdin','stdout','stderr']:
                    record[channel] = (audit / (record['id'] + '.' + channel)).read_text(encoding='utf-8', errors='replace')
                qe.write_json(folder / 'work' / (record['id'] + '.json'), portable(record))
        for name in ['controller-event.json', 'controller-correction.json']:
            if (private / name).exists(): qe.write_json(folder / name, portable(qe.read_json(private / name)))
    manifest = {p.relative_to(out).as_posix(): qe.sha(p.read_bytes()) for p in sorted(out.rglob('*')) if p.is_file() and p != out / 'manifest.json'}
    qe.write_json(out / 'manifest.json', manifest)
    return provenance


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__);s=p.add_subparsers(dest='command',required=True)
    a=s.add_parser('prepare');a.add_argument('--amh',required=True);a.add_argument('--suite',required=True);a.add_argument('--out',required=True)
    a=s.add_parser('collect');a.add_argument('--run',required=True);a.add_argument('--case',required=True);a.add_argument('--turn',type=int,required=True);a.add_argument('--actor-note')
    a=s.add_parser('export');a.add_argument('--run',required=True);a.add_argument('--out',required=True)
    a=s.add_parser('finalize');a.add_argument('--run',required=True);a.add_argument('--case',required=True)
    a=p.parse_args()
    if a.command=='prepare':
        c=prepare(a.amh,a.suite,a.out);print(json.dumps({k:v['actor_prompt'] for k,v in c['cases'].items()}))
    elif a.command=='collect':
        o=collect_turn(a.run,a.case,a.turn,a.actor_note);print(json.dumps({'turn':o['turn'],'calls':len(o['amh_calls']),'semantic_verdict':None}))
    elif a.command=='export':print(__import__('json').dumps(export(a.run,a.out)))
    else:print(finalize(a.run,a.case))
