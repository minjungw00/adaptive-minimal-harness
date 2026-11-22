#!/usr/bin/env python3
"""Developer-only fresh-session evaluation support; no model runner or semantic scorer."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time
import uuid

REPO = Path(__file__).resolve().parents[1]
DEFAULT_SUITE = REPO / 'evals/scenarios.json'


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load_suite(path):
    suite = read_json(path)
    if suite.get('format') != 'amh.qualitative-suite.v1':
        raise ValueError('unsupported suite format')
    seen = set()
    for case in suite['cases']:
        if case['id'] in seen or not case['request'].strip():
            raise ValueError('duplicate case ID or empty request')
        seen.add(case['id'])
        fixture = (REPO / 'evals/fixtures' / case['fixture']).resolve()
        if not fixture.is_relative_to((REPO / 'evals/fixtures').resolve()) or not fixture.is_dir():
            raise ValueError('invalid fixture path')
        # No exact selected-set oracle is part of this suite.
        if any(k.startswith('expected_') for k in case):
            raise ValueError('qualitative cases must not contain a fixed expected-answer set')
    return suite


def invoke(amh, root, home, *args):
    result = subprocess.run([str(amh), *args], cwd=root, env={**os.environ, "AMH_HOME": str(home)},
                            capture_output=True, text=True, encoding="utf-8")
    if result.returncode:
        raise RuntimeError(result.stderr)
    return json.loads(result.stdout)


def product_files(root):
    result = {}
    for path in sorted(Path(root).rglob('*')):
        if not path.is_file() or path.is_symlink() or '__pycache__' in path.parts:
            continue
        relative = path.relative_to(root).as_posix()
        if relative.startswith('.git/') or relative == '.git':
            continue
        data = path.read_bytes()
        result[relative] = {'sha256': sha(data), 'bytes': len(data)}
        if len(data) <= 256 * 1024:
            try:
                result[relative]['text'] = data.decode('utf-8')
            except UnicodeDecodeError:
                pass
    return result


def seed(amh, root, case, scratch, home):
    subprocess.run(['git', 'init', '-q'], cwd=root, check=True)
    invoke(amh, root, home, 'project', 'add', '--id', 'workspace', '--apply')
    for pack in case.get('packs', []):
        invoke(amh, root, home, 'pack', 'install', pack, '--reason', 'Install selected project methods', '--apply')
    policies = list(case.get('policies', []))
    for i, goal in enumerate(case.get('goals', [])):
        policies.append({'owner': 'user', 'source': 'Accepted project goal', 'priority': 0,
                         'item': {'id': f'G-{i+1}', 'kind': 'goal', 'scope': 'project', 'revision': 1, 'lifecycle': 'draft', 'content': goal,
                                  'details': {'type': 'goal', 'success_criteria': [goal]}}})
    for policy in policies:
        source = scratch / 'policy.json'
        write_json(source, policy)
        invoke(amh, root, home, 'policy', 'add', '--file', str(source), '--reason', 'Register accepted project condition', '--apply')
    if task := case.get('current_task'):
        write_json(root / 'saved-tasks' / (task['id'] + '.json'), task)
    for id in case.get('pause', []):
        invoke(amh, root, home, 'policy', 'change', id, '--pause', '--reason', 'Pause a user-selected method', '--apply')


def make_tool(directory, amh, audit, home):
    if os.name != 'posix':
        raise ValueError('fresh-session audit wrapper currently requires a POSIX host')
    directory.mkdir(parents=True)
    audit.mkdir(parents=True)
    script = directory / 'amh'
    script.write_text(f'''#!{sys.executable}
import hashlib,json,os,pathlib,subprocess,sys,time,uuid
real={str(amh)!r}
audit=pathlib.Path({str(audit)!r})
id=uuid.uuid4().hex
started=time.time_ns()
args=sys.argv[1:]
reads_selection=args[:2]==['policy','read'] and '--selection' in args
reads_plan=args[:1]==['apply'] and (args[1:]==['--plan','-'] or args[1:]==['--plan=-'])
stdin_data=sys.stdin.buffer.read() if reads_selection or reads_plan else None
r=subprocess.run([real,*args],input=stdin_data,capture_output=True,timeout=300,env={{**os.environ,'AMH_HOME':{str(home)!r}}})
(audit/(id+'.stdout')).write_bytes(r.stdout)
(audit/(id+'.stderr')).write_bytes(r.stderr)
record={{'id':id,'binary_sha256':hashlib.sha256(pathlib.Path(real).read_bytes()).hexdigest(),'argv':args,'cwd':os.getcwd(),'started_ns':started,'finished_ns':time.time_ns(),'exit_code':r.returncode}}
if stdin_data is not None:
    (audit/(id+'.stdin')).write_bytes(stdin_data)
    record['stdin_file']=id+'.stdin'
with (audit/'calls.jsonl').open('a',encoding='utf-8') as f:f.write(json.dumps(record,ensure_ascii=False)+'\\n')
sys.stdout.buffer.write(r.stdout);sys.stderr.buffer.write(r.stderr)
raise SystemExit(r.returncode)
''', encoding='utf-8')
    script.chmod(0o755)
    return script


def prepare(amh, suite_path, out, case_ids=None):
    amh = Path(amh).resolve()
    if not amh.is_file():
        raise ValueError('build the native AMH executable first')
    suite = load_suite(suite_path)
    selected = [c for c in suite['cases'] if case_ids is None or c['id'] in case_ids]
    if case_ids is not None and set(case_ids) != {c['id'] for c in selected}:
        raise ValueError('unknown case ID')
    out = Path(out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    controller = {'format': 'amh.qualitative-run.v1', 'suite_sha256': sha(Path(suite_path).read_bytes()),
                  'amh_sha256': sha(amh.read_bytes()), 'created_ns': time.time_ns(), 'cases': {}}
    for case in selected:
        opaque = uuid.uuid4().hex[:12]
        directory = out / 'actors' / opaque
        root = directory / 'workspace'
        shutil.copytree(REPO / 'evals/fixtures' / case['fixture'], root)
        private = out / 'controller' / case['id']
        private.mkdir(parents=True)
        home = private / 'amh-home'
        seed(amh, root, case, private, home)
        tool = make_tool(directory / 'bin', amh, private / 'audit', home)
        guidance = subprocess.check_output([str(amh), 'agent', 'instructions'], text=True, encoding='utf-8')
        initial = {'request': case['request'], 'files': product_files(root), 'policies': invoke(amh, root, home, 'policy', 'list', '--full'),
                   'agent_instructions': guidance, 'instruction_source': 'simulated_user_global_settings'}
        write_json(private / 'initial.json', initial)
        write_json(directory / 'assignment.json', {'request': case['request'], 'workspace': str(root), 'amh': str(tool)})
        prompt = (f'Work only in {root}. AMH is available at {tool}; put its containing directory on PATH if needed. '
                  'Standard local runtimes are available. Do not use network services, other agents, sibling directories, repository sources or unrelated connected tools. '
                  'Handle the following user request. If essential information is missing, ask the user instead of inventing it. '
                  'When finished, state the work done, checks actually executed, and any unresolved questions.\n\n'
                  + 'User-global agent instructions:\n' + guidance + '\nUser request:\n' + case['request'])
        (directory / 'actor-prompt.txt').write_text(prompt, encoding='utf-8')
        controller['cases'][case['id']] = {'opaque_id': opaque, 'case': case, 'workspace': str(root), 'tool': str(tool),
                                         'private': str(private), 'actor_prompt': str(directory / 'actor-prompt.txt')}
    write_json(out / 'controller.json', controller)
    return controller


def checks(root, commands):
    results = []
    for command in commands:
        argv = [sys.executable if a == '{python}' else a for a in command]
        try:
            r = subprocess.run(argv, cwd=root, capture_output=True, text=True, encoding='utf-8', timeout=30)
            results.append({'argv': command, 'exit_code': r.returncode, 'stdout': r.stdout, 'stderr': r.stderr})
        except subprocess.TimeoutExpired:
            results.append({'argv': command, 'status': 'timed_out'})
    return results


def collect(run, id, actor_note=None):
    run = Path(run).resolve()
    controller = read_json(run / 'controller.json')
    item = controller['cases'][id]
    root = Path(item['workspace'])
    private = Path(item['private'])
    result_dir = private / 'result'
    result_dir.mkdir(exist_ok=False)
    frozen = result_dir / 'workspace'
    def omit_links(directory, names):
        return [n for n in names if n in ('__pycache__', '.git') or (Path(directory) / n).is_symlink()]
    omitted_links = [p.relative_to(root).as_posix() for p in root.rglob('*') if p.is_symlink()]
    shutil.copytree(root, frozen, ignore=omit_links)
    audit = private / 'audit'
    calls = [json.loads(line) for line in (audit / 'calls.jsonl').read_text(encoding='utf-8').splitlines()] if (audit / 'calls.jsonl').exists() else []
    for call in calls:
        call['stdout_file'] = 'audit/' + call['id'] + '.stdout'
        call['stderr_file'] = 'audit/' + call['id'] + '.stderr'
    shutil.copytree(audit, result_dir / 'audit')
    initial = read_json(private / 'initial.json')
    after = product_files(frozen)
    differences = [{'path': p, 'before': initial['files'].get(p, {}).get('sha256'), 'after': after.get(p, {}).get('sha256')}
                   for p in sorted(set(initial['files']) | set(after)) if initial['files'].get(p, {}).get('sha256') != after.get(p, {}).get('sha256')]
    record = {'format': 'amh.qualitative-observation.v1', 'request': item['case']['request'], 'files': after,
              'file_changes': differences, 'amh_calls': calls,
              'actor_note': Path(actor_note).read_text(encoding='utf-8') if actor_note else None,
              'actor_note_authority': 'self_report_not_execution_proof',
              'independent_outcome_checks': checks(frozen, item['case'].get('checks', [])),
              'omitted_symlinks': omitted_links, 'semantic_verdict': None}
    write_json(result_dir / 'observation.json', record)
    manifest = {p.relative_to(result_dir).as_posix(): sha(p.read_bytes()) for p in sorted(result_dir.rglob('*')) if p.is_file() and not p.is_symlink() and '__pycache__' not in p.parts}
    write_json(result_dir / 'evidence-manifest.json', manifest)
    return record


STAGE_ONE = '''You are an independent qualitative reviewer. You have no expected policy list or prior conversation. Read only the supplied initial context. Infer the user's goal, applicable conditions, legitimate alternatives and information gaps. Do not assume that every named method must be used, or that more methods is better. Distinguish mandatory conditions from flexible approaches. Write provisional notes to PROVISIONAL. Do not inspect actor results or other reviewers. Do not edit the workspace or invoke other agents. Your initial view is a revisable hypothesis, not an answer key. Return only the notes path.'''
STAGE_TWO = '''Now read the actor observation and recorded AMH calls. Raw command outputs are under actor/audit as CALL_ID.stdout and CALL_ID.stderr (also linked from newer observation records). For multi-turn cases inspect actor/turns for intermediate snapshots and actor/work-audit/events.jsonl plus its stdout/stderr for observed commands/edits. A controller-event file, if present, identifies controller-originated policy changes. Work traces observe only commands sent through the recorder, not the entire agent session. Inspect relevant evidence in the frozen workspace if needed. Evaluate the appropriateness of chosen AND omitted principles, joint application, unnecessary procedure, preservation of requirements, actual outcomes and uncertainty. Accept justified alternatives and revise your provisional view when evidence supports it. Use no exact-set scoring or method-count target. Tie material findings to the request, policy condition, source state or observed result. Execution records, actor self-reports and unverified claims are different evidence. Write a qualitative review to REVIEW with: understanding, strengths, findings (observation/evidence/impact/alternative), defensible_alternatives, uncertainty, and limits. Do not edit actor evidence, inspect other reviewers or launch agents. Return only the review path.'''


def reviewer(run, id, label):
    run = Path(run).resolve()
    if label not in ('A', 'B'):
        raise ValueError('review label must be A or B')
    routing_file = run / 'review-routing.json'
    routing = read_json(routing_file) if routing_file.exists() else {}
    if id + label in routing:
        raise FileExistsError('review assignment already exists')
    item = read_json(run / 'controller.json')['cases'][id]
    directory = run / 'reviewers' / uuid.uuid4().hex[:12]
    directory.mkdir(parents=True)
    initial = directory / 'initial.json'
    shutil.copyfile(Path(item['private']) / 'initial.json', initial)
    provisional = directory / 'provisional.md'
    prompt = STAGE_ONE.replace('PROVISIONAL', str(provisional)) + '\nInitial context: ' + str(initial)
    (directory / 'stage-one.txt').write_text(prompt, encoding='utf-8')
    write_json(directory / 'routing.json', {'case_id': id, 'label': label, 'private': item['private']})
    routing[id + label] = {'directory': str(directory)}
    write_json(routing_file, routing)
    return {'directory': str(directory), 'prompt': prompt}


def reveal(directory):
    directory = Path(directory).resolve()
    if (directory / 'actor').exists() or (directory / 'stage-two.txt').exists():
        raise FileExistsError('actor evidence has already been revealed')
    if not (directory / 'provisional.md').is_file() or not (directory / 'provisional.md').read_text(encoding='utf-8').strip():
        raise ValueError('freeze provisional reasoning before revealing the actor')
    routing = read_json(directory / 'routing.json')
    routing['provisional_sha256'] = sha((directory / 'provisional.md').read_bytes())
    routing['initial_sha256'] = sha((directory / 'initial.json').read_bytes())
    write_json(directory / 'routing.json', routing)
    source = Path(routing['private']) / 'result'
    target = directory / 'actor'
    shutil.copytree(source, target)
    review = directory / 'review.json'
    prompt = STAGE_TWO.replace('REVIEW', str(review)) + '\nActor observation: ' + str(target / 'observation.json') + '\nFrozen workspace: ' + str(target / 'workspace')
    (directory / 'stage-two.txt').write_text(prompt, encoding='utf-8')
    return prompt


def export_evidence(run, out):
    """Archive observations/reviews, never infer a semantic pass/fail label."""
    run = Path(run).resolve()
    out = Path(out).resolve()
    controller = read_json(run / 'controller.json')
    routing = read_json(run / 'review-routing.json')
    prepared = []
    for id, item in controller['cases'].items():
        private = Path(item['private'])
        observation = read_json(private / 'result/observation.json')
        reviews = []
        for label in ['A', 'B']:
            directory = Path(routing[id + label]['directory'])
            review = read_json(directory / 'review.json')
            provisional = (directory / 'provisional.md').read_text(encoding='utf-8')
            if not provisional.strip():
                raise ValueError('missing independent provisional review')
            route = read_json(directory / 'routing.json')
            if route.get('provisional_sha256') and route['provisional_sha256'] != sha((directory / 'provisional.md').read_bytes()):
                raise ValueError('provisional review changed after reveal')
            if route.get('initial_sha256') and route['initial_sha256'] != sha((directory / 'initial.json').read_bytes()):
                raise ValueError('initial review context changed after reveal')
            reviews.append({'label': label, 'provisional': provisional, 'review': review,
                            'stage_one_prompt': (directory / 'stage-one.txt').read_text(encoding='utf-8'),
                            'stage_two_prompt': (directory / 'stage-two.txt').read_text(encoding='utf-8'),
                            'provisional_frozen_before_reveal_hash': route.get('provisional_sha256'),
                            'phase_file_times_ns': {'provisional': (directory / 'provisional.md').stat().st_mtime_ns,
                                                   'reveal': (directory / 'stage-two.txt').stat().st_mtime_ns}})
        raw = []
        for call in observation['amh_calls']:
            a = private / 'result/audit'
            raw.append({**call, 'stdout': (a / (call['id'] + '.stdout')).read_text(encoding='utf-8'),
                        'stderr': (a / (call['id'] + '.stderr')).read_text(encoding='utf-8'),
                        'stdin': (a / call['stdin_file']).read_text(encoding='utf-8') if call.get('stdin_file') else None})
        prepared.append((id, {'initial': read_json(private / 'initial.json'), 'observation': observation,
                              'calls': raw,
                              'reviews': reviews}))
    out.mkdir(parents=True, exist_ok=False)
    def portable(value):
        if isinstance(value, str):
            return value.replace(str(run), '{RUN_ROOT}').replace(str(REPO), '{REPOSITORY}')
        if isinstance(value, list): return [portable(v) for v in value]
        if isinstance(value, dict): return {k: portable(v) for k, v in value.items()}
        return value
    for id, data in prepared:
        folder = out / id
        write_json(folder / 'initial.json', portable(data['initial']))
        write_json(folder / 'observation.json', portable(data['observation']))
        write_json(folder / 'calls.json', portable(data['calls']))
        for review in data['reviews']:
            write_json(folder / ('review-' + review['label'] + '.json'), portable(review))
    # Session isolation/model facts come from the controller, not from this file
    # exporter: it cannot observe how a person launched an external agent.
    session_file = run / 'session-provenance.json'
    session = read_json(session_file) if session_file.exists() else {}
    provenance = {'format': 'amh.qualitative-archive.v1', 'suite_sha256': controller['suite_sha256'],
                  'amh_sha256': controller['amh_sha256'], 'created_ns': controller['created_ns'],
                  'executed_cases': [id for id, _ in prepared], 'actors': len(prepared),
                  'review_records': 2 * len(prepared), 'session_provenance': session,
                  'automatic_semantic_score': None,
                  'archive_notes': ['Paths normalized for portability; original fingerprints refer to original run bytes, not normalized replay inputs.',
                                    'Actor notes are self-reports; their capture method must be recorded by the controller.',
                                    'AMH command output is captured; arbitrary actor shell/read/edit events are not fully audited.',
                                    'Actor/reviewer access restrictions are instructional on a shared filesystem, not an OS sandbox.',
                                    'A missing pre-reveal hash is retained as null, not reconstructed after review.']}
    write_json(out / 'provenance.json', provenance)
    manifest = {p.relative_to(out).as_posix(): sha(p.read_bytes()) for p in sorted(out.rglob('*')) if p.is_file()}
    write_json(out / 'manifest.json', manifest)
    return provenance


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest='command', required=True)
    a = sub.add_parser('prepare'); a.add_argument('--amh', required=True); a.add_argument('--suite', default=str(DEFAULT_SUITE)); a.add_argument('--out', required=True); a.add_argument('--cases', nargs='+')
    a = sub.add_parser('collect'); a.add_argument('--run', required=True); a.add_argument('--case', required=True); a.add_argument('--actor-note')
    a = sub.add_parser('reviewer'); a.add_argument('--run', required=True); a.add_argument('--case', required=True); a.add_argument('--label', required=True)
    a = sub.add_parser('reveal'); a.add_argument('--directory', required=True)
    a = sub.add_parser('export'); a.add_argument('--run', required=True); a.add_argument('--out', required=True)
    args = p.parse_args()
    if args.command == 'prepare':
        c = prepare(args.amh, args.suite, args.out, args.cases)
        print(json.dumps({'run': str(Path(args.out).resolve()), 'assignments': {k:v['actor_prompt'] for k,v in c['cases'].items()}}, ensure_ascii=False))
    elif args.command == 'collect':
        result = collect(args.run, args.case, args.actor_note); print(json.dumps({'case': args.case, 'amh_calls': len(result['amh_calls']), 'semantic_verdict': None}))
    elif args.command == 'reviewer':
        print(json.dumps(reviewer(args.run, args.case, args.label), ensure_ascii=False))
    elif args.command == 'reveal':
        print(reveal(args.directory))
    else:
        print(json.dumps(export_evidence(args.run, args.out), ensure_ascii=False))


if __name__ == '__main__':
    main()
