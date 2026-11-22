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
import tempfile
import time
import uuid

REPO = Path(__file__).resolve().parents[1]
DEFAULT_SUITE = REPO / 'evals/scenarios.json'
MANIFEST = 'evidence-manifest.json'
FIXTURE_CONTRACTS = {'discount': 'calculator', 'refactor': 'formatter'}


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8'))


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')


def sha(data):
    return hashlib.sha256(data).hexdigest()


def fingerprints(folder):
    return {p.relative_to(folder).as_posix(): sha(p.read_bytes())
            for p in sorted(folder.rglob('*')) if p.is_file() and not p.is_symlink()
            and p != folder / MANIFEST}


def seal(folder):
    write_json(folder / MANIFEST, fingerprints(folder))


def verify(folder):
    manifest = folder / MANIFEST
    if (not manifest.is_file() or any(p.is_symlink() for p in folder.rglob('*'))
            or read_json(manifest) != fingerprints(folder)):
        raise ValueError('actor evidence changed or was never sealed: ' + str(folder))
    return sha(manifest.read_bytes())


def portable(value, run):
    if isinstance(value, str):
        return value.replace(str(run), '{RUN_ROOT}').replace(str(REPO), '{REPOSITORY}')
    if isinstance(value, list):
        return [portable(v, run) for v in value]
    if isinstance(value, dict):
        return {k: portable(v, run) for k, v in value.items()}
    return value


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
        if 'contract' in case and (case['fixture'] not in FIXTURE_CONTRACTS
                                   or case['contract'] != FIXTURE_CONTRACTS[case['fixture']]):
            raise ValueError('unknown contract for fixture')
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
    for policy in case.get('policies', []):
        source = scratch / 'policy.json'
        write_json(source, policy)
        invoke(amh, root, home, 'policy', 'add', '--file', str(source), '--scope', policy['scope'],
               '--reason', 'Register reusable guidance', '--apply')
    if context := case.get('work_context'):
        (root / 'WORK.md').write_text('# Accepted work context\n\n' + ''.join(f'- {line}\n' for line in context), encoding='utf-8')
        with (root / 'README.md').open('a', encoding='utf-8') as readme:
            readme.write('\nAccepted work context is available in WORK.md.\n')
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


def make_trace(directory, root, private):
    script = directory / 'work'
    code = f'''import hashlib
import json
from pathlib import Path
import subprocess
import sys
import time
import uuid

root = Path({str(root)!r})
audit = Path({str(private)!r}) / 'work-audit'
''' + '''
audit.mkdir(exist_ok=True)


def snapshot():
    files = {}
    for path in sorted(root.rglob('*')):
        if not path.is_file() or path.is_symlink() or '__pycache__' in path.parts:
            continue
        relative = path.relative_to(root)
        if relative.parts[0] == '.git':
            continue
        data = path.read_bytes()
        entry = {'sha256': hashlib.sha256(data).hexdigest()}
        if len(data) <= 262144:
            try:
                entry['text'] = data.decode('utf-8')
            except UnicodeDecodeError:
                pass
        files[relative.as_posix()] = entry
    return files


if len(sys.argv) < 3 or sys.argv[1] != 'exec':
    raise SystemExit('usage: work exec COMMAND [ARG...]')

event_id = uuid.uuid4().hex
started = time.time_ns()
before = snapshot()
data = sys.stdin.buffer.read()
result = subprocess.run(sys.argv[2:], cwd=root, input=data, capture_output=True, timeout=300)
after = snapshot()
for name, content in [('stdin', data), ('stdout', result.stdout), ('stderr', result.stderr)]:
    (audit / (event_id + '.' + name)).write_bytes(content)
record = {'id': event_id, 'argv': sys.argv[2:], 'started_ns': started, 'finished_ns': time.time_ns(),
          'exit_code': result.returncode, 'before': before, 'after': after}
with (audit / 'events.jsonl').open('a', encoding='utf-8') as log:
    log.write(json.dumps(record, ensure_ascii=False) + '\\n')
sys.stdout.buffer.write(result.stdout)
sys.stderr.buffer.write(result.stderr)
raise SystemExit(result.returncode)
'''
    script.write_text('#!' + sys.executable + '\n' + code, encoding='utf-8')
    script.chmod(0o755)
    return script


def static_guidance(policies):
    parts = ['# Workspace policies', 'Apply each policy within its stated conditions.']
    for policy in policies:
        if policy['lifecycle'] != 'active':
            continue
        parts.extend([f"## {policy['id']} ({policy['strength']})",
                      policy.get('applicability') or 'Applies throughout the work.',
                      policy['content']])
    return '\n\n'.join(parts) + '\n'


def prepare(amh, suite_path, out, case_ids=None, *, delivery='amh'):
    amh = Path(amh).resolve()
    if not amh.is_file():
        raise ValueError('build the native AMH executable first')
    suite = load_suite(suite_path)
    selected = [c for c in suite['cases'] if case_ids is None or c['id'] in case_ids]
    if case_ids is not None and set(case_ids) != {c['id'] for c in selected}:
        raise ValueError('unknown case ID')
    if delivery not in ('amh', 'agents-md'):
        raise ValueError('unknown policy delivery')
    out = Path(out).resolve()
    out.mkdir(parents=True, exist_ok=False)
    controller = {'format': 'amh.qualitative-run.v1', 'suite_sha256': sha(Path(suite_path).read_bytes()),
                  'amh_sha256': sha(amh.read_bytes()), 'amh': str(amh), 'delivery': delivery,
                  'created_ns': time.time_ns(), 'cases': {}}
    for case in selected:
        opaque = uuid.uuid4().hex[:12]
        directory = out / 'actors' / opaque
        root = directory / 'workspace'
        shutil.copytree(REPO / 'evals/fixtures' / case['fixture'], root)
        private = out / 'controller' / case['id']
        private.mkdir(parents=True)
        home = private / 'amh-home'
        seed(amh, root, case, private, home)
        policies = invoke(amh, root, home, 'policy', 'list', '--full')
        if delivery == 'amh':
            tool = make_tool(directory / 'bin', amh, private / 'audit', home)
            guidance = subprocess.check_output([str(amh), 'agent', 'instructions'], text=True, encoding='utf-8')
            instruction_source = 'simulated_user_global_settings'
            access = f'AMH is available at {tool}; put its containing directory on PATH if needed. '
            instructions = 'User-global agent instructions:\n' + guidance
        else:
            tool = None
            guidance = static_guidance(policies['policies'])
            (root / 'AGENTS.md').write_text(guidance, encoding='utf-8')
            instruction_source = 'project_agents_md'
            access = 'Project guidance is available at AGENTS.md. Read that file before working. '
            instructions = ''
            (private / 'audit').mkdir()
            (directory / 'bin').mkdir()
        initial = {'request': case['request'], 'files': product_files(root), 'policies': policies,
                   'agent_instructions': guidance, 'instruction_source': instruction_source}
        write_json(private / 'initial.json', initial)
        assignment = {'request': case['request'], 'workspace': str(root)}
        if tool:
            assignment['amh'] = str(tool)
        write_json(directory / 'assignment.json', assignment)
        trace = make_trace(directory / 'bin', root, private)
        prompt = (f'Work only in {root}. ' + access
                  + f'Use {trace} exec COMMAND [ARG...] for workspace reads, edits and checks. Use the AMH command directly if supplied. '
                  + 'Standard local runtimes are available. Do not use network services, other agents, sibling directories, repository sources or unrelated connected tools. '
                  'Handle the following user request. When finished, state the work done, checks actually executed, and any unresolved questions.\n\n'
                  + instructions + '\nUser request:\n' + case['request'])
        (directory / 'actor-prompt.txt').write_text(prompt, encoding='utf-8')
        controller['cases'][case['id']] = {'opaque_id': opaque, 'case': case, 'workspace': str(root), 'tool': assignment.get('amh'),
                                         'private': str(private), 'actor_prompt': str(directory / 'actor-prompt.txt')}
        if 'contract' in case:
            verifier = (REPO / 'evals/checks/contracts.py').read_bytes()
            (private / 'contracts.py').write_bytes(verifier)
            controller['cases'][case['id']]['verifier_sha256'] = sha(verifier)
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


def evidence_paths(item, turn):
    private = Path(item['private'])
    turns = item['case'].get('turns')
    if turns is not None:
        if turn is None or not 1 <= turn <= len(turns):
            raise ValueError('give a valid turn for this dialogue case')
        return private / 'turn-inputs' / f'{turn}.json', private / 'turns' / str(turn)
    if turn is not None:
        raise ValueError('single-request cases have no turn number')
    return private / 'initial.json', private / 'result'


def collect(run, id, actor_note=None, *, turn=None):
    run = Path(run).resolve()
    item = read_json(run / 'controller.json')['cases'][id]
    initial_path, destination = evidence_paths(item, turn)
    if turn is not None and not initial_path.is_file():
        raise ValueError('start the turn before collecting it')
    if destination.exists():
        raise FileExistsError('evidence already collected: ' + str(destination))
    destination.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(dir=destination.parent) as staging:
        result_dir = Path(staging) / 'snapshot'
        record = capture(item, result_dir, initial_path, actor_note, turn)
        result_dir.rename(destination)
    return record


def capture(item, result_dir, initial_path, actor_note, turn):
    root = Path(item['workspace'])
    private = Path(item['private'])
    initial = read_json(initial_path)
    if contract := item['case'].get('contract'):
        verifier = private / 'contracts.py'
        verifier_digest = sha(verifier.read_bytes())
        if verifier_digest != item['verifier_sha256']:
            raise ValueError('independent verifier changed after preparing the run')
    result_dir.mkdir(parents=True, exist_ok=False)
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
    shutil.copyfile(initial_path, result_dir / 'initial.json')
    after = product_files(frozen)
    differences = [{'path': p, 'before': initial['files'].get(p, {}).get('sha256'), 'after': after.get(p, {}).get('sha256')}
                   for p in sorted(set(initial['files']) | set(after)) if initial['files'].get(p, {}).get('sha256') != after.get(p, {}).get('sha256')]
    with tempfile.TemporaryDirectory(dir=private) as scratch:
        # Each check gets its own copy so its writes cannot alter evidence or another check.
        checked = Path(scratch) / 'workspace'
        workspace_checks = []
        outcome_checks = []
        if commands := item['case'].get('checks'):
            shutil.copytree(frozen, checked)
            workspace_checks = checks(checked, commands)
            shutil.rmtree(checked)
        if contract:
            shutil.copytree(frozen, checked)
            outcome_checks = checks(checked, [[sys.executable, '-I', str(verifier), contract, str(checked)]])
            outcome_checks[0]['verifier_sha256'] = verifier_digest
    record = {'format': 'amh.qualitative-observation.v1', 'request': initial['request'], 'files': after,
              'file_changes': differences, 'amh_calls': calls,
              'actor_note': Path(actor_note).read_text(encoding='utf-8') if actor_note else None,
              'actor_note_authority': 'self_report_not_execution_proof',
              'workspace_checks': workspace_checks, 'independent_outcome_checks': outcome_checks,
              'omitted_symlinks': omitted_links, 'semantic_verdict': None}
    write_json(result_dir / 'observation.json', record)
    if (private / 'work-audit').exists():
        shutil.copytree(private / 'work-audit', result_dir / 'work-audit')
    if turn is not None:
        intervention = initial_path.with_name(f'{turn}-change.json')
        if str(turn) in item['case'].get('pause_before_turn', {}) and not intervention.is_file():
            raise ValueError('scheduled policy change has no record')
        if intervention.exists():
            shutil.copyfile(intervention, result_dir / 'policy-change.json')
    seal(result_dir)
    return record


STAGE_ONE = '''You are an independent qualitative reviewer. You have no expected policy list or prior conversation. Read only the supplied initial context. Infer the user's goal, applicable conditions, legitimate alternatives and information gaps. Do not assume that every named method must be used, or that more methods is better. Distinguish mandatory conditions from flexible approaches. Write provisional notes to PROVISIONAL. Do not inspect actor results or other reviewers. Do not edit the workspace or invoke other agents. Your initial view is a revisable hypothesis, not an answer key. Return only the notes path.'''
STAGE_TWO = '''Now read the actor observation, AMH calls under actor/audit and work events under actor/work-audit when present. For a dialogue, assess the current turn using the supplied history and the evidence through this turn. policy-change.json records any controller action for this turn. Work traces observe only commands sent through the recorder. Workspace checks use the actor's tests, while independent checks use evaluator-owned tests. A final test pass does not establish the order of work. Inspect the frozen workspace if needed. Evaluate the appropriateness of chosen and omitted principles, joint application, unnecessary procedure, preservation of requirements, actual outcomes and uncertainty. Accept justified alternatives and revise your provisional view when evidence supports it. Use no exact-set scoring or method-count target. Tie material findings to evidence. Keep execution records, self-reports and unverified claims distinct. Write a qualitative review to REVIEW with: understanding, strengths, findings (observation/evidence/impact/alternative), defensible_alternatives, uncertainty, and limits. Do not edit actor evidence, inspect other reviewers or launch agents. Return only the review path.'''


def review_key(id, label, turn):
    return '/'.join([id, str(turn), label] if turn is not None else [id, label])


def reviewer(run, id, label, turn=None):
    run = Path(run).resolve()
    if label not in ('A', 'B'):
        raise ValueError('review label must be A or B')
    routing_file = run / 'review-routing.json'
    routing = read_json(routing_file) if routing_file.exists() else {}
    key = review_key(id, label, turn)
    if key in routing:
        raise FileExistsError('review assignment already exists')
    item = read_json(run / 'controller.json')['cases'][id]
    context, source = evidence_paths(item, turn)
    if not context.is_file():
        raise ValueError('turn must start before review')
    directory = run / 'reviewers' / uuid.uuid4().hex[:12]
    directory.mkdir(parents=True)
    initial = directory / 'initial.json'
    shutil.copyfile(context, initial)
    provisional = directory / 'provisional.md'
    prompt = STAGE_ONE.replace('PROVISIONAL', str(provisional)) + '\nInitial context: ' + str(initial)
    (directory / 'stage-one.txt').write_text(prompt, encoding='utf-8')
    write_json(directory / 'routing.json', {'evidence': str(source), 'initial_sha256': sha(initial.read_bytes())})
    routing[key] = {'directory': str(directory)}
    write_json(routing_file, routing)
    return {'directory': str(directory), 'prompt': prompt}


def reveal(directory):
    directory = Path(directory).resolve()
    if (directory / 'actor').exists() or (directory / 'stage-two.txt').exists():
        raise FileExistsError('actor evidence has already been revealed')
    if not (directory / 'provisional.md').is_file() or not (directory / 'provisional.md').read_text(encoding='utf-8').strip():
        raise ValueError('freeze provisional reasoning before revealing the actor')
    routing = read_json(directory / 'routing.json')
    source = Path(routing['evidence'])
    digest = verify(source)
    if any(sha(path.read_bytes()) != routing['initial_sha256']
           for path in (directory / 'initial.json', source / 'initial.json')):
        raise ValueError('initial review context changed')
    target = directory / 'actor'
    with tempfile.TemporaryDirectory(dir=directory) as staging:
        snapshot = Path(staging) / 'snapshot'
        shutil.copytree(source, snapshot)
        if verify(snapshot) != digest:
            raise ValueError('actor evidence changed during reveal')
        snapshot.rename(target)
    routing['provisional_sha256'] = sha((directory / 'provisional.md').read_bytes())
    routing['evidence_sha256'] = digest
    write_json(directory / 'routing.json', routing)
    review = directory / 'review.json'
    prompt = STAGE_TWO.replace('REVIEW', str(review)) + '\nActor observation: ' + str(target / 'observation.json') + '\nFrozen workspace: ' + str(target / 'workspace')
    (directory / 'stage-two.txt').write_text(prompt, encoding='utf-8')
    return prompt


def reviewed_evidence(source, routing, case, turn):
    digest = verify(source)
    initial = (source / 'initial.json').read_bytes()
    reviews = []
    for label in ('A', 'B'):
        directory = Path(routing[review_key(case, label, turn)]['directory'])
        route = read_json(directory / 'routing.json')
        provisional = (directory / 'provisional.md').read_bytes()
        if not provisional.strip() or route.get('provisional_sha256') != sha(provisional):
            raise ValueError('provisional review missing or changed after reveal')
        if sha(initial) != route['initial_sha256'] or (directory / 'initial.json').read_bytes() != initial:
            raise ValueError('initial review context changed after reveal')
        if route.get('evidence_sha256') != digest or verify(directory / 'actor') != digest:
            raise ValueError('actor evidence differs from the evidence disclosed to reviewer ' + label)
        reviews.append({'label': label, 'provisional': provisional.decode('utf-8'),
                        'review': read_json(directory / 'review.json'),
                        'stage_one_prompt': (directory / 'stage-one.txt').read_text(encoding='utf-8'),
                        'stage_two_prompt': (directory / 'stage-two.txt').read_text(encoding='utf-8'),
                        'provisional_frozen_before_reveal_hash': route['provisional_sha256'],
                        'initial_sha256': route['initial_sha256'], 'actor_evidence_sha256': digest})
    observation = read_json(source / 'observation.json')
    raw = []
    for call in observation['amh_calls']:
        raw.append({**call, 'stdout': (source / call['stdout_file']).read_text(encoding='utf-8'),
                    'stderr': (source / call['stderr_file']).read_text(encoding='utf-8'),
                    'stdin': (source / 'audit' / call['stdin_file']).read_text(encoding='utf-8') if call.get('stdin_file') else None})
    data = {'initial.json': json.loads(initial), 'observation.json': observation, 'calls.json': raw,
            MANIFEST: read_json(source / MANIFEST)}
    for result in reviews:
        data['review-' + result['label'] + '.json'] = result
    work = source / 'work-audit'
    if (work / 'events.jsonl').is_file():
        for line in (work / 'events.jsonl').read_text(encoding='utf-8').splitlines():
            record = json.loads(line)
            for channel in ('stdin', 'stdout', 'stderr'):
                record[channel] = (work / (record['id'] + '.' + channel)).read_text(encoding='utf-8', errors='replace')
            data['work/' + record['id'] + '.json'] = record
    if (source / 'policy-change.json').is_file():
        data['policy-change.json'] = read_json(source / 'policy-change.json')
    return data


def export_evidence(run, out):
    """Archive validated observations and reviews without inferring a semantic verdict."""
    run = Path(run).resolve()
    out = Path(out).resolve()
    controller = read_json(run / 'controller.json')
    routing = read_json(run / 'review-routing.json')
    prepared = []
    for id, item in controller['cases'].items():
        if 'turns' in item['case']:
            for turn in range(1, len(item['case']['turns']) + 1):
                dataset = Path(id) / 'turns' / str(turn)
                _, source = evidence_paths(item, turn)
                prepared.append((dataset, reviewed_evidence(source, routing, id, turn)))
        else:
            _, source = evidence_paths(item, None)
            prepared.append((Path(id), reviewed_evidence(source, routing, id, None)))
    out.mkdir(parents=True, exist_ok=False)
    for dataset, files in prepared:
        for name, value in files.items():
            write_json(out / dataset / name, portable(value, run))
    # Session isolation/model facts come from the controller, not from this file
    # exporter: it cannot observe how a person launched an external agent.
    session_file = run / 'session-provenance.json'
    session = read_json(session_file) if session_file.exists() else {}
    provenance = {'format': 'amh.qualitative-archive.v1', 'suite_sha256': controller['suite_sha256'],
                  'amh_sha256': controller['amh_sha256'], 'created_ns': controller['created_ns'],
                  'delivery': controller['delivery'], 'collected_cases': list(controller['cases']),
                  'reviewed_units': len(prepared),
                  'review_records': 2 * len(prepared), 'session_provenance': session,
                  'automatic_semantic_score': None,
                  'archive_notes': ['Paths normalized for portability; original fingerprints refer to original run bytes, not normalized replay inputs.',
                                    'Actor notes are self-reports; their capture method must be recorded by the controller.',
                                    'AMH command output is captured; arbitrary actor shell/read/edit events are not fully audited.',
                                    'Actor/reviewer access restrictions are instructional on a shared filesystem, not an OS sandbox.']}
    write_json(out / 'provenance.json', provenance)
    manifest = {p.relative_to(out).as_posix(): sha(p.read_bytes()) for p in sorted(out.rglob('*')) if p.is_file()}
    write_json(out / 'manifest.json', manifest)
    return provenance


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest='command', required=True)
    a = sub.add_parser('prepare'); a.add_argument('--amh', required=True); a.add_argument('--suite', default=str(DEFAULT_SUITE)); a.add_argument('--out', required=True); a.add_argument('--cases', nargs='+'); a.add_argument('--delivery', choices=['amh', 'agents-md'], default='amh')
    a = sub.add_parser('collect'); a.add_argument('--run', required=True); a.add_argument('--case', required=True); a.add_argument('--actor-note')
    a = sub.add_parser('reviewer'); a.add_argument('--run', required=True); a.add_argument('--case', required=True); a.add_argument('--label', required=True); a.add_argument('--turn', type=int)
    a = sub.add_parser('reveal'); a.add_argument('--directory', required=True)
    a = sub.add_parser('export'); a.add_argument('--run', required=True); a.add_argument('--out', required=True)
    args = p.parse_args()
    if args.command == 'prepare':
        c = prepare(args.amh, args.suite, args.out, args.cases, delivery=args.delivery)
        print(json.dumps({'run': str(Path(args.out).resolve()), 'assignments': {k:v['actor_prompt'] for k,v in c['cases'].items()}}, ensure_ascii=False))
    elif args.command == 'collect':
        result = collect(args.run, args.case, args.actor_note); print(json.dumps({'case': args.case, 'amh_calls': len(result['amh_calls']), 'semantic_verdict': None}))
    elif args.command == 'reviewer':
        print(json.dumps(reviewer(args.run, args.case, args.label, args.turn), ensure_ascii=False))
    elif args.command == 'reveal':
        print(reveal(args.directory))
    else:
        print(json.dumps(export_evidence(args.run, args.out), ensure_ascii=False))


if __name__ == '__main__':
    main()
